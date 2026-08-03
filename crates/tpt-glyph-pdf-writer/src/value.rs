// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-pdf-writer / value
//
// The low-level PDF object model (`ObjectId`, `Value`, `Stream`) and its
// byte serialization. Serialization appends directly into a caller-provided
// buffer — no intermediate allocations.

use crate::{escape_name_byte, hex_byte, Result, WriteError};
use alloc::string::String;
use alloc::vec::Vec;

/// A reference to an indirect object: `(object number, generation)`.
///
/// Generation is almost always `0`; the field is kept for fidelity with the
/// PDF object model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObjectId {
    pub num: u32,
    pub gen: u16,
}

impl ObjectId {
    pub const fn new(num: u32, gen: u16) -> Self {
        Self { num, gen }
    }
}

/// A PDF value, serializable to the standard object syntax.
///
/// Strings are raw bytes; the serializer chooses literal-string syntax when
/// every byte is printable ASCII and falls back to hex-string syntax
/// otherwise, so arbitrary binary data round-trips exactly.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Boolean(bool),
    Integer(i64),
    Real(f64),
    Name(String),
    String(Vec<u8>),
    HexString(Vec<u8>),
    Array(Vec<Value>),
    /// An ordered dictionary. Keys carry no leading `/`.
    Dict(Vec<(String, Value)>),
    Reference(ObjectId),
}

impl Value {
    pub fn name(name: impl Into<String>) -> Self {
        Value::Name(name.into())
    }

    pub fn string(bytes: impl Into<Vec<u8>>) -> Self {
        Value::String(bytes.into())
    }

    pub fn reference(id: impl Into<ObjectId>) -> Self {
        Value::Reference(id.into())
    }

    pub fn array(items: impl IntoIterator<Item = Value>) -> Self {
        Value::Array(items.into_iter().collect())
    }

    pub fn dict(items: impl IntoIterator<Item = (impl Into<String>, Value)>) -> Self {
        Value::Dict(items.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }
}

impl From<ObjectId> for Value {
    fn from(id: ObjectId) -> Self {
        Value::Reference(id)
    }
}

impl From<(u32, u16)> for ObjectId {
    fn from((num, gen): (u32, u16)) -> Self {
        ObjectId::new(num, gen)
    }
}

/// A stream object: a dictionary plus raw (undecoded) bytes.
#[derive(Debug, Clone, PartialEq)]
pub struct Stream {
    pub dict: Vec<(String, Value)>,
    pub data: Vec<u8>,
    /// Compress `data` with `FlateDecode` on output. The `/Filter` and
    /// `/Length` entries are written automatically; a pre-existing `/Length`
    /// is overwritten.
    pub compress: bool,
}

impl Stream {
    pub fn new(data: Vec<u8>) -> Self {
        Self {
            dict: Vec::new(),
            data,
            compress: false,
        }
    }

    pub fn with_dict(dict: Vec<(String, Value)>, data: Vec<u8>) -> Self {
        Self {
            dict,
            data,
            compress: false,
        }
    }

    pub fn compressed(data: Vec<u8>) -> Self {
        Self {
            dict: Vec::new(),
            data,
            compress: true,
        }
    }

    pub fn compress(&mut self) {
        self.compress = true;
    }

    /// Append a key/value pair to the stream dictionary.
    pub fn with_entry(mut self, key: impl Into<String>, value: Value) -> Self {
        self.dict.push((key.into(), value));
        self
    }
}

/// Serialize `value` into `out`, following indirect references when `resolve`
/// is `Some` (used by the writer to emit objects they point at).
pub(crate) fn serialize_value(value: &Value, out: &mut Vec<u8>) -> Result<()> {
    match value {
        Value::Null => out.extend_from_slice(b"null"),
        Value::Boolean(b) => out.extend_from_slice(if *b { b"true" } else { b"false" }),
        Value::Integer(i) => {
            let mut buf = itoa_like(*i);
            out.append(&mut buf);
        }
        Value::Real(f) => {
            if !f.is_finite() {
                return Err(WriteError::NonFiniteReal(*f));
            }
            let mut buf = ftoa_like(*f);
            out.append(&mut buf);
        }
        Value::Name(n) => {
            out.push(b'/');
            for &b in n.as_bytes() {
                escape_name_byte(out, b);
            }
        }
        Value::String(bytes) => {
            if bytes.iter().all(|&b| (0x20..=0x7E).contains(&b)) {
                out.push(b'(');
                for &b in bytes {
                    match b {
                        b'(' => out.extend_from_slice(b"\\("),
                        b')' => out.extend_from_slice(b"\\)"),
                        b'\\' => out.extend_from_slice(b"\\\\"),
                        _ => out.push(b),
                    }
                }
                out.push(b')');
            } else {
                serialize_hex_string(bytes, out);
            }
        }
        Value::HexString(hex) => {
            // Content is emitted verbatim between `<` and `>`; callers supply
            // already-hex-encoded bytes (e.g. trailer `/ID` values).
            out.push(b'<');
            out.extend_from_slice(hex);
            out.push(b'>');
        }
        Value::Array(items) => {
            out.push(b'[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(b' ');
                }
                serialize_value(item, out)?;
            }
            out.push(b']');
        }
        Value::Dict(entries) => {
            out.extend_from_slice(b"<<");
            for (key, val) in entries {
                out.push(b' ');
                out.push(b'/');
                for &b in key.as_bytes() {
                    escape_name_byte(out, b);
                }
                out.push(b' ');
                serialize_value(val, out)?;
            }
            out.extend_from_slice(b" >>");
        }
        Value::Reference(id) => {
            out.extend_from_slice(&itoa_u64_like(id.num as u64));
            out.push(b' ');
            out.extend_from_slice(&itoa_u64_like(id.gen as u64));
            out.extend_from_slice(b" R");
        }
    }
    Ok(())
}

fn serialize_hex_string(bytes: &[u8], out: &mut Vec<u8>) {
    out.push(b'<');
    for &b in bytes {
        out.extend_from_slice(&hex_byte(b));
    }
    out.push(b'>');
}

/// `u64` → ASCII decimal without intermediate allocation.
fn itoa_u64_like(mut n: u64) -> alloc::vec::Vec<u8> {
    let mut buf = alloc::vec::Vec::new();
    if n == 0 {
        buf.push(b'0');
        return buf;
    }
    while n > 0 {
        buf.push(b'0' + (n % 10) as u8);
        n /= 10;
    }
    buf.reverse();
    buf
}

/// `i64` → ASCII decimal without intermediate allocation.
fn itoa_like(n: i64) -> alloc::vec::Vec<u8> {
    let mut buf = alloc::vec::Vec::new();
    let neg = n < 0;
    let mag = n.unsigned_abs();
    buf.extend_from_slice(&itoa_u64_like(mag));
    if neg {
        buf.insert(0, b'-');
    }
    buf
}

/// `f64` → shortest ASCII decimal (never scientific notation). Reuses the
/// standard library's `Display` formatting, which for `f64` prints the
/// shortest round-trip representation without an exponent.
fn ftoa_like(f: f64) -> alloc::vec::Vec<u8> {
    alloc::format!("{f}").into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_id_tuple_conversion() {
        let id: ObjectId = (3, 0).into();
        assert_eq!(id.num, 3);
        assert_eq!(id.gen, 0);
    }

    #[test]
    fn value_from_object_id() {
        let id = ObjectId::new(7, 0);
        let v: Value = id.into();
        assert_eq!(v, Value::Reference(id));
    }

    #[test]
    fn serialize_scalars() {
        let cases: &[(Value, &[u8])] = &[
            (Value::Null, b"null"),
            (Value::Boolean(true), b"true"),
            (Value::Integer(42), b"42"),
            (Value::Integer(-7), b"-7"),
            (Value::Real(1.5), b"1.5"),
            (Value::Name("Catalog".into()), b"/Catalog"),
            (Value::String(b"hi".to_vec()), b"(hi)"),
            (Value::String(vec![0x00, 0xFF]), b"<00FF>"),
        ];
        for (v, expect) in cases {
            let mut out = Vec::new();
            serialize_value(v, &mut out).unwrap();
            assert_eq!(out, *expect, "value {v:?}");
        }
    }

    #[test]
    fn serialize_escapes_name_bytes() {
        let mut out = Vec::new();
        serialize_value(&Value::Name("a b#c(1)".into()), &mut out).unwrap();
        assert_eq!(out, b"/a#20b#23c#281#29");
    }

    #[test]
    fn serialize_array_and_dict() {
        let v = Value::Array(vec![Value::Integer(1), Value::Name("x".into())]);
        let mut out = Vec::new();
        serialize_value(&v, &mut out).unwrap();
        assert_eq!(out, b"[1 /x]");

        let v = Value::Dict(vec![("Type".into(), Value::Name("Page".into()))]);
        let mut out = Vec::new();
        serialize_value(&v, &mut out).unwrap();
        assert_eq!(out, b"<< /Type /Page >>");
    }

    #[test]
    fn serialize_reference() {
        let mut out = Vec::new();
        serialize_value(&Value::Reference(ObjectId::new(5, 0)), &mut out).unwrap();
        assert_eq!(out, b"5 0 R");
    }

    #[test]
    fn non_finite_real_is_error() {
        let mut out = Vec::new();
        let err = serialize_value(&Value::Real(f64::NAN), &mut out).unwrap_err();
        assert!(matches!(err, WriteError::NonFiniteReal(_)));
    }
}
