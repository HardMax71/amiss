use amiss_wire::controls::GitMode;
use amiss_wire::model::Oid;
use js_int::UInt;
use serde::{Deserialize, Serialize};
use serde_with::{As, TryFromInto};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ContentResponse {
    Entry(Box<ContentRecord>),
    Directory(Vec<ContentRecord>),
}

#[serde_with::apply(u64 => #[serde(with = "As::<TryFromInto<UInt>>")])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContentRecord {
    pub name: String,
    pub path: String,
    pub sha: Oid,
    #[serde(rename = "type")]
    pub kind: ContentKind,
    pub size: u64,
    #[serde(deserialize_with = "Option::deserialize")]
    pub encoding: Option<ContentEncoding>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub content: Option<String>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub target: Option<String>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub url: Option<String>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub html_url: Option<String>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub git_url: Option<String>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub download_url: Option<String>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub submodule_git_url: Option<String>,
    #[serde(rename = "_links", deserialize_with = "Option::deserialize")]
    pub links: Option<ContentLinks>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<GitMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_commit_sha: Option<Oid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_committer_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_author_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_commit_when: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_commit_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lfs_oid: Option<String>,
    #[serde(
        default,
        deserialize_with = "json_serde::deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub lfs_size: Option<UInt>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ContentKind {
    File,
    Dir,
    Symlink,
    Submodule,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ContentEncoding {
    Base64,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContentLinks {
    #[serde(rename = "self", deserialize_with = "Option::deserialize")]
    pub self_url: Option<String>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub git: Option<String>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub html: Option<String>,
}
