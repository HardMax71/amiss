use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

use crate::controls::TargetKind;

use crate::extraction::BlockKind;
use crate::extraction::SourceConstruct;
use crate::model::Digest;
use crate::model::{Adapter, AddressKind, Oid};
use crate::resolution::{Target, VersionScope};

use super::RepoPath;
use crate::report::ReportDefect;
use crate::resolution::{
    ExternalReference, InvalidReference, UnsupportedSemanticsReason, UnsupportedTargetTag,
};

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
#[serde(deny_unknown_fields)]
pub struct TargetIntent<P = RepoPath> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_oid: Option<Oid>,
    pub external_scheme: Option<String>,
    pub fragment_digest: Option<Digest>,
    pub kind: super::super::IntentKind,
    pub query_digest: Option<Digest>,
    pub raw_destination_digest: Digest,
    pub repository_path: Option<P>,
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

/// One unevaluated meaning: the reason it was left, and the target the reason
/// located when it located one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnsupportedSemanticsResolution<P = RepoPath> {
    pub reason: UnsupportedSemanticsReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<Target<P>>,
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
        reason: ExternalReference,
    },
    Invalid {
        reason: InvalidReference,
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
        reason: UnsupportedTargetTag,
    },
    UnsupportedVersion {
        scope: VersionScope<P>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Occurrence<P = RepoPath, R = Resolution<P>> {
    pub block_kind: BlockKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_destination: Option<String>,
    pub observation_id: Digest,
    pub observation_id_input: ObservationIdInput<P>,
    pub resolution: R,
    pub source_span: SourceSpan,
}

/// What each tree held for one correlated reference: one occurrence both
/// trees hold, or each side on its own.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub enum Sides<P = RepoPath, R = Resolution<P>> {
    Each(Box<Pair<Occurrence<P, R>>>),
    Same(Box<Occurrence<P, R>>),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pair<T> {
    pub base: Option<T>,
    pub candidate: Option<T>,
}

/// The occurrence each side holds; a same row holds one for both.
#[must_use]
pub fn occurrences<P, R>(comparison: &ObservationComparison<P, R>) -> Pair<&Occurrence<P, R>> {
    match &comparison.sides {
        Sides::Each(pair) => Pair {
            base: pair.base.as_ref(),
            candidate: pair.candidate.as_ref(),
        },
        Sides::Same(occurrence) => Pair {
            base: Some(occurrence),
            candidate: Some(occurrence),
        },
    }
}

/// The writer spells an equal pair as `same`, so a reader refuses an `each`
/// pair whose sides are equal.
///
/// # Errors
///
/// `Noncanonical` for an `each` pair whose two occurrences are equal.
pub fn comparisons_valid<P: PartialEq, R: PartialEq>(
    comparisons: &[ObservationComparison<P, R>],
) -> Result<(), ReportDefect> {
    let equal = comparisons.iter().any(|comparison| {
        matches!(
            &comparison.sides,
            Sides::Each(pair) if pair.base.is_some() && pair.base == pair.candidate
        )
    });
    if equal {
        Err(ReportDefect::Noncanonical)
    } else {
        Ok(())
    }
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
#[serde(deny_unknown_fields)]
pub struct ObservationComparison<P = RepoPath, R = Resolution<P>> {
    pub alternatives: CorrelationAlternatives<P, R>,
    pub correlation: Correlation,
    pub correlation_reason: CorrelationReason,
    pub impact: Impact,
    pub sides: Sides<P, R>,
    pub source_change: SourceChange,
    pub target_change: TargetChange,
}
