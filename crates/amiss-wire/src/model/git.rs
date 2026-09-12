use strum::{AsRefStr, EnumIter, EnumString, IntoStaticStr};

use super::identity::Invalid;

/// The same-repository URL dialect a run applies: named in the report's
/// evaluation and selecting the recognition grammar in the resolver.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    AsRefStr,
    EnumIter,
    EnumString,
    IntoStaticStr,
    serde::Serialize,
    serde::Deserialize,
)]
#[strum(serialize_all = "lowercase")]
#[serde(rename_all = "kebab-case")]
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
    EnumIter,
    EnumString,
    IntoStaticStr,
    serde::Serialize,
    serde::Deserialize,
)]
#[strum(serialize_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum ObjectFormat {
    Sha1,
    Sha256,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize)]
#[serde(try_from = "TreeFields")]
pub struct TreeIdentity {
    oid: Oid,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct TreeFields {
    object_format: ObjectFormat,
    tree_oid: Oid,
}

impl TryFrom<TreeFields> for TreeIdentity {
    type Error = Invalid;

    fn try_from(fields: TreeFields) -> Result<Self, Invalid> {
        if fields.tree_oid.object_format() != fields.object_format {
            return Err(Invalid("tree object format"));
        }
        Ok(Self {
            oid: fields.tree_oid,
        })
    }
}

impl serde::Serialize for TreeIdentity {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut tree = serializer.serialize_struct("TreeIdentity", 2)?;
        tree.serialize_field("object_format", &self.object_format())?;
        tree.serialize_field("tree_oid", &self.oid)?;
        tree.end()
    }
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
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize)]
#[serde(try_from = "String")]
pub struct Oid {
    object_format: ObjectFormat,
    raw: String,
}

/// On the wire the length names the format; the document's declared format
/// is checked against it afterwards.
impl TryFrom<String> for Oid {
    type Error = Invalid;

    fn try_from(raw: String) -> Result<Self, Invalid> {
        let object_format = match raw.len() {
            40 => ObjectFormat::Sha1,
            64 => ObjectFormat::Sha256,
            _ => return Err(Invalid("object id")),
        };
        Self::new(object_format, raw).ok_or(Invalid("object id"))
    }
}

impl serde::Serialize for Oid {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.raw)
    }
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
