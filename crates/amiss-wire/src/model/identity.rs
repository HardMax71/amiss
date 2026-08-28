#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ArtifactId(String);

impl ArtifactId {
    #[must_use]
    pub fn new(raw: String) -> Option<Self> {
        (raw.len() <= 128 && id_body_valid(raw.as_bytes())).then_some(Self(raw))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
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
        id_body_valid(suffix.as_bytes()).then_some(Self(raw))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn id_body_valid(raw: &[u8]) -> bool {
    let Some((&first, tail)) = raw.split_first() else {
        return false;
    };
    (first.is_ascii_lowercase() || first.is_ascii_digit())
        && tail.iter().copied().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'.' | b'_' | b'/' | b'-')
        })
}

/// Full branch ref under the rolling `ref-format` contract.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct BranchRef(String);

impl BranchRef {
    #[must_use]
    #[expect(
        clippy::case_sensitive_file_extension_comparisons,
        reason = "ref-format component rules are byte-exact"
    )]
    pub fn new(raw: String) -> Option<Self> {
        if raw.len() > 266 {
            return None;
        }
        let suffix = raw.strip_prefix("refs/heads/")?;
        if suffix.is_empty() || suffix.contains("..") || suffix.contains("@{") {
            return None;
        }
        if suffix.bytes().any(|b| {
            b < 0x20
                || b == 0x7f
                || matches!(b, b' ' | b'~' | b'^' | b':' | b'?' | b'*' | b'[' | b'\\')
        }) {
            return None;
        }
        if suffix.ends_with('.') {
            return None;
        }
        let components_ok = suffix
            .split('/')
            .all(|c| !c.is_empty() && !c.starts_with('.') && !c.ends_with(".lock"));
        if components_ok { Some(Self(raw)) } else { None }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RepositoryIdentity {
    host: String,
    owner: String,
    name: String,
}

impl RepositoryIdentity {
    /// The open identity: any canonical forge host, an owner of one or more
    /// slash-joined segments (nested segments spell a GitLab group path),
    /// and a name.
    #[must_use]
    pub fn new(host: String, owner: String, name: String) -> Option<Self> {
        let owner_ok = (1..=255).contains(&owner.len())
            && owner
                .as_bytes()
                .split(|&byte| byte == b'/')
                .all(identity_segment);
        (host_valid(&host) && owner_ok && name_valid(&name)).then_some(Self { host, owner, name })
    }

    /// Convenience constructor for GitHub's fixed host and single-segment
    /// owner form.
    #[must_use]
    pub fn github(owner: String, name: String) -> Option<Self> {
        (identity_segment(owner.as_bytes()) && name_valid(&name)).then_some(Self {
            host: "github.com".to_owned(),
            owner,
            name,
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
