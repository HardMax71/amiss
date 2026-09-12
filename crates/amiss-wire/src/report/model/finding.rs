use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

use crate::controls::{
    ProjectionKind, ProjectionSink, ProjectionSource, SourceConstruct, WaiverResidualDisposition,
};
use crate::model::Digest;
use crate::model::{ArtifactId, OwnerId, RepoPathText, TreeIdentity, UtcInstant};

use super::super::{Disposition, FindingKind};
use super::{
    DocumentGitMode, DocumentResult, DocumentSide, ObservationComparison, RepoPath, Resolution,
    SourceSpan,
};

pub use crate::controls::{
    FindingOccurrence as ReferenceOccurrence, OccurrenceKind as ReferenceOccurrenceKind,
    TargetIntentKind as RepositoryIntentKind,
};

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum EmptyRepositoryPath {
    #[strum(serialize = "")]
    Empty,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RepositoryIntentPath<P = RepoPath> {
    Empty(EmptyRepositoryPath),
    Path(P),
}

pub type RepositoryTargetIntent<P = RepoPath> =
    crate::controls::TargetIntent<RepositoryIntentPath<P>>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum FindingKeyScope<P = RepoPath> {
    Control {
        control_path: Option<P>,
        rule_id: String,
    },
    Document {
        document: P,
    },
    Observation {
        observation_id: Digest,
    },
    Reference {
        document: P,
        normalized_target_intent: RepositoryTargetIntent<P>,
        occurrence: ReferenceOccurrence,
        source_construct: SourceConstruct,
    },
}

pub type FindingKeyInput<P = RepoPath> =
    crate::controls::FindingKeyInput<FindingKind, FindingKeyScope<P>>;

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum ControlStateSchema {
    #[strum(serialize = "amiss/scanner-control-state")]
    Current,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum ControlState {
    Absent,
    Invalid,
    OutsideCoverage,
    Present,
    Unsupported,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlStateSource {
    pub digest: Digest,
    pub multiplicity: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlStateInput {
    pub path: Option<RepoPathText>,
    pub rule_id: String,
    pub schema: ControlStateSchema,
    pub sources: Vec<ControlStateSource>,
    pub state: ControlState,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum ClaimKind {
    #[strum(serialize = "value")]
    Value,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum ClaimObserved {
    LineDiffers,
    LineOutOfRange,
    TargetAbsent,
    TargetLfsPointer,
    TargetNotABlob,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum BrokenRedirectReason {
    AmbiguousRoute,
    MissingAnchor,
    MissingRoute,
    NonterminalRedirect,
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
    strum::EnumIter,
)]
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RowsProjectionDifference {
    pub expected_records: u64,
    pub extra_omitted: u64,
    pub extra_preview: Vec<String>,
    pub extra_records: u64,
    pub missing_omitted: u64,
    pub missing_preview: Vec<String>,
    pub missing_records: u64,
    pub observed_records: u64,
    pub ordering_only: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ProjectionDifference<R = RowsProjectionDifference> {
    Count {
        expected_count: u64,
        observed_count: Option<u64>,
    },
    Rows(R),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Display)]
#[strum(serialize_all = "kebab-case")]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum FindingFactEvidence<
    P = RepoPath,
    R = Resolution<P>,
    S = ProjectionSource,
    D = ProjectionDifference,
    M = DocumentGitMode,
> {
    BrokenRedirect {
        claim_digest: Digest,
        destination: String,
        reason: BrokenRedirectReason,
        route: String,
        source: P,
    },
    Claim {
        claim_kind: ClaimKind,
        expected_digest: Digest,
        line: u64,
        name: String,
        observed: ClaimObserved,
        observed_digest: Option<Digest>,
        sources: Vec<ControlStateSource>,
        target_path: RepoPathText,
    },
    Control {
        base_control_digest: Option<Digest>,
        base_control_state: Option<ControlStateInput>,
        candidate_control_digest: Option<Digest>,
        candidate_control_state: Option<ControlStateInput>,
        control_path: Option<P>,
        exception: Option<Box<ExceptionDiagnostic>>,
        rule_id: String,
    },
    Document {
        document_result: DocumentResult<P, DocumentSide<M>>,
    },
    DuplicateRoute {
        claim_digests: Vec<Digest>,
        route: String,
        sources: Vec<P>,
    },
    Observation {
        comparison: Box<ObservationComparison<P, R>>,
    },
    Projection {
        #[serde(skip_serializing_if = "Option::is_none")]
        difference: Option<D>,
        expected_bytes: Option<u64>,
        expected_digest: Option<Digest>,
        name: String,
        observed: ProjectionObserved,
        observed_bytes: Option<u64>,
        observed_digest: Option<Digest>,
        projection: ProjectionKind,
        sink: ProjectionSink,
        source: S,
        sources: Vec<ControlStateSource>,
    },
    Reference {
        occurrence_multiplicity: u64,
        resolution: R,
    },
}

pub type FindingFactInput<K = FindingKeyInput, E = FindingFactEvidence> =
    crate::controls::Fact<K, E, FindingKind>;

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum CoverageRequirement {
    BuiltIn,
    ControlPlane,
    ExternallyProtected,
    None,
    RepositoryRequested,
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
)]
#[strum(serialize_all = "kebab-case")]
pub enum EvidenceClass {
    AnalysisIntegrity,
    ControlPlane,
    CoverageBoundary,
    DeterministicStructural,
    ImpactObservation,
    Unsupported,
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
)]
#[strum(serialize_all = "kebab-case")]
pub enum InvariantClass {
    Absolute,
    Advisory,
    AnalysisIntegrity,
    Ratcheted,
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
)]
#[strum(serialize_all = "kebab-case")]
pub enum Attribution {
    Introduced,
    NotApplicable,
    PreExisting,
    Resolved,
    Unknown,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum AggregationStrategy {
    #[strum(serialize = "one-per-finding-key")]
    OnePerFindingKey,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum RepresentativeRule {
    #[strum(serialize = "lowest-location-then-observation-id")]
    LowestLocationThenObservationId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FindingAggregation {
    pub locations_omitted: u64,
    pub member_count: u64,
    pub representative_rule: RepresentativeRule,
    pub strategy: AggregationStrategy,
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
)]
#[strum(serialize_all = "kebab-case")]
pub enum LocationSide {
    Base,
    Candidate,
    Control,
    Global,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FindingLocation<P = RepoPath> {
    pub path: Option<P>,
    pub side: LocationSide,
    pub span: Option<SourceSpan>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ByteSpan {
    pub end_byte: u64,
    pub start_byte: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FindingFix {
    pub description: String,
    pub path: RepoPathText,
    pub replacement: String,
    pub span: ByteSpan,
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
)]
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
#[serde(deny_unknown_fields)]
pub struct PolicyStep {
    pub after: Disposition,
    pub before: Disposition,
    pub rule_id: String,
    pub source: PolicySource,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct Finding<P = RepoPath, E = FindingFactEvidence<P>> {
    pub aggregation: FindingAggregation,
    pub attribution: Attribution,
    pub base_fact: Option<FindingFactInput<FindingKeyInput<P>, E>>,
    pub base_fact_digest: Option<Digest>,
    pub candidate_fact: Option<FindingFactInput<FindingKeyInput<P>, E>>,
    pub candidate_fact_digest: Option<Digest>,
    pub configured_disposition: Disposition,
    pub coverage_requirement: CoverageRequirement,
    pub debt: Option<DebtApplication>,
    pub description: String,
    pub effective_disposition: Disposition,
    pub evidence_class: EvidenceClass,
    pub finding_key: Digest,
    pub fix: Option<FindingFix>,
    pub invariant_class: InvariantClass,
    pub key_input: FindingKeyInput<P>,
    pub kind: FindingKind,
    pub location: FindingLocation<P>,
    pub observation_ids: Vec<Digest>,
    pub policy_trace: Vec<PolicyStep>,
    pub waiver: Option<WaiverApplication>,
}
