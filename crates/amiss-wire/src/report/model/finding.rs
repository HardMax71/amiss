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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EmptyRepositoryPath {
    #[serde(rename = "")]
    Empty,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RepositoryIntentPath<P = RepoPath> {
    Empty(EmptyRepositoryPath),
    Path(P),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryTargetIntent<P = RepoPath> {
    #[serde(
        default,
        deserialize_with = "json_serde::deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub commit_oid: Option<Oid>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub fragment_digest: Option<Digest>,
    pub kind: RepositoryIntentKind,
    pub path: RepositoryIntentPath<P>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub query_digest: Option<Digest>,
    pub target_kind: TargetKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControlFindingKeyScopeKind {
    #[serde(rename = "control")]
    Control,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DocumentFindingKeyScopeKind {
    #[serde(rename = "document")]
    Document,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObservationFindingKeyScopeKind {
    #[serde(rename = "observation")]
    Observation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReferenceFindingKeyScopeKind {
    #[serde(rename = "reference")]
    Reference,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FindingKeyScope<P = RepoPath> {
    Control {
        #[serde(deserialize_with = "Option::deserialize")]
        control_path: Option<P>,
        kind: ControlFindingKeyScopeKind,
        rule_id: String,
    },
    Document {
        document: P,
        kind: DocumentFindingKeyScopeKind,
    },
    Observation {
        kind: ObservationFindingKeyScopeKind,
        observation_id: Digest,
    },
    Reference {
        document: P,
        kind: ReferenceFindingKeyScopeKind,
        normalized_target_intent: RepositoryTargetIntent<P>,
        occurrence: ReferenceOccurrence,
        source_construct: SourceConstruct,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindingKeyInput<P = RepoPath> {
    pub finding_kind: FindingKind,
    pub schema: FindingKeyInputSchema,
    pub scope: FindingKeyScope<P>,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BrokenRedirectReason {
    AmbiguousRoute,
    MissingAnchor,
    MissingRoute,
    NonterminalRedirect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlobLinesProjectionSourceKind {
    #[serde(rename = "blob-lines")]
    BlobLines,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NamedRegionProjectionSourceKind {
    #[serde(rename = "named-region")]
    NamedRegion,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecordSetProjectionSourceKind {
    #[serde(rename = "record-set")]
    RecordSet,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecordValueProjectionSourceKind {
    #[serde(rename = "record-value")]
    RecordValue,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TreePathsProjectionSourceKind {
    #[serde(rename = "tree-paths")]
    TreePaths,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ProjectionSource {
    BlobLines {
        first_line: u64,
        kind: BlobLinesProjectionSourceKind,
        last_line: u64,
        path: RepoPathText,
    },
    NamedRegion {
        end_marker: String,
        kind: NamedRegionProjectionSourceKind,
        path: RepoPathText,
        start_marker: String,
    },
    RecordSet {
        kind: RecordSetProjectionSourceKind,
        set: ArtifactId,
    },
    RecordValue {
        key: String,
        kind: RecordValueProjectionSourceKind,
        set: ArtifactId,
    },
    TreePaths {
        kind: TreePathsProjectionSourceKind,
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

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, strum::AsRefStr, strum::EnumIter,
)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CountProjectionDifferenceKind {
    #[serde(rename = "count")]
    Count,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RowsProjectionDifferenceKind {
    #[serde(rename = "rows")]
    Rows,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RowsProjectionDifference {
    pub expected_records: u64,
    pub extra_omitted: u64,
    pub extra_preview: Vec<String>,
    pub extra_records: u64,
    pub kind: RowsProjectionDifferenceKind,
    pub missing_omitted: u64,
    pub missing_preview: Vec<String>,
    pub missing_records: u64,
    pub observed_records: u64,
    pub ordering_only: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ProjectionDifference<R = RowsProjectionDifference> {
    Count {
        expected_count: u64,
        kind: CountProjectionDifferenceKind,
        #[serde(deserialize_with = "Option::deserialize")]
        observed_count: Option<u64>,
    },
    Rows(R),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DebtExceptionDiagnosticKind {
    #[serde(rename = "debt")]
    Debt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WaiverExceptionDiagnosticKind {
    #[serde(rename = "waiver")]
    Waiver,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ExceptionDiagnostic {
    Debt {
        accepted_fact_digest: Digest,
        adoption_tree: TreeIdentity,
        created_at: UtcInstant,
        current_fact_digest: Digest,
        debt_id: ArtifactId,
        debt_snapshot_digest: Digest,
        expires_at: UtcInstant,
        kind: DebtExceptionDiagnosticKind,
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
        kind: WaiverExceptionDiagnosticKind,
        not_before: UtcInstant,
        owner: OwnerId,
        reason: String,
        residual_disposition: WaiverResidualDisposition,
        waiver_bundle_digest: Digest,
        waiver_id: ArtifactId,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BrokenRedirectFactEvidenceKind {
    #[serde(rename = "broken-redirect")]
    BrokenRedirect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClaimFactEvidenceKind {
    #[serde(rename = "claim")]
    Claim,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControlFactEvidenceKind {
    #[serde(rename = "control")]
    Control,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DocumentFactEvidenceKind {
    #[serde(rename = "document")]
    Document,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DuplicateRouteFactEvidenceKind {
    #[serde(rename = "duplicate-route")]
    DuplicateRoute,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObservationFactEvidenceKind {
    #[serde(rename = "observation")]
    Observation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectionFactEvidenceKind {
    #[serde(rename = "projection")]
    Projection,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReferenceFactEvidenceKind {
    #[serde(rename = "reference")]
    Reference,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferenceFactEvidence<R = Resolution> {
    pub kind: ReferenceFactEvidenceKind,
    pub occurrence_multiplicity: u64,
    pub resolution: R,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FindingFactEvidence<P = RepoPath> {
    BrokenRedirect {
        claim_digest: Digest,
        destination: String,
        kind: BrokenRedirectFactEvidenceKind,
        reason: BrokenRedirectReason,
        route: String,
        source: P,
    },
    Claim {
        claim_kind: ClaimKind,
        expected_digest: Digest,
        kind: ClaimFactEvidenceKind,
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
        control_path: Option<P>,
        #[serde(deserialize_with = "Option::deserialize")]
        exception: Option<Box<ExceptionDiagnostic>>,
        kind: ControlFactEvidenceKind,
        rule_id: String,
    },
    Document {
        document_result: DocumentResult<P>,
        kind: DocumentFactEvidenceKind,
    },
    DuplicateRoute {
        claim_digests: Vec<Digest>,
        kind: DuplicateRouteFactEvidenceKind,
        route: String,
        sources: Vec<P>,
    },
    Observation {
        comparison: Box<ObservationComparison<P>>,
        kind: ObservationFactEvidenceKind,
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
        kind: ProjectionFactEvidenceKind,
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
    Reference(ReferenceFactEvidence<Resolution<P>>),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindingFactInput<K = FindingKeyInput, E = FindingFactEvidence> {
    pub evidence: E,
    pub finding_kind: FindingKind,
    pub key_input: K,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, strum::AsRefStr)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, strum::AsRefStr)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum LocationSide {
    Base,
    Candidate,
    Control,
    Global,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound(deserialize = "P: Deserialize<'de>"))]
pub struct FindingLocation<P = RepoPath> {
    #[serde(deserialize_with = "Option::deserialize")]
    pub path: Option<P>,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, strum::AsRefStr)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum PolicySource {
    BuiltIn,
    DebtSnapshot,
    OrganizationFloor,
    RepositoryPolicy,
    ResolvedProjection,
    UnsuppressibleClamp,
    WaiverBundle,
}

/// Built-in starts at `record`; each later `before` equals the previous `after`.
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
pub struct Finding<P = RepoPath> {
    pub aggregation: FindingAggregation,
    pub attribution: Attribution,
    #[serde(deserialize_with = "Option::deserialize")]
    pub base_fact: Option<FindingFactInput<FindingKeyInput<P>, FindingFactEvidence<P>>>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub base_fact_digest: Option<Digest>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub candidate_fact: Option<FindingFactInput<FindingKeyInput<P>, FindingFactEvidence<P>>>,
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
    pub key_input: FindingKeyInput<P>,
    pub kind: FindingKind,
    pub location: FindingLocation<P>,
    pub observation_ids: Vec<Digest>,
    pub policy_trace: Vec<PolicyStep>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub waiver: Option<WaiverApplication>,
}
