use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

use crate::controls::{SourceConstruct, TargetKind};
use crate::extraction::BlockKind;
use crate::model::Digest;
use crate::model::{Adapter, Oid};
pub use crate::resolution::{
    ExternalReference as ExternalResolutionReason, InvalidReference as InvalidResolutionReason,
    UnsupportedTargetTag as UnsupportedTargetReason,
};
use crate::resolution::{TaggedBlobTarget, Target, VersionScope};

use super::RepoPath;

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum ObservationIdInputSchema {
    #[strum(serialize = "amiss/scanner-observation-id-input")]
    Current,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum StructuralAddressSchema {
    #[strum(serialize = "amiss/scanner-structural-address")]
    Current,
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
    strum::IntoStaticStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum AddressKind {
    AsciidocBlockPath,
    MarkdownAstNodePath,
    MdxAstNodePath,
    RstBlockPath,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructuralAddress {
    pub address_kind: AddressKind,
    pub construct_index: u64,
    pub duplicate_index: u64,
    pub node_path: Vec<u64>,
    pub schema: StructuralAddressSchema,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceSpan {
    pub end_byte: u64,
    pub end_column: u64,
    pub end_line: u64,
    pub start_byte: u64,
    pub start_column: u64,
    pub start_line: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, bound(deserialize = "P: Deserialize<'de>"))]
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
#[serde(deny_unknown_fields)]
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

pub type MissingResolution<P = RepoPath> = crate::controls::MissingResolution<P>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "kebab-case", deny_unknown_fields)]
pub enum UnsupportedSemanticsResolution<P = RepoPath> {
    AttributeDependent {},
    CodeFragment { target: Target<P> },
    DuplicateLabel {},
    ExternalInventory {},
    Fragment { target: TaggedBlobTarget<P> },
    NetworkPath {},
    Query { target: Target<P> },
    SiteRoute {},
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, strum::AsRefStr)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
#[strum(serialize_all = "kebab-case")]
pub enum Resolution<P = RepoPath> {
    DeclaredUntracked {
        declared_by: P,
        path: P,
    },
    External {
        reason: ExternalResolutionReason,
    },
    Invalid {
        reason: InvalidResolutionReason,
    },
    Missing(MissingResolution<P>),
    Resolved {
        target: Target<P>,
    },
    TypeMismatch {
        target: Target<P>,
    },
    UnsupportedSemantics(UnsupportedSemanticsResolution<P>),
    UnsupportedTarget {
        path: P,
        reason: UnsupportedTargetReason,
    },
    UnsupportedVersion {
        scope: VersionScope<P>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Occurrence<P = RepoPath, R = Resolution<P>> {
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
    pub resolution: R,
    pub source_construct: SourceConstruct,
    pub source_projection_digest: Digest,
    pub source_span: SourceSpan,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorrelationAlternatives<P = RepoPath, R = Resolution<P>> {
    pub base: Vec<Occurrence<P, R>>,
    pub candidate: Vec<Occurrence<P, R>>,
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
pub enum Correlation {
    Ambiguous,
    Candidate,
    Exact,
    None,
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
pub enum CorrelationReason {
    ExactDocumentRenameUnchangedProjection,
    MultipleCounterparts,
    NewObservation,
    RemovedObservation,
    SameExtractionKeyAndProjection,
    SameIntentSourceChanged,
    SameIntentUnchangedProjection,
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
pub enum SourceChange {
    Added,
    Changed,
    Equal,
    Removed,
    Unknown,
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
pub enum TargetChange {
    BecameMissing,
    Changed,
    Equal,
    NewlyResolved,
    NotComparable,
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
#[serde(
    deny_unknown_fields,
    bound(deserialize = "P: Deserialize<'de>, R: Deserialize<'de>")
)]
pub struct ObservationComparison<P = RepoPath, R = Resolution<P>> {
    pub alternatives: CorrelationAlternatives<P, R>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub base: Option<Occurrence<P, R>>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub candidate: Option<Occurrence<P, R>>,
    pub correlation: Correlation,
    pub correlation_reason: CorrelationReason,
    pub impact: Impact,
    pub source_change: SourceChange,
    pub target_change: TargetChange,
}
