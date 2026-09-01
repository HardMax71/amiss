use core::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{AsRefStr, EnumIter, EnumString, IntoStaticStr};

/// The same-repository URL dialect a run applies: named in the report's
/// evaluation and selecting the recognition grammar in the resolver.
#[derive(Clone, Copy, Debug, PartialEq, Eq, AsRefStr, EnumIter, EnumString, IntoStaticStr)]
#[strum(serialize_all = "lowercase")]
pub enum ForgeDialect {
    Github,
    Gitlab,
    Gitea,
    #[strum(serialize = "bitbucket-cloud")]
    BitbucketCloud,
    #[strum(serialize = "bitbucket-data-center")]
    BitbucketDataCenter,
}

impl ForgeDialect {
    /// The known-host default table; an explicit flag always wins over it.
    #[must_use]
    pub fn default_for_host(host: &str) -> Option<Self> {
        match host {
            "github.com" => Some(Self::Github),
            "gitlab.com" => Some(Self::Gitlab),
            "codeberg.org" => Some(Self::Gitea),
            "bitbucket.org" => Some(Self::BitbucketCloud),
            _ => None,
        }
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    AsRefStr,
    EnumString,
    IntoStaticStr,
    Serialize,
    Deserialize,
)]
#[strum(serialize_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum ObjectFormat {
    Sha1,
    Sha256,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TreeIdentity {
    oid: Oid,
}

impl TreeIdentity {
    #[must_use]
    pub fn new(object_format: ObjectFormat, tree_oid: String) -> Option<Self> {
        Oid::new(object_format, tree_oid).map(|oid| Self { oid })
    }

    #[must_use]
    pub const fn object_format(&self) -> ObjectFormat {
        self.oid.object_format()
    }

    #[must_use]
    pub fn tree_oid(&self) -> &str {
        self.oid.as_str()
    }

    #[must_use]
    pub const fn oid(&self) -> &Oid {
        &self.oid
    }
}

/// Full lowercase object ID for one declared object format.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, SerializeDisplay, DeserializeFromStr)]
pub struct Oid {
    object_format: ObjectFormat,
    raw: String,
}

impl Oid {
    #[must_use]
    pub fn new(object_format: ObjectFormat, raw: String) -> Option<Self> {
        oid_hex(object_format, &raw).then_some(Self { object_format, raw })
    }

    #[must_use]
    pub const fn object_format(&self) -> ObjectFormat {
        self.object_format
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.raw
    }
}

impl fmt::Display for Oid {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.raw)
    }
}

impl FromStr for Oid {
    type Err = &'static str;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        let object_format = match raw.len() {
            40 => ObjectFormat::Sha1,
            64 => ObjectFormat::Sha256,
            _ => return Err("invalid object ID"),
        };
        Self::new(object_format, raw.to_owned()).ok_or("invalid object ID")
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
