use core::{fmt, str::FromStr};
use std::sync::Arc;

use hex_fmt::HexFmt;
use serde::Serialize;
use serde_with::{DeserializeFromStr, DisplayFromStr, SerializeDisplay, serde_as};

use crate::json::Value;

/// A repository path whose bytes are valid UTF-8, mirroring the schema's
/// `RepoPathText`: the form every configuration surface is confined to.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, SerializeDisplay, DeserializeFromStr)]
pub struct RepoPathText(String);

impl RepoPathText {
    #[must_use]
    pub fn new(raw: String) -> Option<Self> {
        path_bytes_valid(raw.as_bytes()).then_some(Self(raw))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RepoPathText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for RepoPathText {
    type Err = &'static str;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        Self::new(raw.to_owned()).ok_or("invalid repository path")
    }
}

/// A repository path as the snapshot names it, mirroring the schema's
/// `RepoPath` union: text when the raw bytes are valid UTF-8, and the bytes
/// themselves otherwise. Construction classifies, so one logical path has
/// exactly one representation and a digest can never split across forms.
#[derive(Clone, Debug, Serialize)]
#[serde(transparent)]
pub struct RepoPath(Repr);

#[serde_as]
#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
enum Repr {
    Text(String),
    Bytes {
        #[serde_as(as = "DisplayFromStr")]
        bytes_hex: Arc<HexFmt<Vec<u8>>>,
    },
}

impl RepoPath {
    /// The primary constructor: validates the byte grammar, then holds the
    /// path as text exactly when the bytes decode as UTF-8.
    #[must_use]
    pub fn from_bytes(raw: Vec<u8>) -> Option<Self> {
        if !path_bytes_valid(&raw) {
            return None;
        }
        match String::from_utf8(raw) {
            Ok(text) => Some(Self(Repr::Text(text))),
            Err(invalid) => Some(Self(Repr::Bytes {
                bytes_hex: Arc::new(HexFmt(invalid.into_bytes())),
            })),
        }
    }

    #[must_use]
    pub fn new(raw: String) -> Option<Self> {
        Self::from_bytes(raw.into_bytes())
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        match &self.0 {
            Repr::Text(text) => text.as_bytes(),
            Repr::Bytes { bytes_hex } => &bytes_hex.0,
        }
    }

    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match &self.0 {
            Repr::Text(text) => Some(text),
            Repr::Bytes { .. } => None,
        }
    }

    /// The wire form: a plain string for text, byte-identical to the first
    /// contract, and the `bytes_hex` object for a path text cannot hold.
    #[must_use]
    pub fn to_value(&self) -> Value {
        match &self.0 {
            Repr::Text(text) => Value::String(text.clone().into()),
            Repr::Bytes { bytes_hex } => Value::Object(Box::new([(
                "bytes_hex".into(),
                Value::String(bytes_hex.to_string().into()),
            )])),
        }
    }
}

/// Text-form paths embed without revalidation: both types enforce the one
/// byte grammar, and a `String` is UTF-8 by construction.
impl From<&RepoPathText> for RepoPath {
    fn from(text: &RepoPathText) -> Self {
        Self(Repr::Text(text.as_str().to_owned()))
    }
}

/// Map queries run on raw bytes, because a range boundary such as `path/`
/// is not itself a valid path. Sound: ordering and equality are the byte
/// forms already.
impl std::borrow::Borrow<[u8]> for RepoPath {
    fn borrow(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl PartialEq for RepoPath {
    fn eq(&self, other: &Self) -> bool {
        self.as_bytes() == other.as_bytes()
    }
}

impl Eq for RepoPath {}

// derived ordering would sort by variant before content
impl Ord for RepoPath {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_bytes().cmp(other.as_bytes())
    }
}

impl PartialOrd for RepoPath {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

fn path_bytes_valid(raw: &[u8]) -> bool {
    if raw.is_empty() || raw.len() > 4096 || raw.contains(&0) || raw.contains(&b'\\') {
        return false;
    }
    !raw.split(|byte| *byte == b'/')
        .any(|segment| segment.is_empty() || segment == b"." || segment == b"..")
}
