use std::io;

use serde::Serialize;
use serde::ser::{SerializeMap, SerializeSeq};

use super::{Value, utf16_cmp};

/// Emits a JSON tree with UTF-16 key ordering.
///
/// Amiss wire callers validate with `codec::to_value` or `json::parse` first.
/// Upstream numeric values retain Serde's encoding, which is not a general JCS number formatter.
#[must_use]
pub fn canonical(value: &Value) -> Vec<u8> {
    let mut out = String::new();
    stream(value, &mut out);
    out.into_bytes()
}

/// Counts canonical bytes without materializing them.
#[must_use]
pub fn canonical_length(value: &Value) -> u64 {
    let mut count = 0_u64;
    stream(
        value,
        &mut Callback(|piece: &str| {
            count = count.saturating_add(u64::try_from(piece.len()).unwrap_or(u64::MAX));
        }),
    );
    count
}

/// Receives canonical JSON in ordered pieces.
pub trait Sink {
    fn write(&mut self, piece: &str);
}

impl Sink for String {
    fn write(&mut self, piece: &str) {
        self.push_str(piece);
    }
}

pub(crate) struct Callback<F>(pub(crate) F);
impl<F: FnMut(&str)> Sink for Callback<F> {
    fn write(&mut self, piece: &str) {
        self.0(piece);
    }
}

struct SinkWriter<'a, S: ?Sized>(&'a mut S);
impl<S: Sink + ?Sized> io::Write for SinkWriter<'_, S> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0
            .write(std::str::from_utf8(bytes).map_err(io::Error::other)?);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Serializes a typed projection in its declared field order.
///
/// # Errors
///
/// The projection cannot be represented as JSON.
pub fn serialize<T: Serialize + ?Sized, S: Sink + ?Sized>(
    value: &T,
    sink: &mut S,
) -> Result<(), serde_json::Error> {
    serde_json::to_writer(SinkWriter(sink), value)
}

/// Streams a JSON tree with UTF-16 object ordering and Serde's JSON encoding.
pub fn stream<S: Sink + ?Sized>(value: &Value, sink: &mut S) {
    let _infallible = serde_json::to_writer(SinkWriter(sink), &Canonical(value));
}

/// Streams one string, including JSON quotes and escapes.
pub fn write_string<S: Sink + ?Sized>(sink: &mut S, value: &str) {
    let _infallible = serde_json::to_writer(SinkWriter(sink), value);
}

/// Borrows a JSON tree with canonical UTF-16 object ordering.
#[must_use]
pub fn canonical_view(value: &Value) -> impl Serialize + '_ {
    Canonical(value)
}

/// Streams canonical JSON directly into an I/O writer.
///
/// # Errors
///
/// The first output error, preserving its kind and source.
pub fn write_to<W: io::Write + ?Sized>(value: &Value, out: &mut W) -> io::Result<()> {
    serde_json::to_writer(out, &Canonical(value)).map_err(Into::into)
}

struct Canonical<'a>(&'a Value);
impl Serialize for Canonical<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.0 {
            Value::Null => serializer.serialize_unit(),
            Value::Bool(value) => serializer.serialize_bool(*value),
            Value::Number(value) => value.serialize(serializer),
            Value::String(value) => serializer.serialize_str(value),
            Value::Array(values) => {
                let mut seq = serializer.serialize_seq(Some(values.len()))?;
                for value in values {
                    seq.serialize_element(&Self(value))?;
                }
                seq.end()
            }
            Value::Object(values) => {
                let mut map = serializer.serialize_map(Some(values.len()))?;
                if values
                    .keys()
                    .zip(values.keys().skip(1))
                    .all(|(left, right)| utf16_cmp(left, right).is_le())
                {
                    for (key, value) in values {
                        map.serialize_entry(key, &Self(value))?;
                    }
                } else {
                    let mut ordered: Vec<_> = values.iter().collect();
                    ordered.sort_unstable_by(|(left, _), (right, _)| utf16_cmp(left, right));
                    for (key, value) in ordered {
                        map.serialize_entry(key, &Self(value))?;
                    }
                }
                map.end()
            }
        }
    }
}
