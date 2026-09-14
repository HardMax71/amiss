use std::borrow::Cow;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct ArtifactId(Cow<'static, str>);

impl ArtifactId {
    const INVALID: &'static str = "invalid artifact identity";
    const MAXIMUM: usize = 128;

    /// # Panics
    /// When the literal is not an artifact identity; `artifact_id!` makes that a build error.
    #[must_use]
    pub const fn from_static(raw: &'static str) -> Self {
        assert!(
            id_body_valid(raw.as_bytes(), Self::MAXIMUM),
            "{}",
            Self::INVALID
        );
        Self(Cow::Borrowed(raw))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ArtifactId {
    type Error = &'static str;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        if id_body_valid(raw.as_bytes(), Self::MAXIMUM) {
            Ok(Self(Cow::Owned(raw)))
        } else {
            Err(Self::INVALID)
        }
    }
}

#[macro_export]
macro_rules! artifact_id {
    ($raw:literal) => {
        const { $crate::model::ArtifactId::from_static($raw) }
    };
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OwnerId(String);

impl OwnerId {
    #[must_use]
    pub fn new(raw: String) -> Option<Self> {
        if raw.len() > 160 {
            return None;
        }
        let suffix = ["team:", "service:", "user:"]
            .iter()
            .find_map(|prefix| raw.strip_prefix(prefix))?;
        id_body_valid(suffix.as_bytes(), 160).then_some(Self(raw))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

const fn id_body_valid(raw: &[u8], maximum: usize) -> bool {
    let Some((&first, mut rest)) = raw.split_first() else {
        return false;
    };
    if raw.len() > maximum || (!first.is_ascii_lowercase() && !first.is_ascii_digit()) {
        return false;
    }
    while let Some((&byte, tail)) = rest.split_first() {
        rest = tail;
        if !byte.is_ascii_lowercase()
            && !byte.is_ascii_digit()
            && !matches!(byte, b'.' | b'_' | b'/' | b'-')
        {
            return false;
        }
    }
    true
}

/// Full branch ref under the rolling `ref-format` contract.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct BranchRef(Cow<'static, str>);

impl BranchRef {
    const INVALID: &'static str = "invalid branch reference";
    const PREFIX: &'static [u8] = b"refs/heads/";

    /// # Panics
    /// When the literal is not a branch ref; `branch_ref!` makes that a build error.
    #[must_use]
    pub const fn from_static(raw: &'static str) -> Self {
        assert!(Self::valid(raw.as_bytes()), "{}", Self::INVALID);
        Self(Cow::Borrowed(raw))
    }

    const fn valid(raw: &[u8]) -> bool {
        let Some(mut rest) = strip_prefix(raw, Self::PREFIX) else {
            return false;
        };
        if raw.len() > 266 || rest.is_empty() {
            return false;
        }
        let mut component = rest;
        let mut previous = b'/';
        while let Some((&byte, tail)) = rest.split_first() {
            if byte < 0x20
                || byte == 0x7f
                || matches!(byte, b' ' | b'~' | b'^' | b':' | b'?' | b'*' | b'[' | b'\\')
                || (previous == b'.' && byte == b'.')
                || (previous == b'@' && byte == b'{')
                || (previous == b'/' && matches!(byte, b'/' | b'.'))
            {
                return false;
            }
            if byte == b'/' {
                if ends_with(component, tail, b".lock") {
                    return false;
                }
                component = tail;
            }
            previous = byte;
            rest = tail;
        }
        previous != b'/' && previous != b'.' && !ends_with(component, rest, b".lock")
    }

    #[must_use]
    pub fn name(&self) -> &str {
        self.0.strip_prefix("refs/heads/").unwrap_or(&self.0)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for BranchRef {
    type Error = &'static str;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        if Self::valid(raw.as_bytes()) {
            Ok(Self(Cow::Owned(raw)))
        } else {
            Err(Self::INVALID)
        }
    }
}

#[macro_export]
macro_rules! branch_ref {
    ($raw:literal) => {
        const { $crate::model::BranchRef::from_static($raw) }
    };
}

const fn strip_prefix<'a>(raw: &'a [u8], prefix: &[u8]) -> Option<&'a [u8]> {
    match raw.split_at_checked(prefix.len()) {
        Some((head, tail)) if bytes_equal(head, prefix) => Some(tail),
        _ => None,
    }
}

/// Whether the component that runs from `component` up to `tail` ends with `suffix`.
const fn ends_with(component: &[u8], tail: &[u8], suffix: &[u8]) -> bool {
    let Some(length) = component.len().checked_sub(tail.len()) else {
        return false;
    };
    let Some(start) = length.checked_sub(suffix.len()) else {
        return false;
    };
    match component.split_at_checked(start) {
        Some((_, rest)) => match rest.split_at_checked(suffix.len()) {
            Some((end, _)) => bytes_equal(end, suffix),
            None => false,
        },
        None => false,
    }
}

const fn bytes_equal(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let (mut left, mut right) = (left, right);
    while let (Some((&a, left_tail)), Some((&b, right_tail))) =
        (left.split_first(), right.split_first())
    {
        if a != b {
            return false;
        }
        left = left_tail;
        right = right_tail;
    }
    true
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryIdentity {
    host: String,
    name: String,
    owner: String,
}

impl RepositoryIdentity {
    /// The open identity: any canonical forge host, an owner of one or more
    /// slash-joined segments (nested segments spell a GitLab group path),
    /// and a name.
    #[must_use]
    pub fn new(host: String, owner: String, name: String) -> Option<Self> {
        Self::valid_components(&host, &owner, &name).then_some(Self { host, name, owner })
    }

    /// Checks repository components without allocating an owned identity.
    #[must_use]
    pub fn valid_components(host: &str, owner: &str, name: &str) -> bool {
        let owner_ok = (1..=255).contains(&owner.len())
            && owner
                .as_bytes()
                .split(|&byte| byte == b'/')
                .all(identity_segment);
        host_valid(host) && owner_ok && name_valid(name)
    }

    /// Convenience constructor for GitHub's fixed host and single-segment
    /// owner form.
    #[must_use]
    pub fn github(owner: String, name: String) -> Option<Self> {
        (identity_segment(owner.as_bytes()) && name_valid(&name)).then_some(Self {
            host: "github.com".to_owned(),
            name,
            owner,
        })
    }

    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    #[must_use]
    pub fn owner(&self) -> &str {
        &self.owner
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

fn identity_segment(segment: &[u8]) -> bool {
    (1..=100).contains(&segment.len())
        && segment.iter().copied().all(identity_byte)
        && segment.first().is_some_and(u8::is_ascii_alphanumeric)
        && segment.last().is_some_and(u8::is_ascii_alphanumeric)
}

fn name_valid(name: &str) -> bool {
    (1..=100).contains(&name.len())
        && name.bytes().all(identity_byte)
        && name != "."
        && name != ".."
}

/// The host is an opaque claim the engine never resolves or normalizes;
/// the caller owns its spelling. A slash would make the identity triple
/// ambiguous, and the cap bounds it like every other wire string.
fn host_valid(host: &str) -> bool {
    (1..=255).contains(&host.len()) && !host.contains('/')
}

fn identity_byte(byte: u8) -> bool {
    byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
}
