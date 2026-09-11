use amiss_wire::digest::Digest;
use amiss_wire::model::Oid;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct ArtifactRunRecord {
    pub id: u64,
    pub repository_id: u64,
    pub head_repository_id: u64,
    pub head_sha: Oid,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct WorkflowArtifactRecord {
    pub id: u64,
    pub name: String,
    pub size_in_bytes: u64,
    pub expired: bool,
    pub digest: Digest,
    pub workflow_run: ArtifactRunRecord,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct WorkflowArtifactPage {
    pub total_count: u64,
    pub artifacts: Vec<WorkflowArtifactRecord>,
}
