use std::collections::BTreeMap;

use amiss_wire::controls::Profile;
use amiss_wire::digest::{Digest, hj};
use amiss_wire::json::Value;
use amiss_wire::model::RepoPath;
use amiss_wire::report::{Disposition, ErrorDetail, FindingKind, FixKind};
use amiss_wire::resolution::{
    BlobContent, BlobTarget, Missing, Resolution, Target, TargetTag, UnsupportedSemantics,
    UnsupportedTarget, VersionScope,
};
use strum::IntoDiscriminant;

use crate::correlate::{Comparison, Observation, Outcome};
use crate::scan::SpanDisplay;

mod claims;
mod control;
mod debt;
mod documents;
mod finding;
mod references;
mod waiver;

use claims::claim_finding;
pub(crate) use claims::source_multiplicities;
pub use claims::{ClaimGroup, claim_groups};
pub use control::GovernedSeed;
use control::{control_finding, governed_finding};
use debt::debt_pass;
use documents::document_findings;
use finding::{key_digest, key_value, observation_location, observation_scope, simple};
pub use references::structural_facts;
use references::{comparison_findings, structural_findings};
use waiver::waiver_pass;

pub const FINDING_KEY_SCHEMA: &str = "amiss/scanner-finding-key-input";
pub const FINDING_KEY_DOMAIN: &str = "amiss/scanner-finding-key";
pub const FACT_SCHEMA: &str = "amiss/scanner-fact";
pub const FACT_DOMAIN: &str = "amiss/scanner-fact";

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Attribution {
    Introduced,
    PreExisting,
    Resolved,
    Unknown,
    NotApplicable,
}

impl Attribution {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Introduced => "introduced",
            Self::PreExisting => "pre-existing",
            Self::Resolved => "resolved",
            Self::Unknown => "unknown",
            Self::NotApplicable => "not-applicable",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::AsRefStr)]
#[strum(serialize_all = "kebab-case")]
pub enum LocationSide {
    Base,
    Candidate,
    Control,
    Global,
}

/// One policy-trace step. Adjacent steps chain exactly: each `before` equals
/// the preceding `after`, the built-in step always starts from `record`, and
/// steps appear only when applicable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolicyStep {
    pub source: &'static str,
    pub rule_id: String,
    pub before: Disposition,
    pub after: Disposition,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    pub side: LocationSide,
    pub path: Option<RepoPath>,
    pub span: Option<(usize, usize)>,
    pub display: Option<SpanDisplay>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum FindingKeyScope {
    Document(RepoPath),
    Observation(Digest),
    Reference {
        document: RepoPath,
        source_construct: amiss_wire::controls::SourceConstruct,
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
    fn new(kind: FindingKind, scope: FindingKeyScope) -> Self {
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
    pub(crate) path: RepoPath,
    pub(crate) span: (usize, usize),
    pub(crate) replacement: String,
    pub(crate) kind: FixKind,
}

/// One constructed finding: its key, its facts where the reference scope
/// defines them, its aggregation, and its built-in dispositions. Policy
/// steps beyond the built-in table live with the control layer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    key: FindingKey,
    pub attribution: Attribution,
    base_fact: Option<FindingFact>,
    candidate_fact: Option<FindingFact>,
    pub member_count: u64,
    pub observation_ids: Vec<Digest>,
    pub location: Location,
    pub configured_disposition: Disposition,
    pub effective_disposition: Disposition,
    pub steps: Vec<PolicyStep>,
    pub debt: Option<DebtApplied>,
    pub waiver: Option<WaiverApplied>,
    fix: Option<FindingFix>,
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

fn resolution_value(observation: &Observation) -> Value {
    resolution_row(&observation.resolution)
}

pub(crate) fn resolution_row(resolution: &crate::resolve::Resolution) -> Value {
    match resolution {
        Resolution::Resolved(target) | Resolution::TypeMismatch(target) => resolution_object(
            resolution.discriminant().as_ref(),
            vec![("target", target_value(target))],
        ),
        Resolution::Missing(missing) => match missing {
            Missing::LineFragmentOutOfRange { path } => reasoned_resolution(
                resolution.discriminant().as_ref(),
                missing.discriminant().as_ref(),
                vec![("path", path.to_value())],
            ),
            Missing::PathNotFound { path, near } => reasoned_resolution(
                resolution.discriminant().as_ref(),
                missing.discriminant().as_ref(),
                vec![
                    ("path", path.to_value()),
                    (
                        "near",
                        near.as_ref().map_or(Value::Null, RepoPath::to_value),
                    ),
                ],
            ),
            Missing::HeadingAnchorNotFound { path, near } => reasoned_resolution(
                resolution.discriminant().as_ref(),
                missing.discriminant().as_ref(),
                vec![
                    ("path", path.to_value()),
                    (
                        "near",
                        near.as_ref()
                            .map_or(Value::Null, |anchor| Value::string(anchor.clone())),
                    ),
                ],
            ),
            Missing::LabelNotDeclared => reasoned_resolution(
                resolution.discriminant().as_ref(),
                missing.discriminant().as_ref(),
                Vec::new(),
            ),
        },
        Resolution::DeclaredUntracked(declared) => resolution_object(
            resolution.discriminant().as_ref(),
            vec![
                ("path", declared.path.to_value()),
                ("declared_by", declared.declared_by.to_value()),
            ],
        ),
        Resolution::UnsupportedTarget(target) => {
            unsupported_target_value(resolution.discriminant().as_ref(), target)
        }
        Resolution::UnsupportedSemantics(semantics) => {
            unsupported_semantics_value(resolution.discriminant().as_ref(), semantics)
        }
        Resolution::UnsupportedVersion(scope) => resolution_object(
            resolution.discriminant().as_ref(),
            vec![("scope", version_scope_value(scope))],
        ),
        Resolution::Invalid(reason) => reasoned_resolution(
            resolution.discriminant().as_ref(),
            reason.as_ref(),
            Vec::new(),
        ),
        Resolution::External(reason) => reasoned_resolution(
            resolution.discriminant().as_ref(),
            reason.as_ref(),
            Vec::new(),
        ),
    }
}

fn resolution_object(kind: &str, fields: Vec<(&str, Value)>) -> Value {
    let mut members = Vec::with_capacity(fields.len().saturating_add(1));
    members.push(("kind".to_owned(), Value::string(kind.to_owned())));
    members.extend(
        fields
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value)),
    );
    Value::object(members)
}

fn reasoned_resolution(kind: &str, reason: &str, fields: Vec<(&str, Value)>) -> Value {
    let mut fields = fields;
    fields.insert(0, ("reason", Value::string(reason.to_owned())));
    resolution_object(kind, fields)
}

fn unsupported_target_value(kind: &str, target: &UnsupportedTarget<RepoPath>) -> Value {
    let path = match target {
        UnsupportedTarget::Symlink { path } | UnsupportedTarget::Gitlink { path } => path,
    };
    reasoned_resolution(
        kind,
        target.discriminant().as_ref(),
        vec![("path", path.to_value())],
    )
}

fn unsupported_semantics_value(kind: &str, semantics: &UnsupportedSemantics<RepoPath>) -> Value {
    match semantics {
        UnsupportedSemantics::Query(target) | UnsupportedSemantics::CodeFragment(target) => {
            reasoned_resolution(
                kind,
                semantics.discriminant().as_ref(),
                vec![("target", target_value(target))],
            )
        }
        UnsupportedSemantics::Fragment(blob) => reasoned_resolution(
            kind,
            semantics.discriminant().as_ref(),
            vec![("target", blob_target_value(blob))],
        ),
        UnsupportedSemantics::SiteRoute
        | UnsupportedSemantics::NetworkPath
        | UnsupportedSemantics::AttributeDependent
        | UnsupportedSemantics::DuplicateLabel
        | UnsupportedSemantics::ExternalInventory => {
            reasoned_resolution(kind, semantics.discriminant().as_ref(), Vec::new())
        }
    }
}

fn target_value(target: &Target<RepoPath>) -> Value {
    match target {
        Target::Tree { path } => Value::object(vec![
            (
                "kind".to_owned(),
                Value::string(target.discriminant().as_ref().to_owned()),
            ),
            ("path".to_owned(), path.to_value()),
        ]),
        Target::Blob(blob) => blob_target_value(blob),
    }
}

fn blob_target_value(blob: &BlobTarget<RepoPath>) -> Value {
    Value::object(vec![
        (
            "kind".to_owned(),
            Value::string(TargetTag::Blob.as_ref().to_owned()),
        ),
        ("path".to_owned(), blob.path.to_value()),
        (
            "mode".to_owned(),
            Value::string(blob.mode.as_ref().to_owned()),
        ),
        ("content".to_owned(), blob_content_value(blob.content)),
    ])
}

fn blob_content_value(content: BlobContent) -> Value {
    match content {
        BlobContent::Available {
            raw_digest,
            projection_digest,
        } => Value::object(vec![
            (
                "kind".to_owned(),
                Value::string(content.discriminant().as_ref().to_owned()),
            ),
            (
                "raw_digest".to_owned(),
                Value::string(raw_digest.to_string()),
            ),
            (
                "projection_digest".to_owned(),
                Value::string(projection_digest.to_string()),
            ),
        ]),
        BlobContent::LfsPointer { raw_digest } => Value::object(vec![
            (
                "kind".to_owned(),
                Value::string(content.discriminant().as_ref().to_owned()),
            ),
            (
                "raw_digest".to_owned(),
                Value::string(raw_digest.to_string()),
            ),
        ]),
    }
}

fn version_scope_value(scope: &VersionScope<RepoPath>) -> Value {
    match scope {
        VersionScope::KnownPath { path } => Value::object(vec![
            (
                "kind".to_owned(),
                Value::string(scope.discriminant().as_ref().to_owned()),
            ),
            ("path".to_owned(), path.to_value()),
        ]),
        VersionScope::UnknownPath => Value::object(vec![(
            "kind".to_owned(),
            Value::string(scope.discriminant().as_ref().to_owned()),
        )]),
    }
}

const fn structural_kind(resolution: &crate::resolve::Resolution) -> Option<FindingKind> {
    match resolution {
        Resolution::Missing(_) => Some(FindingKind::ExplicitTargetMissing),
        Resolution::TypeMismatch(_) => Some(FindingKind::ExplicitTargetTypeMismatch),
        Resolution::Resolved(_)
        | Resolution::DeclaredUntracked(_)
        | Resolution::UnsupportedTarget(_)
        | Resolution::UnsupportedSemantics(_)
        | Resolution::UnsupportedVersion(_)
        | Resolution::Invalid(_)
        | Resolution::External(_) => None,
    }
}

/// The occurrence-boundary mapping of step two: which non-structural kind one
/// candidate resolution emits, if any.
const fn boundary_kind(resolution: &crate::resolve::Resolution) -> Option<FindingKind> {
    match resolution {
        Resolution::Invalid(_) => Some(FindingKind::InvalidReference),
        Resolution::UnsupportedSemantics(_) => Some(FindingKind::UnsupportedReferenceSemantics),
        Resolution::UnsupportedVersion(_) => Some(FindingKind::UnsupportedVersionScope),
        Resolution::UnsupportedTarget(_) => Some(FindingKind::UnsupportedTargetKind),
        Resolution::DeclaredUntracked(_) => Some(FindingKind::TargetDeclaredUntracked),
        Resolution::Resolved(_)
        | Resolution::Missing(_)
        | Resolution::TypeMismatch(_)
        | Resolution::External(_) => None,
    }
}

/// The attribution an invalid reference carries on its report row: introduced
/// when the base held no equal invalid destination, pre-existing when it did,
/// unknown under an ambiguous pairing or for an alternative occurrence. The
/// feedback projection used to recompute this and discard it.
fn invalid_attributions(comparisons: &[Comparison]) -> BTreeMap<Digest, Attribution> {
    let mut rows = BTreeMap::new();
    for comparison in comparisons {
        if let Some(candidate) = &comparison.candidate
            && matches!(candidate.resolution, Resolution::Invalid(_))
        {
            let attribution = match comparison.outcome {
                Outcome::Ambiguous => Attribution::Unknown,
                Outcome::Exact | Outcome::Candidate | Outcome::None => comparison
                    .base
                    .as_ref()
                    .filter(|base| {
                        matches!(base.resolution, Resolution::Invalid(_))
                            && base.raw_destination_digest == candidate.raw_destination_digest
                    })
                    .map_or(Attribution::Introduced, |_base| Attribution::PreExisting),
            };
            rows.insert(candidate.id, attribution);
        }
        for candidate in &comparison.alternatives_candidate {
            if matches!(candidate.resolution, Resolution::Invalid(_)) {
                rows.insert(candidate.id, Attribution::Unknown);
            }
        }
    }
    rows
}

/// The exact ordinary-finding projection: document findings, occurrence
/// boundaries, structural aggregation by key with attribution, and the
/// comparison-derived removal, ambiguity, and impact findings. Analysis
/// errors never enter, and the result is in canonical finding-key order.
#[must_use]
pub fn evaluate(
    documents: &[DocumentInput],
    comparisons: &[Comparison],
    profile: Profile,
) -> Vec<Finding> {
    let (findings, _no_exceptions) = evaluate_with_policy(
        documents,
        comparisons,
        profile,
        &crate::policy::Effects::default(),
        &[],
        &[],
    );
    findings
}

/// The full projection with the candidate policy applied: the raise-only
/// repository and floor steps on structural candidate facts, exact debt and
/// waiver application with their defect findings, the weakening and coverage
/// control findings, and one unsupported-capability finding per candidate
/// document holding reserved governed definitions. The returned rows are the
/// exception-overlap errors; any row makes the run incomplete.
#[must_use]
pub fn evaluate_with_policy(
    documents: &[DocumentInput],
    comparisons: &[Comparison],
    profile: Profile,
    policy: &crate::policy::Effects,
    governed: &[GovernedSeed],
    claims: &[ClaimGroup],
) -> (Vec<Finding>, Vec<ErrorDetail>) {
    let mut findings = ordinary(documents, comparisons, profile);
    for seed in governed {
        findings.push(governed_finding(seed, profile));
    }
    for group in claims {
        findings.push(claim_finding(group, profile));
    }
    for finding in &mut findings {
        if finding.attribution == Attribution::Resolved || finding.candidate_fact.is_none() {
            continue;
        }
        apply_raise(finding, &policy.raised, "repository-policy", "repository");
        apply_raise(finding, &policy.floor_raised, "organization-floor", "floor");
        finding.configured_disposition = finding
            .steps
            .last()
            .map_or(finding.configured_disposition, |step| step.after);
        finding.effective_disposition = finding.configured_disposition;
    }
    if profile.introduced_only() {
        for finding in &mut findings {
            if finding.effective_disposition == Disposition::Fail
                && finding.attribution == Attribution::PreExisting
            {
                finding.steps.push(PolicyStep {
                    source: "built-in",
                    rule_id: format!(
                        "scanner-policy-defaults/{}/enforce-introduced",
                        finding.kind().as_ref()
                    ),
                    before: Disposition::Fail,
                    after: Disposition::Warn,
                });
                finding.effective_disposition = Disposition::Warn;
            }
        }
    }
    let (exception_findings, errors) = apply_exceptions(&mut findings, policy, profile);
    findings.extend(exception_findings);
    for seed in &policy.controls {
        findings.push(control_finding(seed, policy, profile));
    }
    findings.sort_by_key(|finding| finding.key.digest());
    (findings, errors)
}

fn tree_value(tree: &amiss_wire::model::TreeIdentity) -> Value {
    Value::object(vec![
        (
            "object_format".to_owned(),
            Value::string(
                match tree.object_format() {
                    amiss_wire::model::ObjectFormat::Sha1 => "sha1",
                    amiss_wire::model::ObjectFormat::Sha256 => "sha256",
                }
                .to_owned(),
            ),
        ),
        (
            "tree_oid".to_owned(),
            Value::string(tree.tree_oid().to_owned()),
        ),
    ])
}

/// Candidate findings exception items may target: exact keys with candidate
/// facts, excluding resolved projections and every scope exceptions cannot
/// touch. First insertion preserves the former linear `position` semantics if
/// an invalid directly constructed finding slice repeats a key.
fn exception_targets(findings: &[Finding]) -> BTreeMap<Digest, usize> {
    let mut targets = BTreeMap::new();
    for (index, finding) in findings.iter().enumerate() {
        if finding.candidate_fact.is_some() {
            targets.entry(finding.key.digest()).or_insert(index);
        }
    }
    targets
}

fn candidate_digest_of(finding: &Finding) -> Option<Digest> {
    finding.candidate_fact.as_ref().map(FindingFact::digest)
}

/// Steps four and five with their defect findings: exact active debt, one
/// exact selected waiver, the closed defect rows in construction order, and
/// the overlap law that applies neither when both are valid.
fn apply_exceptions(
    findings: &mut [Finding],
    policy: &crate::policy::Effects,
    profile: Profile,
) -> (Vec<Finding>, Vec<ErrorDetail>) {
    let mut extra: Vec<Finding> = Vec::new();
    if policy.debt.is_none() && policy.waiver.is_none() {
        return (extra, Vec::new());
    }
    let Some(instant) = policy
        .time
        .as_ref()
        .map(|time| time.statement.evaluation_instant().clone())
    else {
        return (extra, Vec::new());
    };
    let targets = exception_targets(findings);
    let debt_valid = debt_pass(findings, &targets, policy, profile, &instant, &mut extra);
    let waiver_valid = waiver_pass(findings, &targets, policy, profile, &instant, &mut extra);
    let overlap = apply_valid_exceptions(findings, policy, &debt_valid, &waiver_valid);
    let errors = if overlap {
        vec![ErrorDetail {
            code: amiss_wire::report::AnalysisErrorCode::ExceptionOverlap,
            path: None,
            path_bytes: None,
            resource: None,
        }]
    } else {
        Vec::new()
    };
    (extra, errors)
}

/// The application and overlap law: a finding matched by both valid items
/// applies neither and fails control evaluation; a valid debt step is
/// retained even as a no-op; a waiver step is exactly `fail -> warn`.
fn apply_valid_exceptions(
    findings: &mut [Finding],
    policy: &crate::policy::Effects,
    debt_valid: &BTreeMap<Digest, usize>,
    waiver_valid: &BTreeMap<Digest, usize>,
) -> bool {
    let mut overlap = false;
    for finding in findings.iter_mut() {
        let debt_item = debt_valid.get(&finding.key.digest()).copied();
        let waiver_item = waiver_valid.get(&finding.key.digest()).copied();
        match (debt_item, waiver_item) {
            (Some(_), Some(_)) => {
                overlap = true;
            }
            (Some(index), None) => {
                let (Some(context), Some(item)) = (
                    policy.debt.as_ref(),
                    policy.debt.as_ref().and_then(|debt| debt.items.get(index)),
                ) else {
                    continue;
                };
                let current = finding
                    .steps
                    .last()
                    .map_or(finding.configured_disposition, |step| step.after);
                finding.steps.push(PolicyStep {
                    source: "debt-snapshot",
                    rule_id: format!("debt/{}", item.debt_id.as_str()),
                    before: current,
                    after: Disposition::Warn,
                });
                finding.effective_disposition = Disposition::Warn;
                finding.debt = Some(DebtApplied {
                    item: item.clone(),
                    snapshot_digest: context.digest,
                    adoption_tree: context.adoption_tree.clone(),
                });
            }
            (None, Some(index)) => {
                let (Some(context), Some(item)) = (
                    policy.waiver.as_ref(),
                    policy
                        .waiver
                        .as_ref()
                        .and_then(|waiver| waiver.items.get(index)),
                ) else {
                    continue;
                };
                let current = finding
                    .steps
                    .last()
                    .map_or(finding.configured_disposition, |step| step.after);
                if current == Disposition::Fail {
                    finding.steps.push(PolicyStep {
                        source: "waiver-bundle",
                        rule_id: format!("waiver/{}", item.waiver_id.as_str()),
                        before: Disposition::Fail,
                        after: Disposition::Warn,
                    });
                    finding.effective_disposition = Disposition::Warn;
                    finding.waiver = Some(WaiverApplied {
                        item: item.clone(),
                        bundle_digest: context.digest,
                    });
                }
            }
            (None, None) => {}
        }
    }
    overlap
}

/// Steps two and three: a matching rule applies only when strictly raising,
/// and each step's before equals the preceding after.
fn apply_raise(
    finding: &mut Finding,
    raised: &[(FindingKind, Disposition)],
    source: &'static str,
    prefix: &str,
) {
    let Some((_kind, target)) = raised.iter().find(|(kind, _)| *kind == finding.kind()) else {
        return;
    };
    let current = finding
        .steps
        .last()
        .map_or(finding.configured_disposition, |step| step.after);
    if *target > current {
        finding.steps.push(PolicyStep {
            source,
            rule_id: format!("{prefix}/{}", finding.kind().as_ref()),
            before: current,
            after: *target,
        });
    }
}

fn ordinary(
    documents: &[DocumentInput],
    comparisons: &[Comparison],
    profile: Profile,
) -> Vec<Finding> {
    let mut findings: Vec<Finding> = Vec::new();

    for document in documents {
        document_findings(document, profile, &mut findings);
    }

    let invalid = invalid_attributions(comparisons);
    for observation in comparisons.iter().flat_map(|comparison| {
        comparison
            .candidate
            .iter()
            .chain(&comparison.alternatives_candidate)
    }) {
        let attribution = if matches!(observation.resolution, Resolution::Invalid(_)) {
            invalid
                .get(&observation.id)
                .copied()
                .unwrap_or(Attribution::Unknown)
        } else {
            Attribution::NotApplicable
        };
        let mut emit = |kind: FindingKind| {
            findings.push(simple(
                kind,
                observation_scope(observation.id),
                attribution,
                vec![observation.id],
                observation_location(observation, LocationSide::Candidate),
                profile,
            ));
        };
        if let Some(kind) = boundary_kind(&observation.resolution) {
            emit(kind);
        }
        if observation.resolution.is_lfs_pointer() {
            emit(FindingKind::UnsupportedTargetKind);
        }
    }

    structural_findings(comparisons, profile, &mut findings);

    for comparison in comparisons {
        comparison_findings(comparison, profile, &mut findings);
    }

    findings.sort_by_key(|finding| finding.key.digest());
    findings
}
