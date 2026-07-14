#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RepoPath(String);

impl RepoPath {
    #[must_use]
    pub fn new(raw: String) -> Option<Self> {
        if raw.is_empty() || raw.len() > 4096 || raw.contains(['\\', '\u{0}']) {
            return None;
        }
        if raw
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
        {
            return None;
        }
        Some(Self(raw))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ArtifactId(String);

impl ArtifactId {
    #[must_use]
    pub fn new(raw: String) -> Option<Self> {
        let mut bytes = raw.bytes();
        let first = bytes.next()?;
        if raw.len() > 128 || !first.is_ascii_lowercase() && !first.is_ascii_digit() {
            return None;
        }
        if bytes.all(id_tail_byte) {
            Some(Self(raw))
        } else {
            None
        }
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
        let mut bytes = suffix.bytes();
        let first = bytes.next()?;
        if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
            return None;
        }
        if bytes.all(id_tail_byte) {
            Some(Self(raw))
        } else {
            None
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn id_tail_byte(byte: u8) -> bool {
    byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'/' | b'-')
}

/// Whole-second UTC instant; the fixed-width form makes lexicographic order
/// chronological, so ordering derives from the raw string.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct UtcInstant(String);

impl UtcInstant {
    #[must_use]
    pub fn new(raw: String) -> Option<Self> {
        let bytes = raw.as_bytes();
        if bytes.len() != 20 {
            return None;
        }
        for (index, byte) in bytes.iter().enumerate() {
            let expected_digit = !matches!(index, 4 | 7 | 10 | 13 | 16 | 19);
            if expected_digit != byte.is_ascii_digit() {
                return None;
            }
        }
        if bytes.get(4) != Some(&b'-')
            || bytes.get(7) != Some(&b'-')
            || bytes.get(10) != Some(&b'T')
            || bytes.get(13) != Some(&b':')
            || bytes.get(16) != Some(&b':')
            || bytes.get(19) != Some(&b'Z')
        {
            return None;
        }
        let year = field(bytes, 0, 4)?;
        let month = field(bytes, 5, 2)?;
        let day = field(bytes, 8, 2)?;
        let hour = field(bytes, 11, 2)?;
        let minute = field(bytes, 14, 2)?;
        let second = field(bytes, 17, 2)?;
        if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
            return None;
        }
        if hour > 23 || minute > 59 || second > 59 {
            return None;
        }
        Some(Self(raw))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whole seconds since 1970-01-01T00:00:00Z, computed from the already
    /// validated calendar fields with the days-from-civil identity.
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "every field is bounded by the validated 20-byte format, so all terms stay far inside i64"
    )]
    #[must_use]
    pub fn epoch_seconds(&self) -> i64 {
        let bytes = self.0.as_bytes();
        let part = |start: usize, len: usize| i64::from(field(bytes, start, len).unwrap_or(0));
        let year = part(0, 4);
        let month = part(5, 2);
        let day = part(8, 2);
        let shifted_year = if month <= 2 { year - 1 } else { year };
        let era = shifted_year.div_euclid(400);
        let year_of_era = shifted_year - era * 400;
        let month_index = if month > 2 { month - 3 } else { month + 9 };
        let day_of_year = (153 * month_index + 2) / 5 + day - 1;
        let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
        let days = era * 146_097 + day_of_era - 719_468;
        days * 86_400 + part(11, 2) * 3_600 + part(14, 2) * 60 + part(17, 2)
    }
}

fn field(bytes: &[u8], start: usize, len: usize) -> Option<u32> {
    let end = start.checked_add(len)?;
    bytes.get(start..end)?.iter().try_fold(0_u32, |acc, byte| {
        let digit = u32::from(byte.wrapping_sub(b'0'));
        acc.checked_mul(10)?.checked_add(digit)
    })
}

fn days_in_month(year: u32, month: u32) -> u32 {
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 0,
    }
}

/// Full branch ref under the frozen `ref-format-v1` contract.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct BranchRef(String);

impl BranchRef {
    #[must_use]
    #[expect(
        clippy::case_sensitive_file_extension_comparisons,
        reason = "ref-format-v1 component rules are byte-exact"
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
    pub owner: String,
    pub name: String,
}

impl RepositoryIdentity {
    #[must_use]
    pub fn new(owner: String, name: String) -> Option<Self> {
        let owner_bytes = owner.as_bytes();
        let owner_ok = (1..=100).contains(&owner_bytes.len())
            && owner_bytes.iter().copied().all(identity_byte)
            && owner_bytes.first().is_some_and(u8::is_ascii_alphanumeric)
            && owner_bytes.last().is_some_and(u8::is_ascii_alphanumeric);
        let name_ok = (1..=100).contains(&name.len())
            && name.bytes().all(identity_byte)
            && name != "."
            && name != "..";
        if owner_ok && name_ok {
            Some(Self { owner, name })
        } else {
            None
        }
    }
}

fn identity_byte(byte: u8) -> bool {
    byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ObjectFormat {
    Sha1,
    Sha256,
}

impl ObjectFormat {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sha1 => "sha1",
            Self::Sha256 => "sha256",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TreeIdentity {
    pub object_format: ObjectFormat,
    pub tree_oid: String,
}

impl TreeIdentity {
    #[must_use]
    pub fn new(object_format: ObjectFormat, tree_oid: String) -> Option<Self> {
        if oid_hex(object_format, &tree_oid) {
            Some(Self {
                object_format,
                tree_oid,
            })
        } else {
            None
        }
    }
}

/// Full lowercase object ID for one declared object format.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Oid(String);

impl Oid {
    #[must_use]
    pub fn new(object_format: ObjectFormat, raw: String) -> Option<Self> {
        if oid_hex(object_format, &raw) {
            Some(Self(raw))
        } else {
            None
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn oid_hex(object_format: ObjectFormat, raw: &str) -> bool {
    let expected = match object_format {
        ObjectFormat::Sha1 => 40,
        ObjectFormat::Sha256 => 64,
    };
    raw.len() == expected
        && raw
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The three closed source adapters. Every wire string an adapter contributes
/// (identity, grammar profile, frontmatter contract, projection, address
/// scheme) is frozen here so no call site can spell one by hand.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Adapter {
    Markdown,
    Mdx,
    PlainAdvisory,
}

impl Adapter {
    pub const ALL: [Self; 3] = [Self::Markdown, Self::Mdx, Self::PlainAdvisory];

    #[must_use]
    pub const fn adapter_id(self) -> &'static str {
        match self {
            Self::Markdown => "markdown-v1",
            Self::Mdx => "mdx-v1",
            Self::PlainAdvisory => "plain-advisory-v1",
        }
    }

    #[must_use]
    pub const fn parser_name(self) -> &'static str {
        match self {
            Self::Markdown => "amiss-markdown-adapter",
            Self::Mdx => "amiss-mdx-adapter",
            Self::PlainAdvisory => "amiss-plain-advisory",
        }
    }

    #[must_use]
    pub const fn grammar_profile(self) -> &'static str {
        match self {
            Self::Markdown => "commonmark-gfm-v1",
            Self::Mdx => "mdx-source-v1",
            Self::PlainAdvisory => "plain-zero-lexer-v1",
        }
    }

    #[must_use]
    pub const fn frontmatter_contract(self) -> &'static str {
        match self {
            Self::Markdown | Self::Mdx => "frontmatter-v1",
            Self::PlainAdvisory => "none",
        }
    }

    #[must_use]
    pub const fn source_projection(self) -> &'static str {
        match self {
            Self::Markdown | Self::Mdx => "source-projection-v1",
            Self::PlainAdvisory => "none",
        }
    }

    #[must_use]
    pub const fn structural_address(self) -> &'static str {
        match self {
            Self::Markdown => "markdown-ast-node-path",
            Self::Mdx => "mdx-ast-node-path",
            Self::PlainAdvisory => "none",
        }
    }
}
