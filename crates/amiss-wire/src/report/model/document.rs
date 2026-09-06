use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

use crate::controls::ContentAvailability;
use crate::digest::Digest;
use crate::model::{Adapter, Oid, RepoPathText};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoPathBytes {
    pub bytes_hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RepoPath {
    Text(RepoPathText),
    Bytes(RepoPathBytes),
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Display,
    EnumString,
    SerializeDisplay,
    DeserializeFromStr,
    strum::AsRefStr,
    strum::IntoStaticStr,
    strum::EnumIter,
)]
#[strum(serialize_all = "kebab-case")]
pub enum DocumentClassification {
    ExtensionlessMarkdown,
    PlainAdvisory,
    PolicyIncluded,
    #[strum(serialize = "structured-asciidoc")]
    StructuredAsciiDoc,
    StructuredMarkdown,
    StructuredMdx,
    StructuredRst,
    UnparsedMarkup,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum DocumentEntryKind {
    Blob,
    Gitlink,
    Symlink,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum DocumentStatus {
    ExcludedBuiltIn,
    Scanned,
    Unsupported,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum UnsupportedReason {
    GitlinkDocument,
    LfsPointer,
    SymlinkDocument,
    UnsupportedDocumentFormat,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum DocumentGitMode {
    #[strum(serialize = "100644")]
    RegularFile,
    #[strum(serialize = "100755")]
    ExecutableFile,
    #[strum(serialize = "120000")]
    Symlink,
    #[strum(serialize = "160000")]
    Gitlink,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentSide<M = DocumentGitMode> {
    #[serde(deserialize_with = "Option::deserialize")]
    pub adapter_id: Option<Adapter>,
    pub byte_count: u64,
    pub content_availability: ContentAvailability,
    pub entry_kind: DocumentEntryKind,
    pub entry_oid: Oid,
    pub extracted_references: u64,
    pub frontmatter_bytes: u64,
    pub frontmatter_regions: u64,
    pub git_mode: M,
    pub opaque_html_bytes: u64,
    pub opaque_html_regions: u64,
    pub opaque_mdx_bytes: u64,
    pub opaque_mdx_regions: u64,
    #[serde(deserialize_with = "Option::deserialize")]
    pub raw_digest: Option<Digest>,
    pub status: DocumentStatus,
    #[serde(deserialize_with = "Option::deserialize")]
    pub unsupported_reason: Option<UnsupportedReason>,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum DocumentChange {
    Added,
    Changed,
    Removed,
    Unchanged,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound(deserialize = "P: Deserialize<'de>, S: Deserialize<'de>"))]
pub struct DocumentResult<P = RepoPath, S = DocumentSide> {
    #[serde(deserialize_with = "Option::deserialize")]
    pub base: Option<S>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub candidate: Option<S>,
    pub change: DocumentChange,
    pub classification: DocumentClassification,
    pub path: P,
}
