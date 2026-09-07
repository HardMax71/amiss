use amiss_wire::controls::GitMode;
use amiss_wire::controls::ProjectionSource;
use amiss_wire::digest::Digest;
use amiss_wire::model::{RepoPath, RepoPathText};
use amiss_wire::report::model::DebtApplication;
use amiss_wire::report::model::FindingFactEvidence;
use amiss_wire::report::model::FindingFactInput;
use amiss_wire::report::model::FindingKeyInput;
use amiss_wire::report::model::ProjectionDifference;
use amiss_wire::report::model::RowsProjectionDifference;
use amiss_wire::report::model::WaiverApplication;
pub use amiss_wire::report::model::{Attribution, LocationSide, PolicyStep};
use amiss_wire::report::{Disposition, FixKind};
use amiss_wire::resolution::Resolution;

use crate::scan::SpanDisplay;

/// One document path's paired sides, reduced to what finding construction
/// reads. A failed side never reaches this projection: analysis errors are
/// not findings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentInput {
    pub path: RepoPath,
    pub base: Option<DocumentSide>,
    pub candidate: Option<DocumentSide>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentSide {
    Scanned {
        mdx_regions: u64,
        html_regions: u64,
        extracted_references: u64,
    },
    Unsupported,
    ExcludedBuiltIn,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    pub side: LocationSide,
    pub path: Option<RepoPath>,
    pub span: Option<(usize, usize)>,
    pub display: Option<SpanDisplay>,
}

pub(super) type FindingKeyScope = amiss_wire::report::model::FindingKeyScope<RepoPath>;

/// One canonical fact and the digest computed from those exact bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindingFact<
    E = FindingFactEvidence<
        RepoPath,
        Resolution<RepoPath>,
        ProjectionSource,
        ProjectionDifference<Box<RowsProjectionDifference>>,
        GitMode,
    >,
> {
    pub input: FindingFactInput<FindingKeyInput<RepoPath>, E>,
    pub digest: Digest,
}

/// A repository edit kept as domain data until report serialization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindingFix {
    pub(crate) path: RepoPathText,
    pub(crate) span: (usize, usize),
    pub(crate) replacement: String,
    pub(crate) kind: FixKind,
}

/// One constructed finding: its key, its facts where the reference scope
/// defines them, its aggregation, and its built-in dispositions. Policy
/// steps beyond the built-in table live with the control layer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding<
    E = FindingFactEvidence<
        RepoPath,
        Resolution<RepoPath>,
        ProjectionSource,
        ProjectionDifference<Box<RowsProjectionDifference>>,
        GitMode,
    >,
> {
    pub key_input: FindingKeyInput<RepoPath>,
    pub finding_key: Digest,
    pub attribution: Attribution,
    pub base_fact: Option<FindingFact<E>>,
    pub candidate_fact: Option<FindingFact<E>>,
    pub member_count: u64,
    pub observation_ids: Vec<Digest>,
    pub location: Location,
    pub configured_disposition: Disposition,
    pub effective_disposition: Disposition,
    pub steps: Vec<PolicyStep>,
    pub debt: Option<DebtApplication>,
    pub waiver: Option<WaiverApplication>,
    pub(crate) fix: Option<FindingFix>,
}
