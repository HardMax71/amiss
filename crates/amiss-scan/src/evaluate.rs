use std::collections::BTreeMap;

use amiss_wire::digest::{Digest, hj};
use amiss_wire::json::Value;
use amiss_wire::model::RepoPath;
use amiss_wire::report::{Disposition, ErrorDetail, FindingKind};
use amiss_wire::resolution::{
    BlobContent, BlobTarget, Missing, Resolution, Target, TargetTag, UnsupportedSemantics,
    UnsupportedTarget, VersionScope,
};
use strum::IntoDiscriminant;

use crate::correlate::{Comparison, Impact, Observation, Outcome};
use crate::observe;
use crate::scan::SpanDisplay;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocationSide {
    Base,
    Candidate,
    Control,
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

/// One constructed finding: its key, its facts where the reference scope
/// defines them, its aggregation, and its built-in dispositions. Policy
/// steps beyond the built-in table live with the control layer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub kind: FindingKind,
    pub key_input: Value,
    pub finding_key: Digest,
    pub attribution: Attribution,
    pub base_fact: Option<(Value, Digest)>,
    pub candidate_fact: Option<(Value, Digest)>,
    pub member_count: u64,
    pub observation_ids: Vec<Digest>,
    pub location: Location,
    pub configured_disposition: Disposition,
    pub effective_disposition: Disposition,
    pub steps: Vec<PolicyStep>,
    pub debt: Option<DebtApplied>,
    pub waiver: Option<WaiverApplied>,
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

fn nullable_path(path: Option<&RepoPath>) -> Value {
    path.map_or(Value::Null, RepoPath::to_value)
}

fn key_digest(input: &Value) -> Digest {
    hj(FINDING_KEY_DOMAIN, input)
}

fn key_input(kind: FindingKind, scope: Value) -> (Value, Digest) {
    let input = Value::Object(vec![
        (
            "schema".to_owned(),
            Value::String(FINDING_KEY_SCHEMA.to_owned()),
        ),
        (
            "finding_kind".to_owned(),
            Value::String(kind.as_str().to_owned()),
        ),
        ("scope".to_owned(), scope),
    ]);
    let digest = key_digest(&input);
    (input, digest)
}

fn document_scope(path: &RepoPath) -> Value {
    Value::Object(vec![
        ("kind".to_owned(), Value::String("document".to_owned())),
        ("document".to_owned(), path.to_value()),
    ])
}

fn observation_scope(id: Digest) -> Value {
    Value::Object(vec![
        ("kind".to_owned(), Value::String("observation".to_owned())),
        ("observation_id".to_owned(), Value::String(id.to_string())),
    ])
}

/// The structural reference scope: document, construct, the repository
/// projection of the intent, and the containing source projection. Line and
/// column are excluded, so moving a construct keeps its key, while changing
/// the broken target resolves the old key and introduces a new one.
fn reference_scope(observation: &Observation) -> Value {
    let intent = &observation.intent;
    Value::Object(vec![
        ("kind".to_owned(), Value::String("reference".to_owned())),
        ("document".to_owned(), observation.document.to_value()),
        (
            "source_construct".to_owned(),
            Value::String(observation.construct.as_str().to_owned()),
        ),
        (
            "normalized_target_intent".to_owned(),
            Value::Object(vec![
                (
                    "kind".to_owned(),
                    Value::String("repository-path".to_owned()),
                ),
                (
                    "path".to_owned(),
                    intent
                        .repository_path
                        .as_ref()
                        .map_or_else(|| Value::String(String::new()), RepoPath::to_value),
                ),
                (
                    "target_kind".to_owned(),
                    Value::String(
                        intent
                            .target_kind
                            .map_or("either", amiss_wire::controls::TargetKind::as_str)
                            .to_owned(),
                    ),
                ),
                (
                    "query_digest".to_owned(),
                    observe::query_digest(intent)
                        .map_or(Value::Null, |digest| Value::String(digest.to_string())),
                ),
                (
                    "fragment_digest".to_owned(),
                    observe::fragment_digest(intent)
                        .map_or(Value::Null, |digest| Value::String(digest.to_string())),
                ),
            ]),
        ),
        (
            "occurrence".to_owned(),
            Value::Object(vec![
                (
                    "kind".to_owned(),
                    Value::String("source-projection".to_owned()),
                ),
                (
                    "source_projection_digest".to_owned(),
                    Value::String(observation.projection_digest.to_string()),
                ),
            ]),
        ),
    ])
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
            Missing::PathNotFound { path } | Missing::LineFragmentOutOfRange { path } => {
                reasoned_resolution(
                    resolution.discriminant().as_ref(),
                    missing.discriminant().as_ref(),
                    vec![("path", path.to_value())],
                )
            }
        },
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
    members.push(("kind".to_owned(), Value::String(kind.to_owned())));
    members.extend(
        fields
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value)),
    );
    Value::Object(members)
}

fn reasoned_resolution(kind: &str, reason: &str, fields: Vec<(&str, Value)>) -> Value {
    let mut fields = fields;
    fields.insert(0, ("reason", Value::String(reason.to_owned())));
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
        UnsupportedSemantics::SiteRoute | UnsupportedSemantics::NetworkPath => {
            reasoned_resolution(kind, semantics.discriminant().as_ref(), Vec::new())
        }
    }
}

fn target_value(target: &Target<RepoPath>) -> Value {
    match target {
        Target::Tree { path } => Value::Object(vec![
            (
                "kind".to_owned(),
                Value::String(target.discriminant().as_ref().to_owned()),
            ),
            ("path".to_owned(), path.to_value()),
        ]),
        Target::Blob(blob) => blob_target_value(blob),
    }
}

fn blob_target_value(blob: &BlobTarget<RepoPath>) -> Value {
    Value::Object(vec![
        (
            "kind".to_owned(),
            Value::String(TargetTag::Blob.as_ref().to_owned()),
        ),
        ("path".to_owned(), blob.path.to_value()),
        (
            "mode".to_owned(),
            Value::String(blob.mode.as_ref().to_owned()),
        ),
        ("content".to_owned(), blob_content_value(blob.content)),
    ])
}

fn blob_content_value(content: BlobContent) -> Value {
    match content {
        BlobContent::Available {
            raw_digest,
            projection_digest,
        } => Value::Object(vec![
            (
                "kind".to_owned(),
                Value::String(content.discriminant().as_ref().to_owned()),
            ),
            (
                "raw_digest".to_owned(),
                Value::String(raw_digest.to_string()),
            ),
            (
                "projection_digest".to_owned(),
                Value::String(projection_digest.to_string()),
            ),
        ]),
        BlobContent::LfsPointer { raw_digest } => Value::Object(vec![
            (
                "kind".to_owned(),
                Value::String(content.discriminant().as_ref().to_owned()),
            ),
            (
                "raw_digest".to_owned(),
                Value::String(raw_digest.to_string()),
            ),
        ]),
    }
}

fn version_scope_value(scope: &VersionScope<RepoPath>) -> Value {
    match scope {
        VersionScope::KnownPath { path } => Value::Object(vec![
            (
                "kind".to_owned(),
                Value::String(scope.discriminant().as_ref().to_owned()),
            ),
            ("path".to_owned(), path.to_value()),
        ]),
        VersionScope::UnknownPath => Value::Object(vec![(
            "kind".to_owned(),
            Value::String(scope.discriminant().as_ref().to_owned()),
        )]),
    }
}

fn reference_fact(
    kind: FindingKind,
    key: &Value,
    observation: &Observation,
    multiplicity: u64,
) -> (Value, Digest) {
    let fact = Value::Object(vec![
        ("schema".to_owned(), Value::String(FACT_SCHEMA.to_owned())),
        (
            "finding_kind".to_owned(),
            Value::String(kind.as_str().to_owned()),
        ),
        ("key_input".to_owned(), key.clone()),
        (
            "evidence".to_owned(),
            Value::Object(vec![
                ("kind".to_owned(), Value::String("reference".to_owned())),
                ("resolution".to_owned(), resolution_value(observation)),
                (
                    "occurrence_multiplicity".to_owned(),
                    Value::Integer(i64::try_from(multiplicity).unwrap_or(i64::MAX)),
                ),
            ]),
        ),
    ]);
    let digest = hj(FACT_DOMAIN, &fact);
    (fact, digest)
}

const fn structural_kind(resolution: &crate::resolve::Resolution) -> Option<FindingKind> {
    match resolution {
        Resolution::Missing(_) => Some(FindingKind::ExplicitTargetMissing),
        Resolution::TypeMismatch(_) => Some(FindingKind::ExplicitTargetTypeMismatch),
        Resolution::Resolved(_)
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
        Resolution::External(_) => Some(FindingKind::ExternalOutOfScope),
        Resolution::Resolved(_) | Resolution::Missing(_) | Resolution::TypeMismatch(_) => None,
    }
}

fn observation_location(observation: &Observation, side: LocationSide) -> Location {
    Location {
        side,
        path: Some(observation.document.clone()),
        span: Some(observation.span),
        display: Some(observation.display),
    }
}

fn simple(
    kind: FindingKind,
    scope: Value,
    attribution: Attribution,
    candidate_fact: Option<(Value, Digest)>,
    ids: Vec<Digest>,
    location: Location,
    enforce: bool,
) -> Finding {
    let (key_value, digest) = key_input(kind, scope);
    let configured = kind.built_in_disposition(enforce);
    Finding {
        kind,
        key_input: key_value,
        finding_key: digest,
        attribution,
        base_fact: None,
        candidate_fact,
        member_count: 1,
        observation_ids: ids,
        location,
        configured_disposition: configured,
        effective_disposition: configured,
        debt: None,
        waiver: None,
        steps: vec![built_in_step(kind, enforce)],
    }
}

/// Step one: built-in always starts from `record` and applies the defaults
/// table for the selected profile.
fn built_in_step(kind: FindingKind, enforce: bool) -> PolicyStep {
    let profile = if enforce { "enforce" } else { "observe" };
    PolicyStep {
        source: "built-in",
        rule_id: format!("scanner-policy-defaults/{}/{profile}", kind.as_str()),
        before: Disposition::Record,
        after: kind.built_in_disposition(enforce),
    }
}

/// Every candidate occurrence: primaries plus alternatives on the candidate
/// side of every comparison.
fn candidate_occurrences(comparisons: &[Comparison]) -> Vec<&Observation> {
    let mut out: Vec<&Observation> = Vec::new();
    for comparison in comparisons {
        out.extend(comparison.candidate.iter());
        out.extend(comparison.alternatives_candidate.iter());
    }
    out
}

fn base_occurrences(comparisons: &[Comparison]) -> Vec<&Observation> {
    let mut out: Vec<&Observation> = Vec::new();
    for comparison in comparisons {
        out.extend(comparison.base.iter());
        out.extend(comparison.alternatives_base.iter());
    }
    out
}

/// The exact ordinary-finding projection: document findings, occurrence
/// boundaries, structural aggregation by key with attribution, and the
/// comparison-derived removal, ambiguity, and impact findings. Analysis
/// errors never enter, and the result is in canonical finding-key order.
#[must_use]
pub fn evaluate(
    documents: &[DocumentInput],
    comparisons: &[Comparison],
    enforce: bool,
) -> Vec<Finding> {
    let (findings, _no_exceptions) = evaluate_with_policy(
        documents,
        comparisons,
        enforce,
        &crate::policy::Effects::default(),
        &[],
    );
    findings
}

/// The reserved governed declaration boundary: control-scoped at the affected
/// document under the one closed rule, with null base state, candidate
/// `unsupported`, exact node multiplicity, and the sorted distinct source
/// digests.
fn governed_finding(seed: &GovernedSeed, enforce: bool) -> Finding {
    let scope = Value::Object(vec![
        ("kind".to_owned(), Value::String("control".to_owned())),
        ("control_path".to_owned(), seed.document.to_value()),
        (
            "rule_id".to_owned(),
            Value::String("unsupported/governed-claim".to_owned()),
        ),
    ]);
    let (key_value, digest) = key_input(FindingKind::UnsupportedCapability, scope);
    let sources: Vec<Value> = seed
        .sources
        .iter()
        .map(|(source_digest, multiplicity)| {
            Value::Object(vec![
                (
                    "multiplicity".to_owned(),
                    Value::Integer(i64::try_from(*multiplicity).unwrap_or(i64::MAX)),
                ),
                (
                    "digest".to_owned(),
                    Value::String(source_digest.to_string()),
                ),
            ])
        })
        .collect();
    let fact = Value::Object(vec![
        ("schema".to_owned(), Value::String(FACT_SCHEMA.to_owned())),
        (
            "finding_kind".to_owned(),
            Value::String(FindingKind::UnsupportedCapability.as_str().to_owned()),
        ),
        ("key_input".to_owned(), key_value.clone()),
        (
            "evidence".to_owned(),
            Value::Object(vec![
                ("kind".to_owned(), Value::String("control".to_owned())),
                ("control_path".to_owned(), seed.document.to_value()),
                (
                    "rule_id".to_owned(),
                    Value::String("unsupported/governed-claim".to_owned()),
                ),
                ("base_control_state".to_owned(), Value::Null),
                ("base_control_digest".to_owned(), Value::Null),
                (
                    "candidate_control_state".to_owned(),
                    Value::Object(vec![
                        (
                            "schema".to_owned(),
                            Value::String("amiss/scanner-control-state".to_owned()),
                        ),
                        (
                            "rule_id".to_owned(),
                            Value::String("unsupported/governed-claim".to_owned()),
                        ),
                        (
                            "path".to_owned(),
                            seed.document
                                .as_str()
                                .map_or(Value::Null, |path| Value::String(path.to_owned())),
                        ),
                        ("sources".to_owned(), Value::Array(sources)),
                        ("state".to_owned(), Value::String("unsupported".to_owned())),
                    ]),
                ),
                ("candidate_control_digest".to_owned(), Value::Null),
                ("exception".to_owned(), Value::Null),
            ]),
        ),
    ]);
    let fact_digest = hj(FACT_DOMAIN, &fact);
    let configured = FindingKind::UnsupportedCapability.built_in_disposition(enforce);
    Finding {
        kind: FindingKind::UnsupportedCapability,
        key_input: key_value,
        finding_key: digest,
        attribution: Attribution::NotApplicable,
        base_fact: None,
        candidate_fact: Some((fact, fact_digest)),
        member_count: seed.member_count,
        observation_ids: Vec::new(),
        location: Location {
            side: LocationSide::Candidate,
            path: Some(seed.document.clone()),
            span: seed.representative_span,
            display: seed.representative_display,
        },
        configured_disposition: configured,
        effective_disposition: configured,
        debt: None,
        waiver: None,
        steps: vec![built_in_step(FindingKind::UnsupportedCapability, enforce)],
    }
}

/// One candidate document's reserved governed definitions: the exact node
/// count and the distinct source digests with their multiplicities, plus the
/// least location as the representative.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GovernedSeed {
    pub document: RepoPath,
    pub member_count: u64,
    pub sources: Vec<(Digest, u64)>,
    pub representative_span: Option<(usize, usize)>,
    pub representative_display: Option<SpanDisplay>,
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
    enforce: bool,
    policy: &crate::policy::Effects,
    governed: &[GovernedSeed],
) -> (Vec<Finding>, Vec<ErrorDetail>) {
    let mut findings = ordinary(documents, comparisons, enforce);
    for seed in governed {
        findings.push(governed_finding(seed, enforce));
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
    let (exception_findings, errors) = apply_exceptions(&mut findings, policy, enforce);
    findings.extend(exception_findings);
    for seed in &policy.controls {
        findings.push(control_finding(seed, policy, enforce));
    }
    findings.sort_by_key(|finding| finding.finding_key);
    (findings, errors)
}

fn tree_value(tree: &amiss_wire::model::TreeIdentity) -> Value {
    Value::Object(vec![
        (
            "object_format".to_owned(),
            Value::String(
                match tree.object_format {
                    amiss_wire::model::ObjectFormat::Sha1 => "sha1",
                    amiss_wire::model::ObjectFormat::Sha256 => "sha256",
                }
                .to_owned(),
            ),
        ),
        ("tree_oid".to_owned(), Value::String(tree.tree_oid.clone())),
    ])
}

fn debt_diagnostic(
    item: &amiss_wire::controls::DebtItem,
    context: &crate::policy::DebtContext,
    current_fact_digest: Digest,
) -> Value {
    Value::Object(vec![
        ("kind".to_owned(), Value::String("debt".to_owned())),
        (
            "debt_id".to_owned(),
            Value::String(item.debt_id.as_str().to_owned()),
        ),
        (
            "debt_snapshot_digest".to_owned(),
            Value::String(context.digest.to_string()),
        ),
        (
            "adoption_tree".to_owned(),
            tree_value(&context.adoption_tree),
        ),
        (
            "accepted_fact_digest".to_owned(),
            Value::String(item.accepted_fact_digest.to_string()),
        ),
        (
            "current_fact_digest".to_owned(),
            Value::String(current_fact_digest.to_string()),
        ),
        (
            "owner".to_owned(),
            Value::String(item.owner.as_str().to_owned()),
        ),
        ("reason".to_owned(), Value::String(item.reason.clone())),
        (
            "created_at".to_owned(),
            Value::String(item.created_at.as_str().to_owned()),
        ),
        (
            "expires_at".to_owned(),
            Value::String(item.expires_at.as_str().to_owned()),
        ),
    ])
}

fn waiver_diagnostic(
    item: &amiss_wire::controls::WaiverItem,
    bundle_digest: Digest,
    current_fact_digest: Option<Digest>,
) -> Value {
    Value::Object(vec![
        ("kind".to_owned(), Value::String("waiver".to_owned())),
        (
            "waiver_id".to_owned(),
            Value::String(item.waiver_id.as_str().to_owned()),
        ),
        (
            "waiver_bundle_digest".to_owned(),
            Value::String(bundle_digest.to_string()),
        ),
        (
            "candidate_tree".to_owned(),
            tree_value(&item.candidate_tree),
        ),
        (
            "finding_key".to_owned(),
            Value::String(item.finding_key.to_string()),
        ),
        (
            "authorized_fact_digest".to_owned(),
            Value::String(item.authorized_fact_digest.to_string()),
        ),
        (
            "current_fact_digest".to_owned(),
            current_fact_digest.map_or(Value::Null, |digest| Value::String(digest.to_string())),
        ),
        (
            "owner".to_owned(),
            Value::String(item.owner.as_str().to_owned()),
        ),
        (
            "issuer".to_owned(),
            Value::String(item.issuer.as_str().to_owned()),
        ),
        ("reason".to_owned(), Value::String(item.reason.clone())),
        (
            "created_at".to_owned(),
            Value::String(item.created_at.as_str().to_owned()),
        ),
        (
            "not_before".to_owned(),
            Value::String(item.not_before.as_str().to_owned()),
        ),
        (
            "expires_at".to_owned(),
            Value::String(item.expires_at.as_str().to_owned()),
        ),
        (
            "residual_disposition".to_owned(),
            Value::String("warn".to_owned()),
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
            targets.entry(finding.finding_key).or_insert(index);
        }
    }
    targets
}

fn candidate_digest_of(finding: &Finding) -> Option<Digest> {
    finding.candidate_fact.as_ref().map(|(_, digest)| *digest)
}

/// Steps four and five with their defect findings: exact active debt, one
/// exact selected waiver, the closed defect rows in construction order, and
/// the overlap law that applies neither when both are valid.
fn apply_exceptions(
    findings: &mut [Finding],
    policy: &crate::policy::Effects,
    enforce: bool,
) -> (Vec<Finding>, Vec<ErrorDetail>) {
    let mut extra: Vec<Finding> = Vec::new();
    if policy.debt.is_none() && policy.waiver.is_none() {
        return (extra, Vec::new());
    }
    let Some(instant) = policy
        .time
        .as_ref()
        .map(|time| time.statement.evaluation_instant.clone())
    else {
        return (extra, Vec::new());
    };
    let targets = exception_targets(findings);
    let debt_valid = debt_pass(findings, &targets, policy, enforce, &instant, &mut extra);
    let waiver_valid = waiver_pass(findings, &targets, policy, enforce, &instant, &mut extra);
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

/// The debt item pass: expiry before fact inequality, both defect rows able
/// to coexist, and a finding absent from the snapshot receiving no
/// treatment.
fn debt_pass(
    findings: &[Finding],
    targets: &BTreeMap<Digest, usize>,
    policy: &crate::policy::Effects,
    enforce: bool,
    instant: &amiss_wire::model::UtcInstant,
    extra: &mut Vec<Finding>,
) -> BTreeMap<Digest, usize> {
    let mut debt_valid: BTreeMap<Digest, usize> = BTreeMap::new();
    let Some(context) = &policy.debt else {
        return debt_valid;
    };
    for (index, item) in context.items.iter().enumerate() {
        let Some(target) = targets.get(&item.finding_key).copied() else {
            continue;
        };
        let Some(current) = findings.get(target).and_then(candidate_digest_of) else {
            continue;
        };
        let expired = *instant >= item.expires_at;
        let equal = current == item.accepted_fact_digest;
        if expired {
            extra.push(control_row(
                FindingKind::DebtExpired,
                format!("debt/{}/expired", item.debt_id.as_str()),
                None,
                (None, Some(context.digest)),
                debt_diagnostic(item, context, current),
                enforce,
            ));
        }
        if !equal {
            extra.push(control_row(
                FindingKind::DebtWorsened,
                format!("debt/{}/fact", item.debt_id.as_str()),
                None,
                (None, Some(context.digest)),
                debt_diagnostic(item, context, current),
                enforce,
            ));
        }
        if !expired && equal {
            debt_valid.insert(item.finding_key, index);
        }
    }
    debt_valid
}

/// The selected-waiver pass: the closed defect rows in construction order,
/// with the finding-bound rows applicable only when the key names a current
/// candidate finding.
fn waiver_pass(
    findings: &[Finding],
    targets: &BTreeMap<Digest, usize>,
    policy: &crate::policy::Effects,
    enforce: bool,
    instant: &amiss_wire::model::UtcInstant,
    extra: &mut Vec<Finding>,
) -> BTreeMap<Digest, usize> {
    let mut waiver_valid: BTreeMap<Digest, usize> = BTreeMap::new();
    let Some(context) = &policy.waiver else {
        return waiver_valid;
    };
    for (index, item) in context.items.iter().enumerate() {
        if item.candidate_tree != context.candidate_tree {
            continue;
        }
        let target = targets.get(&item.finding_key).copied();
        let current = target.and_then(|found| findings.get(found).and_then(candidate_digest_of));
        let mut defects: Vec<&'static str> = Vec::new();
        if *instant < item.not_before {
            defects.push("not-yet");
        }
        if *instant >= item.expires_at {
            defects.push("expired");
        }
        if !context.authorized_issuers.contains(&item.issuer) {
            defects.push("issuer");
        }
        if !context
            .waivable_kinds
            .contains(&item.authorized_fact.finding_kind())
        {
            defects.push("kind");
        }
        if item.owner == item.issuer {
            defects.push("same-owner");
        }
        if let Some(found) = target {
            if findings.get(found).is_some_and(|finding| {
                finding.kind.as_str() != item.authorized_fact.finding_kind().as_str()
            }) {
                defects.push("key");
            }
            if current != Some(item.authorized_fact_digest) {
                defects.push("fact");
            }
        }
        for suffix in &defects {
            extra.push(control_row(
                FindingKind::WaiverInvalid,
                format!("waiver/{}/{suffix}", item.waiver_id.as_str()),
                None,
                (None, Some(context.digest)),
                waiver_diagnostic(item, context.digest, current),
                enforce,
            ));
        }
        if defects.is_empty() && target.is_some() {
            waiver_valid.insert(item.finding_key, index);
        }
    }
    waiver_valid
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
        let debt_item = debt_valid.get(&finding.finding_key).copied();
        let waiver_item = waiver_valid.get(&finding.finding_key).copied();
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
    let Some((_kind, target)) = raised.iter().find(|(kind, _)| *kind == finding.kind) else {
        return;
    };
    let current = finding
        .steps
        .last()
        .map_or(finding.configured_disposition, |step| step.after);
    if *target > current {
        finding.steps.push(PolicyStep {
            source,
            rule_id: format!("{prefix}/{}", finding.kind.as_str()),
            before: current,
            after: *target,
        });
    }
}

fn control_finding(
    seed: &crate::policy::ControlSeed,
    policy: &crate::policy::Effects,
    enforce: bool,
) -> Finding {
    control_row(
        seed.kind,
        seed.rule_id.clone(),
        seed.control_path.clone(),
        (policy.base_digest, policy.candidate_digest),
        Value::Null,
        enforce,
    )
}

/// One control-scoped finding under an exact rule: the fact embeds the
/// governing control's digests and, for exception defects, the complete
/// typed diagnostic.
fn control_row(
    kind: FindingKind,
    rule_id: String,
    control_path: Option<RepoPath>,
    control_digests: (Option<Digest>, Option<Digest>),
    exception: Value,
    enforce: bool,
) -> Finding {
    let scope = Value::Object(vec![
        ("kind".to_owned(), Value::String("control".to_owned())),
        (
            "control_path".to_owned(),
            nullable_path(control_path.as_ref()),
        ),
        ("rule_id".to_owned(), Value::String(rule_id.clone())),
    ]);
    let (key_value, digest) = key_input(kind, scope);
    let nullable_digest = |value: Option<Digest>| {
        value.map_or(Value::Null, |digest| Value::String(digest.to_string()))
    };
    let fact = Value::Object(vec![
        ("schema".to_owned(), Value::String(FACT_SCHEMA.to_owned())),
        (
            "finding_kind".to_owned(),
            Value::String(kind.as_str().to_owned()),
        ),
        ("key_input".to_owned(), key_value.clone()),
        (
            "evidence".to_owned(),
            Value::Object(vec![
                ("kind".to_owned(), Value::String("control".to_owned())),
                (
                    "control_path".to_owned(),
                    nullable_path(control_path.as_ref()),
                ),
                ("rule_id".to_owned(), Value::String(rule_id)),
                ("base_control_state".to_owned(), Value::Null),
                (
                    "base_control_digest".to_owned(),
                    nullable_digest(control_digests.0),
                ),
                ("candidate_control_state".to_owned(), Value::Null),
                (
                    "candidate_control_digest".to_owned(),
                    nullable_digest(control_digests.1),
                ),
                ("exception".to_owned(), exception),
            ]),
        ),
    ]);
    let fact_digest = hj(FACT_DOMAIN, &fact);
    let configured = kind.built_in_disposition(enforce);
    Finding {
        kind,
        key_input: key_value,
        finding_key: digest,
        attribution: Attribution::NotApplicable,
        base_fact: None,
        candidate_fact: Some((fact, fact_digest)),
        member_count: 1,
        observation_ids: Vec::new(),
        location: Location {
            side: LocationSide::Control,
            path: control_path,
            span: None,
            display: None,
        },
        configured_disposition: configured,
        effective_disposition: configured,
        debt: None,
        waiver: None,
        steps: vec![built_in_step(kind, enforce)],
    }
}

fn ordinary(
    documents: &[DocumentInput],
    comparisons: &[Comparison],
    enforce: bool,
) -> Vec<Finding> {
    let mut findings: Vec<Finding> = Vec::new();

    for document in documents {
        document_findings(document, enforce, &mut findings);
    }

    for observation in candidate_occurrences(comparisons) {
        let mut emit = |kind: FindingKind| {
            findings.push(simple(
                kind,
                observation_scope(observation.id),
                Attribution::NotApplicable,
                None,
                vec![observation.id],
                observation_location(observation, LocationSide::Candidate),
                enforce,
            ));
        };
        if let Some(kind) = boundary_kind(&observation.resolution) {
            emit(kind);
        }
        if observation.resolution.is_lfs_pointer() {
            emit(FindingKind::UnsupportedTargetKind);
        }
    }

    structural_findings(comparisons, enforce, &mut findings);

    for comparison in comparisons {
        comparison_findings(comparison, enforce, &mut findings);
    }

    findings.sort_by_key(|finding| finding.finding_key);
    findings
}

fn document_findings(document: &DocumentInput, enforce: bool, findings: &mut Vec<Finding>) {
    let path = &document.path;
    if document.base.is_some() && document.candidate.is_none() {
        findings.push(simple(
            FindingKind::DocumentRemoved,
            document_scope(path),
            Attribution::NotApplicable,
            None,
            Vec::new(),
            Location {
                side: LocationSide::Base,
                path: Some(path.clone()),
                span: None,
                display: None,
            },
            enforce,
        ));
        return;
    }
    let candidate_location = || Location {
        side: LocationSide::Candidate,
        path: Some(path.clone()),
        span: None,
        display: None,
    };
    match document.candidate {
        None | Some(DocumentSide::ExcludedBuiltIn) => {}
        Some(DocumentSide::Unsupported) => {
            findings.push(simple(
                FindingKind::UnsupportedDocumentFormat,
                document_scope(path),
                Attribution::NotApplicable,
                None,
                Vec::new(),
                candidate_location(),
                enforce,
            ));
        }
        Some(DocumentSide::Scanned {
            mdx_regions,
            html_regions,
            extracted_references,
        }) => {
            if mdx_regions > 0 {
                findings.push(simple(
                    FindingKind::OpaqueMdxRegion,
                    document_scope(path),
                    Attribution::NotApplicable,
                    None,
                    Vec::new(),
                    candidate_location(),
                    enforce,
                ));
            }
            if html_regions > 0 {
                findings.push(simple(
                    FindingKind::OpaqueHtmlRegion,
                    document_scope(path),
                    Attribution::NotApplicable,
                    None,
                    Vec::new(),
                    candidate_location(),
                    enforce,
                ));
            }
            if extracted_references == 0 {
                findings.push(simple(
                    FindingKind::UnlinkedDocument,
                    document_scope(path),
                    Attribution::NotApplicable,
                    None,
                    Vec::new(),
                    candidate_location(),
                    enforce,
                ));
            }
        }
    }
}

/// The adoption-reproduction projection: every structural key among the
/// observations, with its occurrence count and the fact digest computed at
/// that count. Exactly one occurrence with the accepted fact digest is the
/// reproduction requirement.
#[must_use]
pub fn structural_facts(observations: &[Observation]) -> BTreeMap<Digest, (u64, Digest)> {
    let mut groups: BTreeMap<Digest, KeyGroup<'_>> = BTreeMap::new();
    for observation in observations {
        collect_structural(&mut groups, observation, false);
    }
    groups
        .into_iter()
        .filter_map(|(digest, group)| {
            let first = group.candidate.first()?;
            let multiplicity = u64::try_from(group.candidate.len()).unwrap_or(u64::MAX);
            let (key_value, _same) = key_input(group.kind, group.scope.clone());
            let (_fact, fact_digest) = reference_fact(group.kind, &key_value, first, multiplicity);
            Some((digest, (multiplicity, fact_digest)))
        })
        .collect()
}

struct KeyGroup<'a> {
    kind: FindingKind,
    scope: Value,
    base: Vec<&'a Observation>,
    candidate: Vec<&'a Observation>,
}

fn collect_structural<'a>(
    groups: &mut BTreeMap<Digest, KeyGroup<'a>>,
    observation: &'a Observation,
    is_base: bool,
) {
    let Some(kind) = structural_kind(&observation.resolution) else {
        return;
    };
    let scope = reference_scope(observation);
    let (_input, digest) = key_input(kind, scope.clone());
    let group = groups.entry(digest).or_insert_with(|| KeyGroup {
        kind,
        scope,
        base: Vec::new(),
        candidate: Vec::new(),
    });
    if is_base {
        group.base.push(observation);
    } else {
        group.candidate.push(observation);
    }
}

/// Step three: structural kinds aggregate independently by key across both
/// sides, one finding per key with at least one included side. Attribution
/// follows fact presence and equality, and a base-only projection is forced
/// to record so a deletion cannot retain an old blocking failure.
fn structural_findings(comparisons: &[Comparison], enforce: bool, findings: &mut Vec<Finding>) {
    let mut groups: BTreeMap<Digest, KeyGroup<'_>> = BTreeMap::new();
    for observation in candidate_occurrences(comparisons) {
        collect_structural(&mut groups, observation, false);
    }
    for observation in base_occurrences(comparisons) {
        collect_structural(&mut groups, observation, true);
    }

    for (digest, group) in groups {
        let (key_value, _same) = key_input(group.kind, group.scope.clone());
        let base_fact = group.base.first().map(|observation| {
            reference_fact(
                group.kind,
                &key_value,
                observation,
                u64::try_from(group.base.len()).unwrap_or(u64::MAX),
            )
        });
        let candidate_fact = group.candidate.first().map(|observation| {
            reference_fact(
                group.kind,
                &key_value,
                observation,
                u64::try_from(group.candidate.len()).unwrap_or(u64::MAX),
            )
        });
        let attribution = match (&base_fact, &candidate_fact) {
            (None, Some(_)) => Attribution::Introduced,
            (Some(_), None) => Attribution::Resolved,
            (Some((left, _)), Some((right, _))) if left == right => Attribution::PreExisting,
            (Some(_), Some(_)) => Attribution::Unknown,
            (None, None) => Attribution::NotApplicable,
        };
        if attribution == Attribution::NotApplicable {
            continue;
        }

        let members = if group.candidate.is_empty() {
            &group.base
        } else {
            &group.candidate
        };
        let mut ids: Vec<Digest> = members.iter().map(|observation| observation.id).collect();
        ids.sort_unstable();
        let representative = members
            .iter()
            .min_by_key(|observation| {
                (
                    observation.document.clone(),
                    observation.span,
                    observation.id,
                )
            })
            .copied();
        let side = if group.candidate.is_empty() {
            LocationSide::Base
        } else {
            LocationSide::Candidate
        };
        let location = representative.map_or(
            Location {
                side,
                path: None,
                span: None,
                display: None,
            },
            |observation| observation_location(observation, side),
        );

        let configured = if attribution == Attribution::Resolved {
            Disposition::Record
        } else {
            group.kind.built_in_disposition(enforce)
        };
        findings.push(Finding {
            kind: group.kind,
            key_input: key_value,
            finding_key: digest,
            attribution,
            base_fact,
            candidate_fact,
            member_count: u64::try_from(members.len()).unwrap_or(u64::MAX),
            observation_ids: ids,
            location,
            configured_disposition: configured,
            effective_disposition: configured,
            debt: None,
            waiver: None,
            steps: if attribution == Attribution::Resolved {
                vec![PolicyStep {
                    source: "resolved-projection",
                    rule_id: "resolved-projection".to_owned(),
                    before: Disposition::Record,
                    after: Disposition::Record,
                }]
            } else {
                vec![built_in_step(group.kind, enforce)]
            },
        });
    }
}

/// Step four: one removal per base-only comparison, one ambiguity per
/// ambiguous comparison, and the three named impact findings only.
fn comparison_findings(comparison: &Comparison, enforce: bool, findings: &mut Vec<Finding>) {
    if comparison.outcome == Outcome::None
        && comparison.base.is_some()
        && comparison.candidate.is_none()
    {
        if let Some(base) = &comparison.base {
            findings.push(simple(
                FindingKind::ExplicitReferenceRemoved,
                observation_scope(base.id),
                Attribution::NotApplicable,
                None,
                vec![base.id],
                observation_location(base, LocationSide::Base),
                enforce,
            ));
        }
        return;
    }
    let primary = comparison
        .candidate
        .as_ref()
        .map(|observation| (observation, LocationSide::Candidate))
        .or_else(|| {
            comparison
                .base
                .as_ref()
                .map(|observation| (observation, LocationSide::Base))
        });
    let Some((primary, side)) = primary else {
        return;
    };
    if comparison.outcome == Outcome::Ambiguous {
        findings.push(simple(
            FindingKind::ObservationCorrelationAmbiguous,
            observation_scope(primary.id),
            Attribution::NotApplicable,
            None,
            vec![primary.id],
            observation_location(primary, side),
            enforce,
        ));
        return;
    }
    let impact_kind = match comparison.impact {
        Impact::DependencyChangedSubjectUnchanged => {
            Some(FindingKind::DependencyChangedSubjectUnchanged)
        }
        Impact::DependencyAndSubjectCochanged => Some(FindingKind::DependencyAndSubjectCochanged),
        Impact::SubjectChanged => Some(FindingKind::SubjectChanged),
        Impact::None
        | Impact::ReferenceResolved
        | Impact::NotApplicable
        | Impact::ObservationCorrelationAmbiguous
        | Impact::NewObservation
        | Impact::RemovedObservation => None,
    };
    if let Some(kind) = impact_kind {
        findings.push(simple(
            kind,
            observation_scope(primary.id),
            Attribution::NotApplicable,
            None,
            vec![primary.id],
            observation_location(primary, side),
            enforce,
        ));
    }
}
