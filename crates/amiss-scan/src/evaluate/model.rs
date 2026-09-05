use amiss_wire::digest::{Digest, hj};
use amiss_wire::json::Value;
use amiss_wire::model::{RepoPath, RepoPathText};
use amiss_wire::report::model::FindingKeyInput;
pub use amiss_wire::report::model::{Attribution, LocationSide, PolicyStep};
use amiss_wire::report::{Disposition, FixKind};

use super::finding::key_value;
use super::{FACT_DOMAIN, FACT_SCHEMA};
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
pub struct FindingFact {
    value: Value,
    digest: Digest,
}

impl FindingFact {
    pub(crate) fn new(key: &FindingKeyInput<RepoPath>, evidence: Value) -> Self {
        let value = Value::object(vec![
            ("schema".to_owned(), Value::string(FACT_SCHEMA.to_owned())),
            (
                "finding_kind".to_owned(),
                Value::string(key.finding_kind.as_ref().to_owned()),
            ),
            ("key_input".to_owned(), key_value(key)),
            ("evidence".to_owned(), evidence),
        ]);
        let digest = hj(FACT_DOMAIN, &value);
        Self { value, digest }
    }

    #[must_use]
    pub const fn digest(&self) -> Digest {
        self.digest
    }

    pub(crate) const fn value(&self) -> &Value {
        &self.value
    }
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
pub struct Finding {
    pub key_input: FindingKeyInput<RepoPath>,
    pub finding_key: Digest,
    pub attribution: Attribution,
    pub base_fact: Option<FindingFact>,
    pub candidate_fact: Option<FindingFact>,
    pub member_count: u64,
    pub observation_ids: Vec<Digest>,
    pub location: Location,
    pub configured_disposition: Disposition,
    pub effective_disposition: Disposition,
    pub steps: Vec<PolicyStep>,
    pub debt: Option<DebtApplied>,
    pub waiver: Option<WaiverApplied>,
    pub(crate) fix: Option<FindingFix>,
}

/// A valid active debt item applied to this finding, retained as adoption
/// provenance even when its residual equals the incoming disposition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DebtApplied {
    pub item: amiss_wire::controls::DebtItem,
    pub snapshot_digest: Digest,
    pub adoption_tree: amiss_wire::model::TreeIdentity,
}

/// A valid selected waiver applied to this finding: exactly `fail -> warn`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WaiverApplied {
    pub item: amiss_wire::controls::WaiverItem,
    pub bundle_digest: Digest,
}
