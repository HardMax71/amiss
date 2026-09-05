use amiss_wire::digest::{Digest, hj};
use amiss_wire::json::Value;
use amiss_wire::model::{Oid, RepoPath, RepoPathText};
pub use amiss_wire::report::model::{Attribution, LocationSide, PolicyStep};
use amiss_wire::report::{Disposition, FindingKind, FixKind};

use super::finding::{key_digest, key_value};
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum FindingKeyScope {
    Document(RepoPath),
    Observation(Digest),
    Reference {
        document: RepoPath,
        source_construct: amiss_wire::controls::SourceConstruct,
        commit_oid: Option<Oid>,
        repository_path: Option<RepoPath>,
        target_kind: Option<amiss_wire::controls::TargetKind>,
        query_digest: Option<Digest>,
        fragment_digest: Option<Digest>,
        source_projection_digest: Digest,
    },
    Control {
        path: Option<RepoPath>,
        rule_id: String,
    },
}

/// A finding's typed identity and its canonical digest. Construction owns the
/// key projection, so a digest cannot be paired with another key input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindingKey {
    kind: FindingKind,
    scope: FindingKeyScope,
    digest: Digest,
}

impl FindingKey {
    pub(super) fn new(kind: FindingKind, scope: FindingKeyScope) -> Self {
        let digest = key_digest(&key_value(kind, &scope));
        Self {
            kind,
            scope,
            digest,
        }
    }

    #[must_use]
    pub const fn kind(&self) -> FindingKind {
        self.kind
    }

    #[must_use]
    pub const fn digest(&self) -> Digest {
        self.digest
    }

    pub(crate) fn to_value(&self) -> Value {
        key_value(self.kind, &self.scope)
    }
}

/// One canonical fact and the digest computed from those exact bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindingFact {
    value: Value,
    digest: Digest,
}

impl FindingFact {
    pub(crate) fn new(key: &FindingKey, evidence: Value) -> Self {
        let value = Value::object(vec![
            ("schema".to_owned(), Value::string(FACT_SCHEMA.to_owned())),
            (
                "finding_kind".to_owned(),
                Value::string(key.kind().as_ref().to_owned()),
            ),
            ("key_input".to_owned(), key.to_value()),
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
    pub(super) key: FindingKey,
    pub attribution: Attribution,
    pub(super) base_fact: Option<FindingFact>,
    pub(super) candidate_fact: Option<FindingFact>,
    pub member_count: u64,
    pub observation_ids: Vec<Digest>,
    pub location: Location,
    pub configured_disposition: Disposition,
    pub effective_disposition: Disposition,
    pub steps: Vec<PolicyStep>,
    pub debt: Option<DebtApplied>,
    pub waiver: Option<WaiverApplied>,
    pub(super) fix: Option<FindingFix>,
}

impl Finding {
    #[must_use]
    pub const fn kind(&self) -> FindingKind {
        self.key.kind()
    }

    #[must_use]
    pub const fn key(&self) -> &FindingKey {
        &self.key
    }

    #[must_use]
    pub const fn base_fact(&self) -> Option<&FindingFact> {
        self.base_fact.as_ref()
    }

    #[must_use]
    pub const fn candidate_fact(&self) -> Option<&FindingFact> {
        self.candidate_fact.as_ref()
    }

    pub(crate) const fn fix(&self) -> Option<&FindingFix> {
        self.fix.as_ref()
    }
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
