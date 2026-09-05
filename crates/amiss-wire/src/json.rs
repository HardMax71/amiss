use std::cmp::Ordering;

mod read;
mod write;

pub use read::{Error, ErrorKind, parse};
pub(crate) use write::Callback;
pub use write::{Sink, canonical, canonical_length, stream, write_string};

pub const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

/// An owned JSON string with no spare mutable capacity.
pub type Text = Box<str>;

/// An owned strict-JSON tree with fixed-size strings, arrays, and objects.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Null,
    Bool(bool),
    Integer(i64),
    String(Text),
    Array(Box<[Value]>),
    /// Keys sorted by UTF-16 code units and unique; `parse` enforces both.
    Object(Box<[(String, Value)]>),
}

impl Value {
    /// Freezes one owned string into a JSON string value.
    #[must_use]
    pub fn string(value: impl Into<Text>) -> Self {
        Self::String(value.into())
    }

    /// Freezes a completed sequence into a JSON array value.
    #[must_use]
    pub fn array(values: Vec<Self>) -> Self {
        Self::Array(values.into_boxed_slice())
    }

    /// Freezes a completed member sequence into a JSON object value.
    #[must_use]
    pub fn object(values: Vec<(String, Self)>) -> Self {
        Self::Object(values.into_boxed_slice())
    }

    /// The named member of an object value, or `None` off objects.
    #[must_use]
    pub fn member(&self, name: &str) -> Option<&Self> {
        if let Self::Object(members) = self {
            members
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value)
        } else {
            None
        }
    }

    /// The named member as a string slice, or `None` when absent or not one.
    #[must_use]
    pub fn text(&self, name: &str) -> Option<&str> {
        if let Some(Self::String(text)) = self.member(name) {
            Some(text)
        } else {
            None
        }
    }
}

fn utf16_cmp(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}
