// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-pdf-writer / writer
//
// Object-ID management, XRef table construction, and top-level document
// assembly. `Value`/`Stream` byte serialization lives in `value`.

use crate::value::serialize_value;
use crate::{hex_byte, ObjectId, Result, Stream, Value, WriteError};
use alloc::string::String;
use alloc::vec::Vec;

/// Output options for [`Writer`].
#[derive(Debug, Clone, PartialEq)]
pub struct WriteOptions {
    /// Pack ordinary objects into compressed object streams (`/Type /ObjStm`).
    /// When enabled, non-stream objects are grouped into batches (see
    /// `object_stream_batch`), each written as a Flate-compressed stream, and
    /// their XRef entries become type-2 (compressed) entries. Data streams are
    /// never packed. Default `false`.
    pub use_object_streams: bool,
    /// Maximum number of objects per compressed object stream. Default `100`.
    pub object_stream_batch: usize,
    /// PDF header version line. Default `"1.7"`.
    pub header_version: &'static str,
    /// Emit an auto-generated `/ID` in the trailer when no explicit one was
    /// set. Default `false` so output stays byte-for-byte deterministic.
    pub generate_trailer_id: bool,
}

impl Default for WriteOptions {
    fn default() -> Self {
        Self {
            use_object_streams: false,
            object_stream_batch: 100,
            header_version: "1.7",
            generate_trailer_id: false,
        }
    }
}

#[derive(Debug, Clone)]
enum ObjectKind {
    Value(Value),
    Stream(Stream),
}

/// Where an object number ends up referenced from the XRef table.
#[derive(Debug, Clone, Copy, PartialEq)]
enum XrefMode {
    Free,
    TopLevel,
    Packed { container_num: u32, index: u32 },
}

/// One row of the cross-reference table.
#[derive(Debug, Clone, Copy)]
enum XrefEntry {
    Free { next: u32, gen: u16 },
    TopLevel { offset: u64, gen: u16 },
    Packed { container_num: u32, index: u32 },
}

/// A low-level PDF document assembler.
///
/// Objects are numbered sequentially from 1 on allocation. References by
/// number are resolved at serialization time, so forward references (and
/// circular structures like page trees) work naturally.
#[derive(Debug)]
pub struct Writer {
    options: WriteOptions,
    objects: Vec<Option<ObjectKind>>,
    root: Option<ObjectId>,
    info: Option<ObjectId>,
    trailer_id: Option<[String; 2]>,
    trailer_extra: Vec<(String, Value)>,
}

impl Writer {
    pub fn new() -> Self {
        Self::with_options(WriteOptions::default())
    }

    pub fn with_options(options: WriteOptions) -> Self {
        Self {
            options,
            objects: Vec::new(),
            root: None,
            info: None,
            trailer_id: None,
            trailer_extra: Vec::new(),
        }
    }

    /// Reserve an object number without defining it. The slot stays a free
    /// XRef entry until [`Writer::define`] fills it.
    pub fn alloc(&mut self) -> ObjectId {
        self.objects.push(None);
        ObjectId::new(self.objects.len() as u32, 0)
    }

    /// Allocate and define an object in one step.
    pub fn add(&mut self, value: Value) -> ObjectId {
        let id = self.alloc();
        self.define(id, value).expect("fresh id is in range");
        id
    }

    /// Define the value for a previously reserved object number.
    pub fn define(&mut self, id: ObjectId, value: Value) -> Result<()> {
        let slot = self
            .objects
            .get_mut(id.num as usize - 1)
            .ok_or(WriteError::UndefinedObject { num: id.num })?;
        *slot = Some(ObjectKind::Value(value));
        Ok(())
    }

    /// Allocate and define a stream object. The `/Length` (and, when
    /// `Stream::compress`, the `/Filter`) entries are computed at
    /// serialization time.
    pub fn add_stream(&mut self, stream: Stream) -> ObjectId {
        let id = self.alloc();
        self.objects[id.num as usize - 1] = Some(ObjectKind::Stream(stream));
        id
    }

    pub fn set_root(&mut self, id: ObjectId) {
        self.root = Some(id);
    }

    pub fn set_info(&mut self, id: ObjectId) {
        self.info = Some(id);
    }

    pub fn set_trailer_id(&mut self, id: [String; 2]) {
        self.trailer_id = Some(id);
    }

    /// Add an arbitrary trailer dictionary entry (e.g. `/Encrypt`).
    pub fn add_trailer_entry(&mut self, key: impl Into<String>, value: Value) {
        self.trailer_extra.push((key.into(), value));
    }

    /// Serialize the whole document, returning the final PDF bytes.
    pub fn finish(&self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        self.write_document(&mut out)?;
        Ok(out)
    }

    /// Serialize the document into any `Write` sink.
    pub fn write_to(&self, out: &mut impl std::io::Write) -> std::io::Result<()> {
        let bytes = self.finish().map_err(std::io::Error::other)?;
        out.write_all(&bytes)
    }

    /// Serialize and write the document to a file.
    pub fn save(&self, path: impl AsRef<std::path::Path>) -> std::io::Result<()> {
        let bytes = self.finish().map_err(std::io::Error::other)?;
        std::fs::write(path, bytes)
    }

    // ---------------------------------------------------------------------
    // Serialization
    // ---------------------------------------------------------------------

    fn write_document(&self, out: &mut Vec<u8>) -> Result<()> {
        let packing = self.compute_packing();
        let containers = if self.options.use_object_streams {
            build_containers(&self.objects, &packing, self.options.object_stream_batch)?
        } else {
            Vec::new()
        };

        // Header. The binary comment line stops transfer channels from
        // mangling 8-bit payload bytes.
        out.extend_from_slice(format!("%PDF-{}\n", self.options.header_version).as_bytes());
        out.extend_from_slice(b"%\xE2\xE3\xCF\xD3\n");

        // Top-level objects, in numeric order.
        let mut offsets: Vec<Option<u64>> = vec![None; self.objects.len()];
        for (idx, kind) in self.objects.iter().enumerate() {
            let num = idx as u32 + 1;
            let offset = out.len() as u64;
            match kind {
                Some(ObjectKind::Value(v)) => {
                    if !matches!(packing[idx], XrefMode::TopLevel) {
                        continue; // value lives inside a compressed object stream
                    }
                    write_object(out, num, 0, |buf| serialize_value(v, buf))?;
                    offsets[idx] = Some(offset);
                }
                Some(ObjectKind::Stream(s)) => {
                    write_object(out, num, 0, |buf| write_stream_payload(buf, s))?;
                    offsets[idx] = Some(offset);
                }
                None => {}
            }
        }

        // Compressed object stream containers, at their own object numbers.
        let mut container_offsets: Vec<(u32, u64)> = Vec::new();
        for (container_num, stream) in &containers {
            let offset = out.len() as u64;
            write_object(out, *container_num, 0, |buf| {
                write_stream_payload(buf, stream)
            })?;
            container_offsets.push((*container_num, offset));
        }

        // XRef rows, in object-number order.
        let size = self.objects.len() + 1 + containers.len();
        let mut entries: Vec<(u32, XrefEntry)> = Vec::with_capacity(size);
        entries.push((
            0,
            XrefEntry::Free {
                next: 0,
                gen: 65535,
            },
        ));
        for (idx, mode) in packing.iter().enumerate() {
            let num = idx as u32 + 1;
            let entry = match mode {
                XrefMode::Free => XrefEntry::Free {
                    next: 0,
                    gen: 65535,
                },
                XrefMode::TopLevel => {
                    let offset = offsets[idx].ok_or(WriteError::UndefinedObject { num })?;
                    XrefEntry::TopLevel { offset, gen: 0 }
                }
                XrefMode::Packed {
                    container_num,
                    index,
                } => XrefEntry::Packed {
                    container_num: *container_num,
                    index: *index,
                },
            };
            entries.push((num, entry));
        }
        for (container_num, offset) in container_offsets {
            entries.push((container_num, XrefEntry::TopLevel { offset, gen: 0 }));
        }

        let xref_offset = out.len() as u64;
        write_xref(out, &entries)?;

        // Trailer.
        let mut trailer: Vec<(String, Value)> = Vec::new();
        trailer.push(("Size".into(), Value::Integer(size as i64)));
        if let Some(root) = self.root {
            trailer.push(("Root".into(), Value::Reference(root)));
        }
        if let Some(info) = self.info {
            trailer.push(("Info".into(), Value::Reference(info)));
        }
        if let Some(id) = &self.trailer_id {
            trailer.push((
                "ID".into(),
                Value::Array(vec![
                    Value::HexString(id[0].as_bytes().to_vec()),
                    Value::HexString(id[1].as_bytes().to_vec()),
                ]),
            ));
        } else if self.options.generate_trailer_id {
            trailer.push(("ID".into(), generated_id()));
        }
        trailer.extend(self.trailer_extra.iter().cloned());

        out.extend_from_slice(b"trailer\n");
        serialize_value(&Value::Dict(trailer), out)?;
        out.extend_from_slice(b"\nstartxref\n");
        out.extend_from_slice(xref_offset.to_string().as_bytes());
        out.extend_from_slice(b"\n%%EOF\n");
        Ok(())
    }

    /// Decide the XRef mode of every object number.
    fn compute_packing(&self) -> Vec<XrefMode> {
        let batch = self.options.object_stream_batch.max(1);
        let mut packing = Vec::with_capacity(self.objects.len());

        if !self.options.use_object_streams {
            for kind in &self.objects {
                packing.push(match kind {
                    Some(_) => XrefMode::TopLevel,
                    None => XrefMode::Free,
                });
            }
            return packing;
        }

        // Object-stream mode: every Value object is packed into one of
        // `ceil(count / batch)` containers numbered right after the last
        // user object. Containers fill sequentially in object-number order.
        let packed_count = self
            .objects
            .iter()
            .filter(|k| matches!(k, Some(ObjectKind::Value(_))))
            .count();
        let container_count = packed_count.div_ceil(batch);
        let mut container_num = self.objects.len() as u32 + 1;
        let mut index_in_container = 0usize;

        for kind in &self.objects {
            let mode = match kind {
                Some(ObjectKind::Value(_)) => {
                    let m = XrefMode::Packed {
                        container_num,
                        index: index_in_container as u32,
                    };
                    index_in_container += 1;
                    if index_in_container == batch {
                        container_num += 1;
                        index_in_container = 0;
                    }
                    m
                }
                Some(ObjectKind::Stream(_)) => XrefMode::TopLevel,
                None => XrefMode::Free,
            };
            packing.push(mode);
        }
        debug_assert_eq!(
            container_num,
            self.objects.len() as u32 + 1 + container_count as u32
        );
        packing
    }
}

impl Default for Writer {
    fn default() -> Self {
        Self::new()
    }
}

/// Write `{num} {gen} obj\n<body>\nendobj\n`, calling `body` once between the
/// object header and the trailer.
fn write_object(
    out: &mut Vec<u8>,
    num: u32,
    gen: u16,
    body: impl FnOnce(&mut Vec<u8>) -> Result<()>,
) -> Result<()> {
    out.extend_from_slice(format!("{num} {gen} obj\n").as_bytes());
    body(out)?;
    out.extend_from_slice(b"\nendobj\n");
    Ok(())
}

/// Serialize a stream object body: dictionary (with computed `/Length` and,
/// when compressing, `/Filter`), `stream` keyword, payload, `endstream`.
fn write_stream_payload(out: &mut Vec<u8>, s: &Stream) -> Result<()> {
    let mut dict = s.dict.clone();
    dict.retain(|(k, _)| k != "Length");
    let payload = if s.compress {
        dict.retain(|(k, _)| k != "Filter");
        dict.push(("Filter".into(), Value::Name("FlateDecode".into())));
        deflate(&s.data)
    } else {
        s.data.clone()
    };
    dict.push(("Length".into(), Value::Integer(payload.len() as i64)));

    out.extend_from_slice(b"<<");
    for (key, val) in &dict {
        out.extend_from_slice(b" /");
        for &b in key.as_bytes() {
            crate::escape_name_byte(out, b);
        }
        out.push(b' ');
        serialize_value(val, out)?;
    }
    out.extend_from_slice(b" >>\nstream\n");
    out.extend_from_slice(&payload);
    out.extend_from_slice(b"\nendstream");
    Ok(())
}

/// Build the `Value`-only subset of objects into compressed `/ObjStm`
/// container streams. Returns `(container object number, stream)` pairs.
fn build_containers(
    objects: &[Option<ObjectKind>],
    packing: &[XrefMode],
    _batch: usize,
) -> Result<Vec<(u32, Stream)>> {
    let mut grouped: Vec<(u32, Vec<(u32, Value)>)> = Vec::new();
    for (idx, mode) in packing.iter().enumerate() {
        if let XrefMode::Packed { container_num, .. } = mode {
            if let Some(ObjectKind::Value(v)) = &objects[idx] {
                match grouped.last_mut() {
                    Some((c, items)) if *c == *container_num => {
                        items.push((idx as u32 + 1, v.clone()));
                    }
                    _ => grouped.push((*container_num, vec![(idx as u32 + 1, v.clone())])),
                }
            }
        }
    }

    let mut containers: Vec<(u32, Stream)> = Vec::with_capacity(grouped.len());
    for (container_num, items) in grouped {
        let data = build_object_stream_data(&items)?;
        let dict = vec![
            ("Type".into(), Value::Name("ObjStm".into())),
            ("N".into(), Value::Integer(items.len() as i64)),
            (
                "First".into(),
                Value::Integer(streams_first_offset(&data) as i64),
            ),
        ];
        containers.push((
            container_num,
            Stream {
                dict,
                data,
                compress: true,
            },
        ));
    }
    Ok(containers)
}

/// The byte offset of the first object within an object-stream payload (i.e.
/// end of the header line including its newline).
fn streams_first_offset(data: &[u8]) -> usize {
    data.iter()
        .position(|&b| b == b'\n')
        .map(|p| p + 1)
        .unwrap_or(data.len())
}

/// Serialize packed objects into the raw object-stream payload: a header of
/// `objnum offset` pairs followed by the concatenated object bodies.
fn build_object_stream_data(items: &[(u32, Value)]) -> Result<Vec<u8>> {
    let mut body = Vec::new();
    let mut body_offsets = Vec::with_capacity(items.len());
    for (_, value) in items {
        body_offsets.push(body.len());
        serialize_value(value, &mut body)?;
        body.push(b' ');
    }

    // Header offsets are measured from the start of the payload, which begins
    // with the header itself, so the header length and the offsets are
    // mutually dependent. Iterate to a fixed point (converges in a couple of
    // passes because only digit counts change).
    let mut header: Vec<u8> = Vec::new();
    loop {
        let prev_len = header.len();
        header.clear();
        for (i, (num, _)) in items.iter().enumerate() {
            let offset = prev_len + 1 + body_offsets[i];
            header.extend_from_slice(num.to_string().as_bytes());
            header.push(b' ');
            header.extend_from_slice(offset.to_string().as_bytes());
            header.push(b' ');
        }
        if header.len() == prev_len {
            break;
        }
    }

    let mut data = header;
    data.push(b'\n');
    data.extend_from_slice(&body);
    Ok(data)
}

/// Zlib-compress `data` (PDF `FlateDecode`).
fn deflate(data: &[u8]) -> Vec<u8> {
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    use std::io::Write;

    let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
    enc.write_all(data).expect("writing into a Vec cannot fail");
    enc.finish()
        .expect("finishing a Vec-backed encoder cannot fail")
}

/// Write the XRef table (`xref`, subsections, rows) followed by nothing —
/// the caller appends `startxref`.
fn write_xref(out: &mut Vec<u8>, entries: &[(u32, XrefEntry)]) -> Result<()> {
    out.extend_from_slice(b"xref\n");
    out.extend_from_slice(format!("0 {}\n", entries.len()).as_bytes());
    for (_, entry) in entries {
        let line = match entry {
            XrefEntry::Free { next, gen } => format!("{next:010} {gen:05} f \n"),
            XrefEntry::TopLevel { offset, gen } => {
                if *offset >= 10_000_000_000 {
                    return Err(WriteError::XrefFieldTooLarge {
                        what: "offset",
                        value: *offset,
                    });
                }
                format!("{offset:010} {gen:05} n \n")
            }
            XrefEntry::Packed {
                container_num,
                index,
            } => format!("{container_num:010} {index:05} n \n"),
        };
        out.extend_from_slice(line.as_bytes());
    }
    Ok(())
}

fn hex_str(bytes: &[u8]) -> Value {
    let mut out = Vec::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.extend_from_slice(&hex_byte(b));
    }
    Value::HexString(out)
}

/// A non-cryptographic, time-seeded trailer ID. Deterministic output is the
/// default; set an explicit [`Writer::set_trailer_id`] for reproducible
/// builds.
fn generated_id() -> Value {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let a = prng_fill(seed as u64 ^ 0x9E37_79B9_7F4A_7C15);
    let b = prng_fill(seed as u64 ^ 0xC2B2_AE3D_27D4_EB4F);
    Value::Array(vec![hex_str(&a), hex_str(&b)])
}

fn prng_fill(mut x: u64) -> [u8; 16] {
    let mut out = [0u8; 16];
    for byte in out.iter_mut() {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *byte = x as u8;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Value;

    fn minimal_writer() -> (Writer, Vec<u8>) {
        let mut w = Writer::new();
        let catalog = w.add(Value::dict([
            ("Type", Value::name("Catalog")),
            ("Pages", Value::reference((2, 0))),
        ]));
        let pages = w.add(Value::dict([
            ("Type", Value::name("Pages")),
            ("Kids", Value::array([])),
            ("Count", Value::Integer(0)),
        ]));
        w.set_root(catalog);
        w.set_info(pages);
        let bytes = w.finish().unwrap();
        (w, bytes)
    }

    #[test]
    fn produces_header_and_eof() {
        let (_, bytes) = minimal_writer();
        assert!(bytes.starts_with(b"%PDF-1.7\n"));
        assert!(bytes.windows(5).any(|w| w == b"%%EOF"));
        assert!(bytes.windows(9).any(|w| w == b"startxref"));
    }

    #[test]
    fn xref_offsets_are_consistent() {
        let (_, bytes) = minimal_writer();
        // startxref value must point at the literal `xref` keyword. It sits on
        // the line after `startxref`.
        let startxref_pos = bytes
            .windows(9)
            .position(|w| w == b"startxref")
            .expect("startxref present");
        let num_start = startxref_pos + 9;
        let num_start = num_start
            + bytes[num_start..]
                .iter()
                .position(|b| b.is_ascii_digit())
                .expect("offset digits present");
        let num_end = bytes[num_start..]
            .iter()
            .position(|b| !b.is_ascii_digit())
            .map(|p| num_start + p)
            .unwrap();
        let xref_offset: usize = std::str::from_utf8(&bytes[num_start..num_end])
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(&bytes[xref_offset..xref_offset + 4], b"xref");
    }

    #[test]
    fn writes_object_streams_when_requested() {
        let mut w = Writer::with_options(WriteOptions {
            use_object_streams: true,
            object_stream_batch: 2,
            ..Default::default()
        });
        for i in 0..5 {
            w.add(Value::dict([("N", Value::Integer(i))]));
        }
        let catalog = w.add(Value::dict([("Type", Value::name("Catalog"))]));
        w.set_root(catalog);
        let bytes = w.finish().unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("/Type /ObjStm"), "no ObjStm emitted");
        assert_eq!(text.matches("/Type /ObjStm").count(), 3); // ceil(6/2) = 3 containers
                                                              // No top-level `N obj` for the packed objects (only containers + root).
        assert!(text.contains("7 0 obj")); // root catalog
    }

    #[test]
    fn compressed_stream_gets_filter_and_length() {
        let mut w = Writer::new();
        let mut s = Stream::new(b"hello hello hello hello".to_vec());
        s.compress();
        let _id = w.add_stream(s);
        let catalog = w.add(Value::dict([("Type", Value::name("Catalog"))]));
        w.set_root(catalog);
        let bytes = w.finish().unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("/Filter /FlateDecode"));
        assert!(text.contains("/Length"));
    }

    #[test]
    fn undefined_allocated_object_is_free_entry() {
        let mut w = Writer::new();
        let _reserved = w.alloc();
        let catalog = w.add(Value::dict([("Type", Value::name("Catalog"))]));
        w.set_root(catalog);
        let bytes = w.finish().unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("0000000000 65535 f "));
    }

    #[test]
    fn explicit_trailer_id_round_trips() {
        let mut w = Writer::new();
        let catalog = w.add(Value::dict([("Type", Value::name("Catalog"))]));
        w.set_root(catalog);
        w.set_trailer_id([
            "0123456789ABCDEF0123456789ABCDEF".into(),
            "FEDCBA9876543210FEDCBA9876543210".into(),
        ]);
        let bytes = w.finish().unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains(
            "/ID [<0123456789ABCDEF0123456789ABCDEF> <FEDCBA9876543210FEDCBA9876543210>]"
        ));
    }

    #[test]
    fn empty_document_is_serializable() {
        let w = Writer::new();
        let bytes = w.finish().unwrap();
        assert!(bytes.starts_with(b"%PDF-"));
        assert!(bytes.windows(5).any(|w| w == b"%%EOF"));
    }
}
