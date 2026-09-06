use amiss_wire::report::model::{Occurrence, RepoPath};
use serde::Deserialize;
use serde_json::Value;

pub(crate) struct Reference {
    pub occurrence: Occurrence<RepoPath, ReferenceResolution>,
    pub original: Value,
}

#[derive(Deserialize)]
pub(super) struct ReferencePayload {
    pub observations: Vec<ReferenceComparison>,
}

#[derive(Deserialize)]
pub(super) struct ReferenceComparison {
    pub candidate: Option<Value>,
    pub alternatives: ReferenceAlternatives,
}

#[derive(Deserialize)]
pub(super) struct ReferenceAlternatives {
    pub candidate: Vec<Value>,
}

#[derive(Deserialize)]
pub(crate) struct ReferenceResolution {
    pub kind: String,
    pub path: Option<RepoPath>,
    pub target: Option<ReferenceTarget>,
    pub scope: Option<ReferenceTarget>,
}

#[derive(Deserialize)]
pub(crate) struct ReferenceTarget {
    pub path: Option<RepoPath>,
}
