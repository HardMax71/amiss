use serde::{Deserialize, Serialize};

use crate::controls::{SourceConstruct, TargetKind};
use crate::digest::Digest;
use crate::extraction::BlockKind;
use crate::model::{Adapter, Oid};
use crate::resolution::BlobMode;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
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
pub struct TargetIntent {
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
    pub repository_path: Option<RepoPath>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub target_kind: Option<TargetKind>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservationIdInput {
    pub adapter_contract_digest: Digest,
    pub adapter_id: Adapter,
    pub document: RepoPath,
    pub extracted_intent: TargetIntent,
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ResolutionTarget {
    Blob {
        content: ResolutionContent,
        mode: BlobMode,
        path: RepoPath,
    },
    Tree {
        path: RepoPath,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "kebab-case")]
pub enum MissingResolution {
    HeadingAnchorNotFound {
        #[serde(deserialize_with = "Option::deserialize")]
        near: Option<String>,
        path: RepoPath,
    },
    LabelNotDeclared,
    LineFragmentOutOfRange {
        path: RepoPath,
    },
    PathNotFound {
        #[serde(deserialize_with = "Option::deserialize")]
        near: Option<RepoPath>,
        path: RepoPath,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        same_object_at: Option<RepoPath>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UnsupportedTargetReason {
    Gitlink,
    Symlink,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "kebab-case")]
pub enum UnsupportedSemanticsResolution {
    AttributeDependent,
    CodeFragment { target: ResolutionTarget },
    DuplicateLabel,
    ExternalInventory,
    Fragment { target: ResolutionTarget },
    NetworkPath,
    Query { target: ResolutionTarget },
    SiteRoute,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum VersionScope {
    KnownCommit { commit_oid: Oid, path: RepoPath },
    KnownPath { path: RepoPath },
    UnknownPath,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InvalidResolutionReason {
    BackslashSeparator,
    DecodedPathControl,
    EncodedSlash,
    FragmentEncoding,
    PathTraversal,
    PercentEncoding,
    Syntax,
    Uri,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExternalResolutionReason {
    ForeignRepository,
    IntersphinxInventory,
    SiteBuild,
    Url,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Resolution {
    DeclaredUntracked {
        declared_by: RepoPath,
        path: RepoPath,
    },
    External {
        reason: ExternalResolutionReason,
    },
    Invalid {
        reason: InvalidResolutionReason,
    },
    Missing {
        #[serde(flatten)]
        detail: MissingResolution,
    },
    Resolved {
        target: ResolutionTarget,
    },
    TypeMismatch {
        target: ResolutionTarget,
    },
    UnsupportedSemantics {
        #[serde(flatten)]
        detail: UnsupportedSemanticsResolution,
    },
    UnsupportedTarget {
        path: RepoPath,
        reason: UnsupportedTargetReason,
    },
    UnsupportedVersion {
        scope: VersionScope,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Occurrence {
    pub adapter_id: Adapter,
    pub block_kind: BlockKind,
    pub document: RepoPath,
    #[serde(
        default,
        deserialize_with = "json_serde::deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub external_destination: Option<String>,
    pub intent: TargetIntent,
    pub observation_id: Digest,
    pub observation_id_input: ObservationIdInput,
    pub resolution: Resolution,
    pub source_construct: SourceConstruct,
    pub source_projection_digest: Digest,
    pub source_span: SourceSpan,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorrelationAlternatives {
    pub base: Vec<Occurrence>,
    pub candidate: Vec<Occurrence>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Correlation {
    Ambiguous,
    Candidate,
    Exact,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CorrelationReason {
    ExactDocumentRenameUnchangedProjection,
    MultipleCounterparts,
    NewObservation,
    RemovedObservation,
    SameExtractionKeyAndProjection,
    SameIntentSourceChanged,
    SameIntentUnchangedProjection,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceChange {
    Added,
    Changed,
    Equal,
    Removed,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TargetChange {
    BecameMissing,
    Changed,
    Equal,
    NewlyResolved,
    NotComparable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
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
pub struct ObservationComparison {
    pub alternatives: CorrelationAlternatives,
    #[serde(deserialize_with = "Option::deserialize")]
    pub base: Option<Occurrence>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub candidate: Option<Occurrence>,
    pub correlation: Correlation,
    pub correlation_reason: CorrelationReason,
    pub impact: Impact,
    pub source_change: SourceChange,
    pub target_change: TargetChange,
}
