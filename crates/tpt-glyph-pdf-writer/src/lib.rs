// SPDX-License-Identifier: MIT OR Apache-2.0
//
// TPT Glyph — tpt-glyph-pdf-writer
//
// A dependency-light, low-level PDF serialization crate. It knows nothing
// about pages, content streams, or fonts: it manages object numbers, emits
// objects/streams, and assembles cross-reference tables and trailers — the
// fragile byte-level plumbing of a PDF. Higher-level crates
// (`tpt-glyph-pdf-editor`, the CLI demo) build PDF semantics on top of it.

//! # tpt-glyph-pdf-writer
//!
//! Low-level, append-only PDF serialization. The crate manages object IDs and
//! cross-reference tables and serializes objects, streams, and (optionally)
//! compressed object streams. It has a single non-`std` dependency
//! (`flate2`, for `FlateDecode` streams) and performs no intermediate
//! allocation while serializing: the output buffer is the only allocation.
//!
//! The typical flow is:
//!
//! 1. [`Writer::new`], optionally with [`WriteOptions`] to enable compressed
//!    object streams.
//! 2. Allocate/define objects ([`Writer::add`], [`Writer::add_stream`]),
//!    taking care to wire up `/Root` and `/Info` trailer references
//!    ([`Writer::set_root`], [`Writer::set_info`]).
//! 3. Serialize with [`Writer::finish`] (or stream to any `Write` via
//!    [`Writer::write_to`]).
//!
//! ```
//! use tpt_glyph_pdf_writer::{Value, Writer};
//!
//! let mut w = Writer::new();
//! // 1 0 obj /Type /Catalog /Pages 2 0 R
//! let catalog = w.add(Value::dict([
//!     ("Type", Value::name("Catalog")),
//!     ("Pages", Value::reference((2, 0))),
//! ]));
//! // 2 0 obj /Type /Pages /Kids [] /Count 0
//! let pages = w.add(Value::dict([
//!     ("Type", Value::name("Pages")),
//!     ("Kids", Value::array([])),
//!     ("Count", Value::Integer(0)),
//! ]));
//! w.set_root(catalog);
//! w.set_info(pages);
//!
//! let bytes = w.finish().unwrap();
//! assert!(bytes.starts_with(b"%PDF-"));
//! assert!(bytes.windows(5).any(|w| w == b"%%EOF"));
//! ```
//!
//! `tpt-glyph-pdf-writer` is deliberately the bottom of the stack: it has one
//! dependency (`flate2`, for `FlateDecode` streams) and serializes objects
//! directly into a caller-provided buffer, so the only meaningful allocation
//! while writing is the output buffer itself.

extern crate alloc;
use alloc::vec::Vec;

mod value;
pub mod writer;

pub use value::{ObjectId, Stream, Value};
pub use writer::{WriteOptions, Writer};

/// Errors produced while serializing a PDF.
#[derive(Debug, Clone, PartialEq)]
pub enum WriteError {
    /// A real-number value was not finite (NaN or ±inf), which PDF cannot represent.
    NonFiniteReal(f64),
    /// A referenced object number was never defined.
    UndefinedObject { num: u32 },
    /// A value exceeded a fixed-width XRef field (offsets beyond 9,999,999,999).
    XrefFieldTooLarge { what: &'static str, value: u64 },
}

impl core::fmt::Display for WriteError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            WriteError::NonFiniteReal(v) => write!(f, "cannot serialize non-finite real {v}"),
            WriteError::UndefinedObject { num } => write!(f, "object {num} was never defined"),
            WriteError::XrefFieldTooLarge { what, value } => {
                write!(f, "{what} {value} does not fit the 10-digit XRef field")
            }
        }
    }
}

impl std::error::Error for WriteError {}

/// Serialization result type for the writer crate.
pub type Result<T> = core::result::Result<T, WriteError>;

/// Append the `#xx`-escaped form of a name byte to `out`.
pub(crate) fn escape_name_byte(out: &mut Vec<u8>, b: u8) {
    let safe = b.is_ascii_alphanumeric()
        || matches!(b, b'!' | b'\'' | b'$' | b'&' | b'*' | b'+' | b'-' | b'.' | b'_' | b'|' | b'~');
    if safe {
        out.push(b);
    } else {
        out.push(b'#');
        out.extend_from_slice(&hex_byte(b));
    }
}

/// The uppercase hex digits for `b`.
pub(crate) fn hex_byte(b: u8) -> [u8; 2] {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    [HEX[(b >> 4) as usize], HEX[(b & 0x0F) as usize]]
}
