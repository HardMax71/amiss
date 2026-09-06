use amiss_wire::controls::Profile;
use amiss_wire::digest::Digest;
use amiss_wire::model::{ArtifactId, RepoPathText};
use serde::Serialize;

use super::super::{ExternalPolicy, SemanticEvidenceExpectation};

#[derive(Clone, Copy, Serialize)]
pub(super) struct ControlIdentity {
    pub(super) digest: Digest,
    pub(super) trust_source: amiss_wire::requests::RequestTrust,
}

#[derive(Serialize)]
pub(super) struct PlanIdentity<'a> {
    pub(super) debt_snapshot: Option<ControlIdentity>,
    pub(super) execution_constraint_digest: Digest,
    pub(super) external_policy: ExternalPolicy,
    pub(super) organization_floor: Option<ControlIdentity>,
    pub(super) profile: Profile,
    pub(super) required_status_name: &'a str,
    pub(super) schema: &'static str,
    pub(super) semantic_acquisitions: &'a [SemanticEvidenceExpectation],
    pub(super) semantic_evidence: Vec<SemanticIdentity<'a>>,
    pub(super) waiver_bundle: Option<ControlIdentity>,
    pub(super) workflow_artifacts: Vec<WorkflowArtifactIdentity<'a>>,
}

#[derive(Serialize)]
pub(super) struct SemanticIdentity<'a> {
    pub(super) complete: bool,
    pub(super) context_digest: Digest,
    pub(super) input_digest: Digest,
    pub(super) producer_identity: &'a ArtifactId,
    pub(super) producer_kind: amiss_wire::semantic::SemanticProducerKind,
    pub(super) producer_version: &'a str,
}

#[derive(Serialize)]
pub(super) struct WorkflowArtifactIdentity<'a> {
    pub(super) archive_byte_limit: u64,
    pub(super) artifact_name: &'a str,
    pub(super) candidate_binding: &'static str,
    pub(super) event: &'a str,
    pub(super) file_byte_limit: u64,
    pub(super) payload_file: &'a RepoPathText,
    pub(super) provider_instance: &'a str,
    pub(super) provider_namespace: &'a str,
    pub(super) repository_host: &'a str,
    pub(super) repository_name: &'a str,
    pub(super) repository_owner: &'a str,
    pub(super) semantic: &'a SemanticEvidenceExpectation,
    pub(super) workflow_identity: &'a str,
}
