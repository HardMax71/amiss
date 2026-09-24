use std::borrow::Cow;

use serde::{Deserialize, Serialize};

/// A repository path whose bytes are valid UTF-8, mirroring the schema's
/// `RepoPathText`: the form every configuration surface is confined to.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct RepoPathText(Cow<'static, str>);

impl RepoPathText {
    const INVALID: &'static str = "invalid repository path";

    /// # Panics
    /// When the literal is not a repository path; `repo_path_text!` makes that a build error.
    #[must_use]
    pub const fn from_static(raw: &'static str) -> Self {
        assert!(path_bytes_valid(raw.as_bytes()), "{}", Self::INVALID);
        Self(Cow::Borrowed(raw))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for RepoPathText {
    type Error = &'static str;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        if path_bytes_valid(raw.as_bytes()) {
            Ok(Self(Cow::Owned(raw)))
        } else {
            Err(Self::INVALID)
        }
    }
}

#[macro_export]
macro_rules! repo_path_text {
    ($raw:literal) => {
        const { $crate::model::RepoPathText::from_static($raw) }
    };
}

/// A repository path as the snapshot names it, mirroring the schema's
/// `RepoPath` union: text when the raw bytes are valid UTF-8, and the bytes
/// themselves otherwise. Construction classifies, so one logical path has
/// exactly one representation and a digest can never split across forms.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RepoPath(Repr);

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum Repr {
    Text(RepoPathText),
    Bytes(PathBytes),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PathBytes {
    bytes: Vec<u8>,
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
            Ok(text) => Some(Self(Repr::Text(RepoPathText(Cow::Owned(text))))),
            Err(invalid) => Some(Self(Repr::Bytes(PathBytes {
                bytes: invalid.into_bytes(),
            }))),
        }
    }

    #[must_use]
    pub fn new(raw: String) -> Option<Self> {
        Self::from_bytes(raw.into_bytes())
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        match &self.0 {
            Repr::Text(text) => text.as_str().as_bytes(),
            Repr::Bytes(path) => &path.bytes,
        }
    }

    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match &self.0 {
            Repr::Text(text) => Some(text.as_str()),
            Repr::Bytes(_) => None,
        }
    }
}

/// Text-form paths embed without revalidation: both types enforce the one
/// byte grammar, and a `String` is UTF-8 by construction.
impl From<&RepoPathText> for RepoPath {
    fn from(text: &RepoPathText) -> Self {
        Self(Repr::Text(text.clone()))
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

const fn path_bytes_valid(raw: &[u8]) -> bool {
    if raw.is_empty() || raw.len() > 4096 {
        return false;
    }
    let mut rest = raw;
    let mut segment = Segment::Empty;
    while let Some((&byte, tail)) = rest.split_first() {
        rest = tail;
        segment = match byte {
            b'/' if matches!(segment, Segment::Named) => Segment::Empty,
            0 | b'\\' | b'/' => return false,
            b'.' => match segment {
                Segment::Empty => Segment::Dot,
                Segment::Dot => Segment::DotDot,
                Segment::DotDot | Segment::Named => Segment::Named,
            },
            _ => Segment::Named,
        };
    }
    matches!(segment, Segment::Named)
}

// a segment is a name once it holds anything other than one or two dots
enum Segment {
    Empty,
    Dot,
    DotDot,
    Named,
}
