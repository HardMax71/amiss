use std::collections::BTreeMap;

use amiss_wire::controls::{GitMode, SourceConstruct};
use amiss_wire::digest::Digest;
use amiss_wire::model::{Adapter, RepoPath};

use crate::resolve::{Intent, Resolution};

/// One side's occurrence as correlation sees it: its identity, where it
/// lives, what it extracted, and how it resolved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
    pub id: Digest,
    pub adapter_contract_digest: Digest,
    pub document: RepoPath,
    pub span: (usize, usize),
    pub display: crate::scan::SpanDisplay,
    pub block_kind: amiss_md::extract::BlockKind,
    pub node_path: Vec<usize>,
    pub adapter: Adapter,
    pub construct: SourceConstruct,
    pub intent: Intent,
    /// The destination after the format's own decoding, which is what a fetcher
    /// would request, kept only for a reference the engine leaves to another
    /// layer so that layer can read it without the tree.
    pub external_destination: Option<String>,
    pub raw_destination: String,
    pub raw_destination_digest: Digest,
    pub projection_digest: Digest,
    pub resolution: Resolution,
    pub fragment_span: Option<(usize, usize)>,
    pub path_span: Option<(usize, usize)>,
}

/// One snapshot side: its observations and, for the rename rule, every
/// classified document's mode and raw-evidence digest.
#[derive(Clone, Debug, Default)]
pub struct Side {
    pub observations: Vec<Observation>,
    pub documents: BTreeMap<RepoPath, (GitMode, Digest)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::AsRefStr)]
#[strum(serialize_all = "kebab-case")]
pub enum Outcome {
    Exact,
    Candidate,
    Ambiguous,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::AsRefStr)]
#[strum(serialize_all = "kebab-case")]
pub enum Reason {
    SameExtractionKeyAndProjection,
    SameIntentUnchangedProjection,
    SameIntentSourceChanged,
    ExactDocumentRenameUnchangedProjection,
    MultipleCounterparts,
    NewObservation,
    RemovedObservation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::AsRefStr)]
#[strum(serialize_all = "kebab-case")]
pub enum SourceChange {
    Equal,
    Changed,
    Unknown,
    Added,
    Removed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::AsRefStr)]
#[strum(serialize_all = "kebab-case")]
pub enum TargetChange {
    Equal,
    Changed,
    NewlyResolved,
    BecameMissing,
    NotComparable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::AsRefStr)]
#[strum(serialize_all = "kebab-case")]
pub enum Impact {
    None,
    SubjectChanged,
    DependencyChangedSubjectUnchanged,
    DependencyAndSubjectCochanged,
    ReferenceResolved,
    NotApplicable,
    ObservationCorrelationAmbiguous,
    NewObservation,
    RemovedObservation,
}

/// One comparison row: a primary on each present side, alternatives only for
/// ambiguity, and the target derivation for exact and candidate pairs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Comparison {
    pub outcome: Outcome,
    pub reason: Reason,
    pub source_change: SourceChange,
    pub base: Option<Observation>,
    pub candidate: Option<Observation>,
    pub alternatives_base: Vec<Observation>,
    pub alternatives_candidate: Vec<Observation>,
    pub target_change: TargetChange,
    pub impact: Impact,
}
