use serde::{Deserialize, Serialize};

use crate::assessment::Nullable;
use crate::controls::{SourceConstruct, TargetKind};
use crate::digest::Digest;
use crate::extraction::BlockKind;
use crate::model::{Adapter, Oid};
use crate::resolution::BlobMode;
pub use crate::resolution::{
    ExternalReference as ExternalResolutionReason, InvalidReference as InvalidResolutionReason,
    UnsupportedTargetTag as UnsupportedTargetReason,
};

use super::RepoPath;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObservationIdInputSchema {
    #[serde(rename = "amiss/scanner-observation-id-input")]
    Current,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StructuralAddressSchema {
    #[serde(rename = "amiss/scanner-structural-address")]
    Current,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, strum::IntoStaticStr)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum AddressKind {
    AsciidocBlockPath,
    MarkdownAstNodePath,
    MdxAstNodePath,
    RstBlockPath,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructuralAddress {
    pub address_kind: AddressKind,
    pub construct_index: u64,
    pub duplicate_index: u64,
    pub node_path: Vec<u64>,
    pub schema: StructuralAddressSchema,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceSpan {
    pub end_byte: u64,
    pub end_column: u64,
    pub end_line: u64,
    pub start_byte: u64,
    pub start_column: u64,
    pub start_line: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound(deserialize = "P: Deserialize<'de>"))]
pub struct TargetIntent<P = RepoPath> {
    #[serde(
        default,
        deserialize_with = "json_serde::deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub commit_oid: Option<Oid>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub external_scheme: Option<String>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub fragment_digest: Option<Digest>,
    pub kind: super::super::IntentKind,
    #[serde(deserialize_with = "Option::deserialize")]
    pub query_digest: Option<Digest>,
    pub raw_destination_digest: Digest,
    #[serde(deserialize_with = "Option::deserialize")]
    pub repository_path: Option<P>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub target_kind: Option<TargetKind>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservationIdInput<P = RepoPath> {
    pub adapter_contract_digest: Digest,
    pub adapter_id: Adapter,
    pub document: P,
    pub extracted_intent: TargetIntent<P>,
    pub schema: ObservationIdInputSchema,
    pub source_construct: SourceConstruct,
    pub source_projection_digest: Digest,
    pub structural_address: StructuralAddress,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ResolutionContent {
    Available {
        projection_digest: Digest,
        raw_digest: Digest,
    },
    LfsPointer {
        raw_digest: Digest,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlobResolutionTargetKind {
    #[serde(rename = "blob")]
    Blob,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TreeResolutionTargetKind {
    #[serde(rename = "tree")]
    Tree,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResolutionTarget<P = RepoPath> {
    Blob {
        content: ResolutionContent,
        kind: BlobResolutionTargetKind,
        mode: BlobMode,
        path: P,
    },
    Tree {
        kind: TreeResolutionTargetKind,
        path: P,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HeadingAnchorNotFoundResolutionReason {
    #[serde(rename = "heading-anchor-not-found")]
    HeadingAnchorNotFound,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LabelNotDeclaredResolutionReason {
    #[serde(rename = "label-not-declared")]
    LabelNotDeclared,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineFragmentOutOfRangeResolutionReason {
    #[serde(rename = "line-fragment-out-of-range")]
    LineFragmentOutOfRange,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PathNotFoundResolutionReason {
    #[serde(rename = "path-not-found")]
    PathNotFound,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged, bound(deserialize = "P: Deserialize<'de>"))]
pub enum MissingResolution<P = RepoPath> {
    HeadingAnchorNotFound {
        #[serde(deserialize_with = "Option::deserialize")]
        near: Option<String>,
        path: P,
        reason: HeadingAnchorNotFoundResolutionReason,
    },
    LabelNotDeclared {
        reason: LabelNotDeclaredResolutionReason,
    },
    LineFragmentOutOfRange {
        path: P,
        reason: LineFragmentOutOfRangeResolutionReason,
    },
    PathNotFound {
        #[serde(deserialize_with = "Option::deserialize")]
        near: Option<P>,
        path: P,
        reason: PathNotFoundResolutionReason,
        #[serde(
            default,
            deserialize_with = "json_serde::deserialize_some",
            skip_serializing_if = "Option::is_none"
        )]
        same_object_at: Option<Nullable<P>>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "kebab-case")]
pub enum UnsupportedSemanticsResolution<P = RepoPath> {
    AttributeDependent,
    CodeFragment { target: ResolutionTarget<P> },
    DuplicateLabel,
    ExternalInventory,
    Fragment { target: ResolutionTarget<P> },
    NetworkPath,
    Query { target: ResolutionTarget<P> },
    SiteRoute,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum KnownCommitVersionScopeKind {
    #[serde(rename = "known-commit")]
    KnownCommit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum KnownPathVersionScopeKind {
    #[serde(rename = "known-path")]
    KnownPath,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnknownPathVersionScopeKind {
    #[serde(rename = "unknown-path")]
    UnknownPath,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum VersionScope<P = RepoPath> {
    KnownCommit {
        commit_oid: Oid,
        kind: KnownCommitVersionScopeKind,
        path: P,
    },
    KnownPath {
        kind: KnownPathVersionScopeKind,
        path: P,
    },
    UnknownPath {
        kind: UnknownPathVersionScopeKind,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeclaredUntrackedResolutionKind {
    #[serde(rename = "declared-untracked")]
    DeclaredUntracked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExternalResolutionKind {
    #[serde(rename = "external")]
    External,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InvalidResolutionKind {
    #[serde(rename = "invalid")]
    Invalid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MissingResolutionKind {
    #[serde(rename = "missing")]
    Missing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResolvedResolutionKind {
    #[serde(rename = "resolved")]
    Resolved,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TypeMismatchResolutionKind {
    #[serde(rename = "type-mismatch")]
    TypeMismatch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnsupportedSemanticsResolutionKind {
    #[serde(rename = "unsupported-semantics")]
    UnsupportedSemantics,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnsupportedTargetResolutionKind {
    #[serde(rename = "unsupported-target")]
    UnsupportedTarget,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnsupportedVersionResolutionKind {
    #[serde(rename = "unsupported-version")]
    UnsupportedVersion,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Resolution<P = RepoPath> {
    DeclaredUntracked {
        declared_by: P,
        kind: DeclaredUntrackedResolutionKind,
        path: P,
    },
    External {
        kind: ExternalResolutionKind,
        reason: ExternalResolutionReason,
    },
    Invalid {
        kind: InvalidResolutionKind,
        reason: InvalidResolutionReason,
    },
    Missing {
        kind: MissingResolutionKind,
        #[serde(flatten)]
        detail: MissingResolution<P>,
    },
    Resolved {
        kind: ResolvedResolutionKind,
        target: ResolutionTarget<P>,
    },
    TypeMismatch {
        kind: TypeMismatchResolutionKind,
        target: ResolutionTarget<P>,
    },
    UnsupportedSemantics {
        kind: UnsupportedSemanticsResolutionKind,
        #[serde(flatten)]
        detail: UnsupportedSemanticsResolution<P>,
    },
    UnsupportedTarget {
        kind: UnsupportedTargetResolutionKind,
        path: P,
        reason: UnsupportedTargetReason,
    },
    UnsupportedVersion {
        kind: UnsupportedVersionResolutionKind,
        scope: VersionScope<P>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Occurrence<P = RepoPath> {
    pub adapter_id: Adapter,
    pub block_kind: BlockKind,
    pub document: P,
    #[serde(
        default,
        deserialize_with = "json_serde::deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub external_destination: Option<String>,
    pub intent: TargetIntent<P>,
    pub observation_id: Digest,
    pub observation_id_input: ObservationIdInput<P>,
    pub resolution: Resolution<P>,
    pub source_construct: SourceConstruct,
    pub source_projection_digest: Digest,
    pub source_span: SourceSpan,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorrelationAlternatives<P = RepoPath> {
    pub base: Vec<Occurrence<P>>,
    pub candidate: Vec<Occurrence<P>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, strum::AsRefStr)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum Correlation {
    Ambiguous,
    Candidate,
    Exact,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, strum::AsRefStr)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum CorrelationReason {
    ExactDocumentRenameUnchangedProjection,
    MultipleCounterparts,
    NewObservation,
    RemovedObservation,
    SameExtractionKeyAndProjection,
    SameIntentSourceChanged,
    SameIntentUnchangedProjection,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, strum::AsRefStr)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum SourceChange {
    Added,
    Changed,
    Equal,
    Removed,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, strum::AsRefStr)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum TargetChange {
    BecameMissing,
    Changed,
    Equal,
    NewlyResolved,
    NotComparable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, strum::AsRefStr)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum Impact {
    DependencyAndSubjectCochanged,
    DependencyChangedSubjectUnchanged,
    NewObservation,
    None,
    NotApplicable,
    ObservationCorrelationAmbiguous,
    ReferenceResolved,
    RemovedObservation,
    SubjectChanged,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservationComparison<P = RepoPath> {
    pub alternatives: CorrelationAlternatives<P>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub base: Option<Occurrence<P>>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub candidate: Option<Occurrence<P>>,
    pub correlation: Correlation,
    pub correlation_reason: CorrelationReason,
    pub impact: Impact,
    pub source_change: SourceChange,
    pub target_change: TargetChange,
}
