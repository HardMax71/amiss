use amiss_wire::assessment::Nullable;
use amiss_wire::digest::Digest;
use amiss_wire::model::Oid;
use json_serde::deserialize_some;
use serde::{Deserialize, Serialize};

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactRunRecord {
    pub id: Option<u64>,
    pub repository_id: Option<u64>,
    pub head_repository_id: Option<u64>,
    pub head_branch: Option<String>,
    pub head_sha: Option<Oid>,
}

#[serde_with::apply(Option<Nullable<_>> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowArtifactRecord {
    pub id: u64,
    pub node_id: String,
    pub name: String,
    pub size_in_bytes: u64,
    pub url: String,
    pub archive_download_url: String,
    pub expired: bool,
    #[serde(deserialize_with = "Option::deserialize")]
    pub created_at: Option<String>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub expires_at: Option<String>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub updated_at: Option<String>,
    pub digest: Option<Nullable<Digest>>,
    pub workflow_run: Option<Nullable<ArtifactRunRecord>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowArtifactPage {
    pub total_count: u64,
    pub artifacts: Vec<WorkflowArtifactRecord>,
}
