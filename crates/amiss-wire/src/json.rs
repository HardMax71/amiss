use std::cmp::Ordering;

mod profile;
mod strict;
mod write;

pub(crate) use profile::check as check_profile;
pub use serde_json::{Map, Value};
pub(crate) use strict::decode as strict_value;
pub use strict::{Error, ErrorKind, parse, parse_upstream};
pub(crate) use write::Callback;
pub use write::{
    Sink, canonical, canonical_length, canonical_view, serialize, stream, write_string, write_to,
};

pub const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

/// Convenience projections and lookups on Serde's JSON tree.
pub trait ValueExt: Sized {
    fn string(value: impl Into<String>) -> Self;
    fn array(values: Vec<Self>) -> Self;
    fn object(values: Vec<(String, Self)>) -> Self;
    fn member(&self, name: &str) -> Option<&Self>;
    fn text(&self, name: &str) -> Option<&str>;
}

impl ValueExt for Value {
    fn string(value: impl Into<String>) -> Self {
        Self::String(value.into())
    }
    fn array(values: Vec<Self>) -> Self {
        Self::Array(values)
    }
    fn object(values: Vec<(String, Self)>) -> Self {
        Self::Object(values.into_iter().collect())
    }
    fn member(&self, name: &str) -> Option<&Self> {
        self.get(name)
    }
    fn text(&self, name: &str) -> Option<&str> {
        self.get(name).and_then(Self::as_str)
    }
}

fn utf16_cmp(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}
