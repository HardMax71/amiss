mod decode;
mod parse;
mod site;

use std::collections::BTreeMap;
use std::sync::Arc;

use amiss_wire::digest::Digest;
use amiss_wire::json::Value;
use amiss_wire::model::ArtifactId;
use amiss_wire::report::{AnalysisErrorCode, ErrorDetail};

pub(crate) use parse::parse;
pub(crate) use site::navigation_contains;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Inputs {
    pub(crate) candidate_bindings: Vec<Digest>,
    pub(crate) labels: Arc<BTreeMap<String, InventoryLabel>>,
    pub(crate) routes: Arc<BTreeMap<String, SiteRoute>>,
    pub(crate) site: SiteEvaluation,
    pub(crate) provenance: Vec<Provenance>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum InventoryLabel {
    Unique(String),
    Ambiguous,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SiteRoute {
    Unique(SiteClaim),
    Ambiguous {
        sources: Vec<amiss_wire::model::RepoPath>,
        claims: Vec<Digest>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SiteClaim {
    pub(crate) source: Option<amiss_wire::model::RepoPath>,
    pub(crate) digest: Digest,
    pub(crate) target: SiteTarget,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SiteTarget {
    Page {
        backing: SitePageBacking,
        anchors: Vec<String>,
    },
    Redirect {
        destination: String,
        fragment: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SitePageBacking {
    Repository,
    Generated,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SiteNavigation {
    pub(crate) root: Option<amiss_wire::model::RepoPath>,
    pub(crate) manifest: amiss_wire::model::RepoPath,
    pub(crate) entrypoints: Vec<String>,
    pub(crate) reachable: Vec<amiss_wire::model::RepoPath>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SiteEvaluation {
    pub(crate) navigation: Option<Arc<SiteNavigation>>,
    pub(crate) defects: Arc<[SiteDefect]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SiteDefect {
    pub(crate) id: Digest,
    pub(crate) evidence: Value,
    pub(crate) source: Option<amiss_wire::model::RepoPath>,
    pub(crate) member_count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Provenance {
    pub payload_digest: Digest,
    pub producer_kind: ArtifactId,
    pub producer_identity: ArtifactId,
    pub producer_version: String,
    pub input_digest: Digest,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Context {
    pub(crate) labels: Arc<BTreeMap<String, InventoryLabel>>,
    pub(crate) routes: Arc<BTreeMap<String, SiteRoute>>,
    pub(crate) site: SiteEvaluation,
    pub(crate) provenance: Vec<Provenance>,
}

#[derive(Clone, Copy)]
pub(crate) struct View<'a> {
    pub(crate) labels: &'a BTreeMap<String, InventoryLabel>,
    pub(crate) routes: Option<&'a BTreeMap<String, SiteRoute>>,
}

pub(crate) fn bind(inputs: &Inputs, candidate: Digest) -> Result<Context, ErrorDetail> {
    if inputs
        .candidate_bindings
        .iter()
        .any(|binding| *binding != candidate)
    {
        return Err(ErrorDetail {
            code: AnalysisErrorCode::ControlBindingMismatch,
            path: None,
            path_bytes: None,
            resource: None,
        });
    }
    Ok(Context {
        labels: inputs.labels.clone(),
        routes: inputs.routes.clone(),
        site: inputs.site.clone(),
        provenance: inputs.provenance.clone(),
    })
}
