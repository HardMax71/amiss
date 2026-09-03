use serde::{Deserialize, Serialize};

use crate::controls::{
    FactSchema, FindingKeyInputSchema, ProjectionKind, ProjectionSink, SourceConstruct, TargetKind,
    WaiverResidualDisposition,
};
use crate::digest::Digest;
use crate::model::{ArtifactId, Oid, OwnerId, RepoPathText, TreeIdentity, UtcInstant};

use super::super::{Disposition, FindingKind};
use super::{DocumentResult, ObservationComparison, RepoPath, Resolution, SourceSpan};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReferenceOccurrenceKind {
    #[serde(rename = "source-projection")]
    SourceProjection,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferenceOccurrence {
    pub kind: ReferenceOccurrenceKind,
    pub source_projection_digest: Digest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RepositoryIntentKind {
    #[serde(rename = "repository-path")]
    RepositoryPath,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryTargetIntent {
    #[serde(
        default,
        deserialize_with = "json_serde::deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub commit_oid: Option<Oid>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub fragment_digest: Option<Digest>,
    pub kind: RepositoryIntentKind,
    pub path: RepoPath,
    #[serde(deserialize_with = "Option::deserialize")]
    pub query_digest: Option<Digest>,
    pub target_kind: TargetKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum FindingKeyScope {
    Control {
        #[serde(deserialize_with = "Option::deserialize")]
        control_path: Option<RepoPath>,
        rule_id: String,
    },
    Document {
        document: RepoPath,
    },
    Observation {
        observation_id: Digest,
    },
    Reference {
        document: RepoPath,
        normalized_target_intent: RepositoryTargetIntent,
        occurrence: ReferenceOccurrence,
        source_construct: SourceConstruct,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindingKeyInput {
    pub finding_kind: FindingKind,
    pub schema: FindingKeyInputSchema,
    pub scope: FindingKeyScope,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControlStateSchema {
    #[serde(rename = "amiss/scanner-control-state")]
    Current,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ControlState {
    Absent,
    Invalid,
    OutsideCoverage,
    Present,
    Unsupported,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlStateSource {
    pub digest: Digest,
    pub multiplicity: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlStateInput {
    #[serde(deserialize_with = "Option::deserialize")]
    pub path: Option<RepoPathText>,
    pub rule_id: String,
    pub schema: ControlStateSchema,
    pub sources: Vec<ControlStateSource>,
    pub state: ControlState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClaimKind {
    #[serde(rename = "value")]
    Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClaimObserved {
    LineDiffers,
    LineOutOfRange,
    TargetAbsent,
    TargetLfsPointer,
    TargetNotABlob,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ProjectionSource {
    BlobLines {
        first_line: u64,
        last_line: u64,
        path: RepoPathText,
    },
    NamedRegion {
        end_marker: String,
        path: RepoPathText,
        start_marker: String,
    },
    RecordSet {
        set: ArtifactId,
    },
    RecordValue {
        key: String,
        set: ArtifactId,
    },
    TreePaths {
        maximum_depth: u64,
        root: RepoPathText,
        #[serde(
            default,
            deserialize_with = "json_serde::deserialize_some",
            skip_serializing_if = "Option::is_none"
        )]
        suffix: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectionObserved {
    ContentDiffers,
    SinkAbsent,
    SinkAmbiguous,
    SinkDocumentUnavailable,
    SinkNotAdjacent,
    SourceAbsent,
    SourceEndMarkerAbsent,
    SourceEndMarkerAmbiguous,
    SourceLfsPointer,
    SourceLinesOutOfRange,
    SourceNotABlob,
    SourceRecordAbsent,
    SourceRecordSetAbsent,
    SourceRecordSetIncomplete,
    SourceRecordUnproven,
    SourceRegionNotUtf8,
    SourceRegionOrderInvalid,
    SourceStartMarkerAbsent,
    SourceStartMarkerAmbiguous,
    SourceTreePathNotARow,
    SourceTreePathNotUtf8,
    SourceTreeRootAbsent,
    SourceTreeRootNotATree,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ProjectionDifference {
    Count {
        expected_count: u64,
        #[serde(deserialize_with = "Option::deserialize")]
        observed_count: Option<u64>,
    },
    Rows {
        expected_records: u64,
        extra_omitted: u64,
        extra_preview: Vec<String>,
        extra_records: u64,
        missing_omitted: u64,
        missing_preview: Vec<String>,
        missing_records: u64,
        observed_records: u64,
        ordering_only: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ExceptionDiagnostic {
    Debt {
        accepted_fact_digest: Digest,
        adoption_tree: TreeIdentity,
        created_at: UtcInstant,
        current_fact_digest: Digest,
        debt_id: ArtifactId,
        debt_snapshot_digest: Digest,
        expires_at: UtcInstant,
        owner: OwnerId,
        reason: String,
    },
    Waiver {
        authorized_fact_digest: Digest,
        candidate_tree: TreeIdentity,
        created_at: UtcInstant,
        #[serde(deserialize_with = "Option::deserialize")]
        current_fact_digest: Option<Digest>,
        expires_at: UtcInstant,
        finding_key: Digest,
        issuer: OwnerId,
        not_before: UtcInstant,
        owner: OwnerId,
        reason: String,
        residual_disposition: WaiverResidualDisposition,
        waiver_bundle_digest: Digest,
        waiver_id: ArtifactId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum FindingFactEvidence {
    Claim {
        claim_kind: ClaimKind,
        expected_digest: Digest,
        line: u64,
        name: String,
        observed: ClaimObserved,
        #[serde(deserialize_with = "Option::deserialize")]
        observed_digest: Option<Digest>,
        sources: Vec<ControlStateSource>,
        target_path: RepoPathText,
    },
    Control {
        #[serde(deserialize_with = "Option::deserialize")]
        base_control_digest: Option<Digest>,
        #[serde(deserialize_with = "Option::deserialize")]
        base_control_state: Option<ControlStateInput>,
        #[serde(deserialize_with = "Option::deserialize")]
        candidate_control_digest: Option<Digest>,
        #[serde(deserialize_with = "Option::deserialize")]
        candidate_control_state: Option<ControlStateInput>,
        #[serde(deserialize_with = "Option::deserialize")]
        control_path: Option<RepoPath>,
        #[serde(deserialize_with = "Option::deserialize")]
        exception: Option<Box<ExceptionDiagnostic>>,
        rule_id: String,
    },
    Document {
        document_result: DocumentResult,
    },
    Observation {
        comparison: Box<ObservationComparison>,
    },
    Projection {
        #[serde(
            default,
            deserialize_with = "json_serde::deserialize_some",
            skip_serializing_if = "Option::is_none"
        )]
        difference: Option<ProjectionDifference>,
        #[serde(deserialize_with = "Option::deserialize")]
        expected_bytes: Option<u64>,
        #[serde(deserialize_with = "Option::deserialize")]
        expected_digest: Option<Digest>,
        name: String,
        observed: ProjectionObserved,
        #[serde(deserialize_with = "Option::deserialize")]
        observed_bytes: Option<u64>,
        #[serde(deserialize_with = "Option::deserialize")]
        observed_digest: Option<Digest>,
        projection: ProjectionKind,
        sink: ProjectionSink,
        source: ProjectionSource,
        sources: Vec<ControlStateSource>,
    },
    Reference {
        occurrence_multiplicity: u64,
        resolution: Resolution,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindingFactInput {
    pub evidence: FindingFactEvidence,
    pub finding_kind: FindingKind,
    pub key_input: FindingKeyInput,
    pub schema: FactSchema,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CoverageRequirement {
    BuiltIn,
    ControlPlane,
    ExternallyProtected,
    None,
    RepositoryRequested,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceClass {
    AnalysisIntegrity,
    ControlPlane,
    CoverageBoundary,
    DeterministicStructural,
    ImpactObservation,
    Unsupported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InvariantClass {
    Absolute,
    Advisory,
    AnalysisIntegrity,
    Ratcheted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Attribution {
    Introduced,
    NotApplicable,
    PreExisting,
    Resolved,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AggregationStrategy {
    #[serde(rename = "one-per-finding-key")]
    OnePerFindingKey,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RepresentativeRule {
    #[serde(rename = "lowest-location-then-observation-id")]
    LowestLocationThenObservationId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindingAggregation {
    pub locations_omitted: u64,
    pub member_count: u64,
    pub representative_rule: RepresentativeRule,
    pub strategy: AggregationStrategy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LocationSide {
    Base,
    Candidate,
    Control,
    Global,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindingLocation {
    #[serde(deserialize_with = "Option::deserialize")]
    pub path: Option<RepoPath>,
    pub side: LocationSide,
    #[serde(deserialize_with = "Option::deserialize")]
    pub span: Option<SourceSpan>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ByteSpan {
    pub end_byte: u64,
    pub start_byte: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindingFix {
    pub description: String,
    pub path: RepoPathText,
    pub replacement: String,
    pub span: ByteSpan,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PolicySource {
    BuiltIn,
    DebtSnapshot,
    OrganizationFloor,
    RepositoryPolicy,
    ResolvedProjection,
    UnsuppressibleClamp,
    WaiverBundle,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyStep {
    pub after: Disposition,
    pub before: Disposition,
    pub rule_id: String,
    pub source: PolicySource,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DebtApplication {
    pub accepted_fact_digest: Digest,
    pub adoption_tree: TreeIdentity,
    pub created_at: UtcInstant,
    pub debt_id: ArtifactId,
    pub debt_snapshot_digest: Digest,
    pub expires_at: UtcInstant,
    pub owner: OwnerId,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaiverApplication {
    pub authorized_fact_digest: Digest,
    pub candidate_tree: TreeIdentity,
    pub created_at: UtcInstant,
    pub expires_at: UtcInstant,
    pub issuer: OwnerId,
    pub not_before: UtcInstant,
    pub owner: OwnerId,
    pub reason: String,
    pub residual_disposition: WaiverResidualDisposition,
    pub waiver_bundle_digest: Digest,
    pub waiver_id: ArtifactId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub aggregation: FindingAggregation,
    pub attribution: Attribution,
    #[serde(deserialize_with = "Option::deserialize")]
    pub base_fact: Option<FindingFactInput>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub base_fact_digest: Option<Digest>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub candidate_fact: Option<FindingFactInput>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub candidate_fact_digest: Option<Digest>,
    pub configured_disposition: Disposition,
    pub coverage_requirement: CoverageRequirement,
    #[serde(deserialize_with = "Option::deserialize")]
    pub debt: Option<DebtApplication>,
    pub description: String,
    pub effective_disposition: Disposition,
    pub evidence_class: EvidenceClass,
    pub finding_key: Digest,
    #[serde(deserialize_with = "Option::deserialize")]
    pub fix: Option<FindingFix>,
    pub invariant_class: InvariantClass,
    pub key_input: FindingKeyInput,
    pub kind: FindingKind,
    pub location: FindingLocation,
    pub observation_ids: Vec<Digest>,
    pub policy_trace: Vec<PolicyStep>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub waiver: Option<WaiverApplication>,
}
