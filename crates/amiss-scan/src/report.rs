use amiss_wire::controls::{ContentAvailability, Profile, ResourceName};
use amiss_wire::digest::{Digest, hj, hj_with_length};
use amiss_wire::json::{Value, canonical, canonical_length};
use amiss_wire::model::{Adapter, RepoPath};
use amiss_wire::report::{
    AnalysisErrorCode, COMPATIBILITY, Disposition, EngineProvenance, ErrorDetail, FindingKind,
    FindingScope, IntentKind, MACHINE_JSON_BYTES, PAYLOAD_SCHEMA, engine_block, error_row_value,
    sandbox_descriptor,
};
pub use amiss_wire::requests::CANDIDATE_IDENTITY_DOMAIN;
use amiss_wire::resolution::Resolution;

use crate::correlate::{Comparison, Observation, Outcome, Reason, SourceChange, TargetChange};
use crate::discovery::{DocumentRecord, DocumentStatus, SnapshotDiscovery, UnsupportedKind};
use crate::evaluate::{
    Attribution, DocumentInput, DocumentSide, Finding, FindingFact, FindingFix, LocationSide,
};
use crate::feedback;
use crate::{Impact, SpanDisplay, observe};

pub const ENVELOPE_SCHEMA: &str = "amiss/scanner-report-envelope";
pub const INDEX_PROJECTION_SCHEMA: &str = "amiss/scanner-index-projection";
pub const SNAPSHOT_SCHEMA: &str = "amiss/scanner-snapshot";

/// The canonical logical-index projection and the synthetic snapshot input
/// built over it, with both digests.
#[must_use]
pub fn synthetic_candidate(
    base_object_format: &'static str,
    base_commit_oid: &str,
    entries: &[(RepoPath, amiss_wire::controls::GitMode, String, bool)],
    skip_worktree_paths: u64,
) -> IndexCandidate {
    let rows: Vec<Value> = entries
        .iter()
        .map(|(path, mode, oid, skip)| {
            let entry_kind = match mode {
                amiss_wire::controls::GitMode::Symlink => "symlink",
                amiss_wire::controls::GitMode::Gitlink => "gitlink",
                amiss_wire::controls::GitMode::RegularFile
                | amiss_wire::controls::GitMode::ExecutableFile
                | amiss_wire::controls::GitMode::Tree => "blob",
            };
            object(vec![
                ("path", path.to_value()),
                ("entry_kind", string(entry_kind)),
                ("git_mode", string(mode.as_str())),
                ("object_format", string(base_object_format)),
                ("object_oid", string(oid)),
                ("skip_worktree", Value::Bool(*skip)),
            ])
        })
        .collect();
    let projection = object(vec![
        ("schema", string(INDEX_PROJECTION_SCHEMA)),
        ("entries", Value::array(rows)),
    ]);
    let projection_digest = hj(INDEX_PROJECTION_SCHEMA, &projection);
    let snapshot_input = object(vec![
        ("schema", string(SNAPSHOT_SCHEMA)),
        ("kind", string("index")),
        ("identity_scope", string("complete-logical-index")),
        ("base_object_format", string(base_object_format)),
        ("base_commit_oid", string(base_commit_oid)),
        ("index_projection_digest", digest_value(projection_digest)),
    ]);
    let snapshot_digest = hj(SNAPSHOT_SCHEMA, &snapshot_input);
    IndexCandidate {
        base_object_format,
        base_commit_oid: base_commit_oid.to_owned(),
        projection_digest,
        entry_count: u64::try_from(entries.len()).unwrap_or(u64::MAX),
        snapshot_digest,
        skip_worktree_paths,
    }
}

/// One snapshot's identity in the evaluation block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotIdentity {
    pub object_format: &'static str,
    pub commit_oid: String,
    pub tree_oid: String,
}

/// The candidate side of the evaluation identity: a Git commit, the
/// synthetic complete logical staged index, or the unavailable projection an
/// incomplete index run reports with its closed reasons.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CandidateBlock {
    Commit(SnapshotIdentity),
    Index(IndexCandidate),
    Unavailable(Vec<&'static str>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexCandidate {
    pub base_object_format: &'static str,
    pub base_commit_oid: String,
    pub projection_digest: Digest,
    pub entry_count: u64,
    pub snapshot_digest: Digest,
    pub skip_worktree_paths: u64,
}

/// The diagnostic request digests of the wrapper lane: present exactly for
/// streams captured completely, and rendered only inside unavailable
/// snapshot and controls values. The in-process CLI has none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RequestDigests {
    pub evaluation: Option<Digest>,
    pub snapshot: Option<Digest>,
    pub controls: Option<Digest>,
}

/// The run identity a complete local report carries, plus the acquired
/// policy effects and, for an invalid-policy run, the unavailable-controls
/// reason.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Setup {
    pub engine: EngineProvenance,
    pub profile: Profile,
    pub repository: Option<amiss_wire::model::RepositoryIdentity>,
    pub forge: Option<amiss_wire::model::ForgeDialect>,
    pub candidate_ref: Option<String>,
    pub target_ref: Option<String>,
    pub default_branch_ref: Option<String>,
    pub base: SnapshotIdentity,
    pub candidate: CandidateBlock,
    pub policy: crate::policy::Effects,
    pub controls_unavailable: Option<&'static str>,
    pub requests: RequestDigests,
}

/// A constructed report: the envelope value, the payload digest, and the
/// result the process must exit with. The wire is never materialized here;
/// a binary streams the envelope through its reserved fatal serializer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Built {
    pub envelope: Value,
    pub payload_digest: Digest,
    pub status: &'static str,
    pub exit_code: i64,
}

impl Built {
    /// The exact report wire, `JCS(envelope) || LF`, for callers that must
    /// hold the bytes.
    #[must_use]
    pub fn wire(&self) -> Vec<u8> {
        let mut wire = canonical(&self.envelope);
        wire.push(b'\n');
        wire
    }
}

fn string(text: &str) -> Value {
    Value::string(text.to_owned())
}

fn nullable(text: Option<&str>) -> Value {
    text.map_or(Value::Null, string)
}

fn nullable_path(path: Option<&RepoPath>) -> Value {
    path.map_or(Value::Null, RepoPath::to_value)
}

fn integer(value: u64) -> Value {
    Value::Integer(i64::try_from(value).unwrap_or(i64::MAX))
}

fn digest_value(digest: Digest) -> Value {
    Value::string(digest.to_string())
}

fn object(members: Vec<(&str, Value)>) -> Value {
    Value::object(
        members
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    )
}

fn source_span_value(span: (usize, usize), display: SpanDisplay) -> Value {
    object(vec![
        (
            "start_byte",
            integer(u64::try_from(span.0).unwrap_or(u64::MAX)),
        ),
        (
            "end_byte",
            integer(u64::try_from(span.1).unwrap_or(u64::MAX)),
        ),
        ("start_line", integer(display.start_line)),
        ("start_column", integer(display.start_column)),
        ("end_line", integer(display.end_line)),
        ("end_column", integer(display.end_column)),
    ])
}

fn occurrence_value(observation: &Observation) -> Value {
    let identity = observe::ObservationIdentity {
        adapter: observation.adapter,
        contract_digest: observation.adapter_contract_digest,
        document: &observation.document,
        construct: observation.construct,
        node_path: &observation.node_path,
        projection_digest: observation.projection_digest,
        intent: &observation.intent,
        raw_destination_digest: observation.raw_destination_digest,
    };
    let id = observe::observation_digest(&identity);
    let input = observe::observation_input(&identity);
    let resolution = crate::evaluate::resolution_row(&observation.resolution);
    let mut members = vec![
        ("observation_id", digest_value(id)),
        ("observation_id_input", input),
        ("adapter_id", string(observation.adapter.adapter_id())),
        ("document", observation.document.to_value()),
        ("source_construct", string(observation.construct.as_str())),
        (
            "source_span",
            source_span_value(observation.span, observation.display),
        ),
        ("block_kind", string(observation.block_kind.as_str())),
        (
            "source_projection_digest",
            digest_value(observation.projection_digest),
        ),
        (
            "intent",
            observe::intent_value(&observation.intent, observation.raw_destination_digest),
        ),
        ("resolution", resolution),
    ];
    if let Some(destination) = &observation.external_destination {
        members.push(("external_destination", string(destination)));
    }
    object(members)
}

const fn reason_str(reason: Reason) -> &'static str {
    reason.as_str()
}

const fn outcome_str(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Exact => "exact",
        Outcome::Candidate => "candidate",
        Outcome::Ambiguous => "ambiguous",
        Outcome::None => "none",
    }
}

const fn source_change_str(change: SourceChange) -> &'static str {
    match change {
        SourceChange::Equal => "equal",
        SourceChange::Changed => "changed",
        SourceChange::Unknown => "unknown",
        SourceChange::Added => "added",
        SourceChange::Removed => "removed",
    }
}

const fn target_change_str(change: TargetChange) -> &'static str {
    match change {
        TargetChange::Equal => "equal",
        TargetChange::Changed => "changed",
        TargetChange::NewlyResolved => "newly-resolved",
        TargetChange::BecameMissing => "became-missing",
        TargetChange::NotComparable => "not-comparable",
    }
}

const fn impact_str(impact: Impact) -> &'static str {
    match impact {
        Impact::None => "none",
        Impact::SubjectChanged => "subject-changed",
        Impact::DependencyChangedSubjectUnchanged => "dependency-changed-subject-unchanged",
        Impact::DependencyAndSubjectCochanged => "dependency-and-subject-cochanged",
        Impact::ReferenceResolved => "reference-resolved",
        Impact::NotApplicable => "not-applicable",
        Impact::ObservationCorrelationAmbiguous => "observation-correlation-ambiguous",
        Impact::NewObservation => "new-observation",
        Impact::RemovedObservation => "removed-observation",
    }
}

fn comparison_value(comparison: &Comparison) -> Value {
    let side = |observation: &Option<Observation>| {
        observation.as_ref().map_or(Value::Null, occurrence_value)
    };
    let list =
        |members: &[Observation]| Value::array(members.iter().map(occurrence_value).collect());
    object(vec![
        ("base", side(&comparison.base)),
        ("candidate", side(&comparison.candidate)),
        ("correlation", string(outcome_str(comparison.outcome))),
        ("correlation_reason", string(reason_str(comparison.reason))),
        (
            "alternatives",
            object(vec![
                ("base", list(&comparison.alternatives_base)),
                ("candidate", list(&comparison.alternatives_candidate)),
            ]),
        ),
        (
            "source_change",
            string(source_change_str(comparison.source_change)),
        ),
        (
            "target_change",
            string(target_change_str(comparison.target_change)),
        ),
        ("impact", string(impact_str(comparison.impact))),
    ])
}

fn side_facets(
    record: &DocumentRecord,
) -> (
    &'static str,
    Option<&'static str>,
    ContentAvailability,
    Option<Adapter>,
) {
    match &record.status {
        DocumentStatus::Scanned(_) => (
            "scanned",
            None,
            ContentAvailability::Available,
            record.adapter,
        ),
        DocumentStatus::ExcludedBuiltIn => (
            "excluded-built-in",
            None,
            ContentAvailability::NotRead,
            None,
        ),
        DocumentStatus::Unsupported(UnsupportedKind::LfsPointer) => (
            "unsupported",
            Some("lfs-pointer"),
            ContentAvailability::LfsPointerOnly,
            None,
        ),
        DocumentStatus::Unsupported(UnsupportedKind::Symlink) => (
            "unsupported",
            Some("symlink-document"),
            ContentAvailability::NotRead,
            None,
        ),
        DocumentStatus::Unsupported(UnsupportedKind::Gitlink) => (
            "unsupported",
            Some("gitlink-document"),
            ContentAvailability::NotRead,
            None,
        ),
        DocumentStatus::Unsupported(UnsupportedKind::Format) => (
            "unsupported",
            Some("unsupported-document-format"),
            ContentAvailability::Available,
            None,
        ),
        DocumentStatus::Failed(_) => ("scanned", None, ContentAvailability::NotRead, None),
    }
}

fn document_side_value(record: Option<&DocumentRecord>) -> Value {
    let Some(record) = record else {
        return Value::Null;
    };
    let entry_kind = match record.mode {
        amiss_wire::controls::GitMode::Symlink => "symlink",
        amiss_wire::controls::GitMode::Gitlink => "gitlink",
        amiss_wire::controls::GitMode::RegularFile
        | amiss_wire::controls::GitMode::ExecutableFile
        | amiss_wire::controls::GitMode::Tree => "blob",
    };
    let (status, reason, availability, adapter) = side_facets(record);
    let scanned = match &record.status {
        DocumentStatus::Scanned(value) => Some(value),
        DocumentStatus::ExcludedBuiltIn
        | DocumentStatus::Unsupported(_)
        | DocumentStatus::Failed(_) => None,
    };
    let opaque = scanned.map(|value| &value.opaque);
    let count =
        |value: Option<usize>| integer(u64::try_from(value.unwrap_or(0)).unwrap_or(u64::MAX));
    let byte_sum = |spans: Option<&Vec<(usize, usize)>>| {
        integer(spans.map_or(0, |list| {
            list.iter()
                .map(|(start, end)| u64::try_from(end.saturating_sub(*start)).unwrap_or(u64::MAX))
                .sum::<u64>()
        }))
    };
    object(vec![
        ("entry_kind", string(entry_kind)),
        ("entry_oid", string(record.oid.as_str())),
        ("git_mode", string(record.mode.as_str())),
        (
            "raw_digest",
            record.raw_digest.map_or(Value::Null, digest_value),
        ),
        ("status", string(status)),
        ("unsupported_reason", nullable(reason)),
        ("content_availability", string(availability.as_str())),
        (
            "adapter_id",
            adapter.map_or(Value::Null, |value: Adapter| string(value.adapter_id())),
        ),
        ("byte_count", integer(record.byte_count)),
        (
            "frontmatter_regions",
            integer(
                opaque
                    .is_some_and(|value| value.frontmatter_bytes > 0)
                    .into(),
            ),
        ),
        (
            "frontmatter_bytes",
            count(opaque.map(|value| value.frontmatter_bytes)),
        ),
        (
            "opaque_mdx_regions",
            count(opaque.map(|value| value.mdx.len())),
        ),
        ("opaque_mdx_bytes", byte_sum(opaque.map(|value| &value.mdx))),
        (
            "opaque_html_regions",
            count(opaque.map(|value| value.html.len())),
        ),
        (
            "opaque_html_bytes",
            byte_sum(opaque.map(|value| &value.html)),
        ),
        (
            "extracted_references",
            integer(scanned.map_or(0, |value| {
                u64::try_from(value.occurrences.len()).unwrap_or(u64::MAX)
            })),
        ),
    ])
}

struct PairedDocument<'a> {
    path: RepoPath,
    classification: &'static str,
    base: Option<&'a DocumentRecord>,
    candidate: Option<&'a DocumentRecord>,
}

fn paired_documents<'a>(
    base: &'a SnapshotDiscovery,
    candidate: &'a SnapshotDiscovery,
) -> Vec<PairedDocument<'a>> {
    let mut paired = Vec::with_capacity(
        base.documents
            .len()
            .saturating_add(candidate.documents.len()),
    );
    let mut base_at = 0;
    let mut candidate_at = 0;
    while let (Some(base_record), Some(candidate_record)) = (
        base.documents.get(base_at),
        candidate.documents.get(candidate_at),
    ) {
        match base_record.path.cmp(&candidate_record.path) {
            std::cmp::Ordering::Less => {
                paired.push(paired_document(base_record, Some(base_record), None));
                base_at = base_at.saturating_add(1);
            }
            std::cmp::Ordering::Equal => {
                paired.push(paired_document(
                    candidate_record,
                    Some(base_record),
                    Some(candidate_record),
                ));
                base_at = base_at.saturating_add(1);
                candidate_at = candidate_at.saturating_add(1);
            }
            std::cmp::Ordering::Greater => {
                paired.push(paired_document(
                    candidate_record,
                    None,
                    Some(candidate_record),
                ));
                candidate_at = candidate_at.saturating_add(1);
            }
        }
    }
    if let Some(remaining) = base.documents.get(base_at..) {
        paired.extend(
            remaining
                .iter()
                .map(|record| paired_document(record, Some(record), None)),
        );
    }
    if let Some(remaining) = candidate.documents.get(candidate_at..) {
        paired.extend(
            remaining
                .iter()
                .map(|record| paired_document(record, None, Some(record))),
        );
    }
    paired
}

fn paired_document<'a>(
    record: &DocumentRecord,
    base: Option<&'a DocumentRecord>,
    candidate: Option<&'a DocumentRecord>,
) -> PairedDocument<'a> {
    PairedDocument {
        path: record.path.clone(),
        classification: record.classification.as_str(),
        base,
        candidate,
    }
}

fn document_result_value(paired: &PairedDocument<'_>) -> Value {
    let base = document_side_value(paired.base);
    let candidate = document_side_value(paired.candidate);
    let change = match (&base, &candidate) {
        (Value::Null, Value::Null) => "unchanged",
        (Value::Null, _present) => "added",
        (_present, Value::Null) => "removed",
        (left, right) if left == right => "unchanged",
        _ => "changed",
    };
    object(vec![
        ("path", paired.path.to_value()),
        ("classification", string(paired.classification)),
        ("base", base),
        ("candidate", candidate),
        ("change", string(change)),
    ])
}

fn document_input(paired: &PairedDocument<'_>) -> DocumentInput {
    let side = |record: Option<&DocumentRecord>| {
        record.map(|record| match &record.status {
            DocumentStatus::Scanned(scanned) => DocumentSide::Scanned {
                mdx_regions: u64::try_from(scanned.opaque.mdx.len()).unwrap_or(u64::MAX),
                html_regions: u64::try_from(scanned.opaque.html.len()).unwrap_or(u64::MAX),
                extracted_references: u64::try_from(scanned.occurrences.len()).unwrap_or(u64::MAX),
            },
            DocumentStatus::Unsupported(_) | DocumentStatus::Failed(_) => DocumentSide::Unsupported,
            DocumentStatus::ExcludedBuiltIn => DocumentSide::ExcludedBuiltIn,
        })
    };
    DocumentInput {
        path: paired.path.clone(),
        base: side(paired.base),
        candidate: side(paired.candidate),
    }
}

fn finding_value(
    finding: &Finding,
    comparison_runs: [&[(Option<Digest>, Value)]; 2],
    document_rows: &[(RepoPath, Value)],
) -> Value {
    let kind = finding.kind();
    let scope = kind.scope();
    let coverage = match scope {
        FindingScope::Control => "control-plane",
        FindingScope::Reference | FindingScope::Observation | FindingScope::Document => "none",
    };
    let candidate_fact = finding
        .candidate_fact()
        .cloned()
        .or_else(|| nonreference_fact(finding, comparison_runs, document_rows));
    let fact_pair = |fact: Option<&FindingFact>| {
        (
            fact.map_or(Value::Null, |fact| digest_value(fact.digest())),
            fact.map_or(Value::Null, |fact| fact.value().clone()),
        )
    };
    let (base_digest, base_fact) = fact_pair(finding.base_fact());
    let (candidate_digest, candidate_fact_value) = fact_pair(candidate_fact.as_ref());
    let trace = trace_value(finding);
    let location_span = location_span_value(finding);
    object(vec![
        ("key_input", finding.key().to_value()),
        ("finding_key", digest_value(finding.key().digest())),
        ("kind", string(kind.as_str())),
        ("description", string(kind.meaning())),
        ("fix", finding.fix().map_or(Value::Null, fix_value)),
        ("coverage_requirement", string(coverage)),
        ("evidence_class", string(kind.evidence_class())),
        ("invariant_class", string(kind.invariant_class())),
        ("attribution", string(finding.attribution.as_str())),
        ("base_fact_digest", base_digest),
        ("base_fact", base_fact),
        ("candidate_fact_digest", candidate_digest),
        ("candidate_fact", candidate_fact_value),
        (
            "aggregation",
            object(vec![
                ("strategy", string("one-per-finding-key")),
                ("member_count", integer(finding.member_count)),
                ("locations_omitted", integer(0)),
                (
                    "representative_rule",
                    string("lowest-location-then-observation-id"),
                ),
            ]),
        ),
        (
            "location",
            object(vec![
                (
                    "side",
                    string(match finding.location.side {
                        LocationSide::Base => "base",
                        LocationSide::Candidate => "candidate",
                        LocationSide::Control => "control",
                        LocationSide::Global => "global",
                    }),
                ),
                ("path", nullable_path(finding.location.path.as_ref())),
                ("span", location_span),
            ]),
        ),
        (
            "observation_ids",
            Value::array(
                finding
                    .observation_ids
                    .iter()
                    .map(|id| digest_value(*id))
                    .collect(),
            ),
        ),
        (
            "configured_disposition",
            string(finding.configured_disposition.as_str()),
        ),
        (
            "effective_disposition",
            string(finding.effective_disposition.as_str()),
        ),
        ("policy_trace", Value::array(trace)),
        (
            "debt",
            finding
                .debt
                .as_ref()
                .map_or(Value::Null, debt_application_value),
        ),
        (
            "waiver",
            finding
                .waiver
                .as_ref()
                .map_or(Value::Null, waiver_application_value),
        ),
    ])
}

fn feedback_value(projected: feedback::Feedback) -> Value {
    let items = projected
        .items
        .into_iter()
        .map(|item| {
            let annotation = item.annotation.map_or(Value::Null, |annotation| {
                object(vec![
                    ("path", string(&annotation.path)),
                    (
                        "span",
                        source_span_value(annotation.span, annotation.display),
                    ),
                ])
            });
            object(vec![
                ("action", string(item.action.as_str())),
                ("target", nullable_path(item.target.as_ref())),
                (
                    "finding_kinds",
                    Value::array(
                        item.finding_kinds
                            .into_iter()
                            .map(|kind| string(kind.as_str()))
                            .collect(),
                    ),
                ),
                ("location_count", integer(item.location_count)),
                (
                    "effective_disposition",
                    string(item.effective_disposition.as_str()),
                ),
                ("annotation", annotation),
            ])
        })
        .collect();
    object(vec![
        ("status", string("available")),
        ("items", Value::array(items)),
        ("existing_count", integer(projected.existing_count)),
    ])
}

fn unavailable_feedback_value() -> Value {
    object(vec![("status", string("unavailable"))])
}

fn run_feedback_value(complete: bool, findings: &[Finding], comparisons: &[Comparison]) -> Value {
    if complete {
        feedback_value(feedback::project(findings, comparisons))
    } else {
        unavailable_feedback_value()
    }
}

fn tree_identity_value(tree: &amiss_wire::model::TreeIdentity) -> Value {
    object(vec![
        ("object_format", string(tree.object_format().as_str())),
        ("tree_oid", string(tree.tree_oid())),
    ])
}

fn debt_application_value(applied: &crate::evaluate::DebtApplied) -> Value {
    object(vec![
        ("debt_id", string(applied.item.debt_id.as_str())),
        (
            "debt_snapshot_digest",
            digest_value(applied.snapshot_digest),
        ),
        ("adoption_tree", tree_identity_value(&applied.adoption_tree)),
        (
            "accepted_fact_digest",
            digest_value(applied.item.accepted_fact_digest),
        ),
        ("owner", string(applied.item.owner.as_str())),
        ("reason", string(&applied.item.reason)),
        ("created_at", string(applied.item.created_at.as_str())),
        ("expires_at", string(applied.item.expires_at.as_str())),
    ])
}

fn waiver_application_value(applied: &crate::evaluate::WaiverApplied) -> Value {
    object(vec![
        ("waiver_id", string(applied.item.waiver_id.as_str())),
        ("waiver_bundle_digest", digest_value(applied.bundle_digest)),
        (
            "candidate_tree",
            tree_identity_value(&applied.item.candidate_tree),
        ),
        (
            "authorized_fact_digest",
            digest_value(applied.item.authorized_fact_digest),
        ),
        ("owner", string(applied.item.owner.as_str())),
        ("issuer", string(applied.item.issuer.as_str())),
        ("reason", string(&applied.item.reason)),
        ("created_at", string(applied.item.created_at.as_str())),
        ("not_before", string(applied.item.not_before.as_str())),
        ("expires_at", string(applied.item.expires_at.as_str())),
        ("residual_disposition", string("warn")),
    ])
}

/// The policy trace renders the finding's exact step chain.
fn trace_value(finding: &Finding) -> Vec<Value> {
    finding
        .steps
        .iter()
        .map(|step| {
            object(vec![
                ("source", string(step.source)),
                ("rule_id", string(&step.rule_id)),
                ("before", string(step.before.as_str())),
                ("after", string(step.after.as_str())),
            ])
        })
        .collect()
}

fn location_span_value(finding: &Finding) -> Value {
    finding.location.span.map_or(Value::Null, |span| {
        let display = finding.location.display.unwrap_or(SpanDisplay {
            start_line: 1,
            start_column: 1,
            end_line: 1,
            end_column: 1,
        });
        source_span_value(span, display)
    })
}

fn fix_value(fix: &FindingFix) -> Value {
    object(vec![
        ("path", fix.path.to_value()),
        (
            "span",
            object(vec![
                (
                    "start_byte",
                    integer(u64::try_from(fix.span.0).unwrap_or(u64::MAX)),
                ),
                (
                    "end_byte",
                    integer(u64::try_from(fix.span.1).unwrap_or(u64::MAX)),
                ),
            ]),
        ),
        ("replacement", string(&fix.replacement)),
        ("description", string(fix.kind.meaning())),
    ])
}

/// A nonreference finding carries exactly one candidate fact embedding the
/// full constructed comparison or document row it was derived from.
fn nonreference_fact(
    finding: &Finding,
    comparison_runs: [&[(Option<Digest>, Value)]; 2],
    document_rows: &[(RepoPath, Value)],
) -> Option<FindingFact> {
    let evidence = match finding.kind().scope() {
        FindingScope::Reference | FindingScope::Control => return None,
        FindingScope::Observation => {
            let id = finding.observation_ids.first()?;
            let row = comparison_runs.into_iter().find_map(|rows| {
                rows.binary_search_by_key(&Some(*id), |(primary, _)| *primary)
                    .ok()
                    .and_then(|index| rows.get(index))
                    .map(|(_, value)| value.clone())
            })?;
            object(vec![("kind", string("observation")), ("comparison", row)])
        }
        FindingScope::Document => {
            let path = finding.location.path.as_ref()?;
            let row = document_rows
                .binary_search_by(|(document, _)| document.cmp(path))
                .ok()
                .and_then(|index| document_rows.get(index))
                .map(|(_, value)| value.clone())?;
            object(vec![("kind", string("document")), ("document_result", row)])
        }
    };
    Some(FindingFact::new(finding.key(), evidence))
}

fn snapshot_value(snapshot: &SnapshotIdentity) -> Value {
    object(vec![
        ("kind", string("git-commit")),
        ("object_format", string(snapshot.object_format)),
        ("commit_oid", string(&snapshot.commit_oid)),
        ("tree_oid", string(&snapshot.tree_oid)),
    ])
}

fn candidate_value(candidate: &CandidateBlock, snapshot_request: Option<Digest>) -> Value {
    match candidate {
        CandidateBlock::Commit(identity) => snapshot_value(identity),
        CandidateBlock::Index(index) => object(vec![
            ("kind", string("index")),
            ("snapshot_schema", string(SNAPSHOT_SCHEMA)),
            ("identity_scope", string("complete-logical-index")),
            ("base_object_format", string(index.base_object_format)),
            ("base_commit_oid", string(&index.base_commit_oid)),
            (
                "index_projection_digest",
                digest_value(index.projection_digest),
            ),
            ("entry_count", integer(index.entry_count)),
            ("snapshot_digest", digest_value(index.snapshot_digest)),
        ]),
        CandidateBlock::Unavailable(reasons) => object(vec![
            ("kind", string("unavailable")),
            (
                "request_digest",
                snapshot_request.map_or(Value::Null, digest_value),
            ),
            (
                "reasons",
                Value::array(reasons.iter().map(|reason| string(reason)).collect()),
            ),
        ]),
    }
}

/// The evaluation's identity rows: everything of the resolved evaluation
/// value that precedes time, in the candidate-identity preimage order.
fn identity_rows(setup: &Setup) -> Vec<(&'static str, Value)> {
    let (mode, event_kind, finality, materialization) = match &setup.candidate {
        CandidateBlock::Commit(_) => (
            "commit-pair",
            "explicit-commit-pair",
            "explicit-replay",
            "git-objects",
        ),
        CandidateBlock::Index(_) | CandidateBlock::Unavailable(_) => {
            ("index", "local-index", "local-nonfinal", "index")
        }
    };
    let skip = match &setup.candidate {
        CandidateBlock::Index(index) => index.skip_worktree_paths,
        CandidateBlock::Commit(_) | CandidateBlock::Unavailable(_) => 0,
    };
    vec![
        ("mode", string(mode)),
        ("event_kind", string(event_kind)),
        ("finality", string(finality)),
        (
            "repository",
            setup.repository.as_ref().map_or(Value::Null, |identity| {
                object(vec![
                    ("host", string(identity.host())),
                    ("owner", string(identity.owner())),
                    ("name", string(identity.name())),
                ])
            }),
        ),
        ("candidate_ref", nullable(setup.candidate_ref.as_deref())),
        ("target_ref", nullable(setup.target_ref.as_deref())),
        (
            "default_branch_ref",
            nullable(setup.default_branch_ref.as_deref()),
        ),
        ("base", snapshot_value(&setup.base)),
        (
            "candidate",
            candidate_value(&setup.candidate, setup.requests.snapshot),
        ),
        ("materialization", string(materialization)),
        ("skip_worktree_paths", integer(skip)),
        ("index_only_materialized_paths", integer(0)),
    ]
}

/// The rolling candidate identity. The selected forge is resolution-significant,
/// so it is bound alongside the repository and snapshots.
fn candidate_identity_value(setup: &Setup) -> Value {
    identity_value(
        setup,
        vec![("schema", string(CANDIDATE_IDENTITY_DOMAIN))],
        Vec::new(),
    )
}

/// The candidate-identity digest a trusted-time statement must carry: `HJ`
/// over the resolved-evaluation identity, including its forge.
#[must_use]
pub fn candidate_identity_digest(setup: &Setup) -> Digest {
    hj(CANDIDATE_IDENTITY_DOMAIN, &candidate_identity_value(setup))
}

fn evaluation_value(setup: &Setup) -> Value {
    identity_value(
        setup,
        Vec::new(),
        vec![
            (
                "evaluation_instant",
                setup.policy.time.as_ref().map_or(Value::Null, |time| {
                    string(time.statement.evaluation_instant().as_str())
                }),
            ),
            ("trusted_time", Value::Bool(setup.policy.time.is_some())),
        ],
    )
}

fn identity_value(
    setup: &Setup,
    mut rows: Vec<(&'static str, Value)>,
    before_forge: Vec<(&'static str, Value)>,
) -> Value {
    rows.extend(identity_rows(setup));
    rows.extend(before_forge);
    rows.push((
        "forge",
        setup
            .forge
            .map_or(Value::Null, |dialect| string(dialect.as_str())),
    ));
    object(rows)
}

fn verified_provenance(control: Option<(Digest, &'static str)>) -> Value {
    control.map_or_else(
        || {
            object(vec![
                ("status", string("none")),
                ("digest", Value::Null),
                ("trust_source", string("none")),
            ])
        },
        |(digest, trust)| {
            object(vec![
                ("status", string("verified")),
                ("digest", digest_value(digest)),
                ("trust_source", string(trust)),
            ])
        },
    )
}

fn controls_value(setup: &Setup) -> Value {
    if let Some(reason) = setup.controls_unavailable {
        return object(vec![
            ("status", string("unavailable")),
            (
                "request_digest",
                setup.requests.controls.map_or(Value::Null, digest_value),
            ),
            ("reasons", Value::array(vec![string(reason)])),
        ]);
    }
    let (descriptor, descriptor_digest) = sandbox_descriptor();
    object(vec![
        ("profile", string(setup.profile.as_str())),
        (
            "base_repository_policy_digest",
            setup.policy.base_digest.map_or(Value::Null, digest_value),
        ),
        (
            "candidate_repository_policy_digest",
            setup
                .policy
                .candidate_digest
                .map_or(Value::Null, digest_value),
        ),
        (
            "organization_floor",
            verified_provenance(setup.policy.floor),
        ),
        (
            "debt_snapshot",
            verified_provenance(
                setup
                    .policy
                    .debt
                    .as_ref()
                    .map(|debt| (debt.digest, debt.trust_source)),
            ),
        ),
        (
            "waiver_bundle",
            verified_provenance(
                setup
                    .policy
                    .waiver
                    .as_ref()
                    .map(|waiver| (waiver.digest, waiver.trust_source)),
            ),
        ),
        (
            "execution_constraint",
            setup.policy.constraint.as_ref().map_or_else(
                || object(vec![("status", string("none"))]),
                |(descriptor, trust)| {
                    object(vec![
                        ("status", string("verified")),
                        ("descriptor", constraint_descriptor_value(descriptor)),
                        ("descriptor_digest", digest_value(descriptor.digest())),
                        ("trust_source", string(trust)),
                    ])
                },
            ),
        ),
        (
            "sandbox",
            object(vec![
                ("assurance", string("self-asserted")),
                ("enforcement_source", string("local-process")),
                ("descriptor", descriptor),
                ("descriptor_digest", digest_value(descriptor_digest)),
                ("verification", Value::Null),
            ]),
        ),
        (
            "trusted_time_source",
            setup.policy.time.as_ref().map_or_else(
                || object(vec![("status", string("none"))]),
                |time| {
                    object(vec![
                        ("status", string("verified")),
                        ("statement", time_statement_value(&time.statement)),
                        ("statement_digest", digest_value(time.digest)),
                        ("trust_source", string("external-required-check")),
                    ])
                },
            ),
        ),
    ])
}

fn constraint_descriptor_value(
    descriptor: &amiss_wire::controls::ExecutionConstraintDescriptor,
) -> Value {
    object(vec![
        ("schema", string("amiss/scanner-execution-constraint")),
        (
            "action_repository",
            object(vec![
                ("host", string(descriptor.action_repository().host())),
                ("owner", string(descriptor.action_repository().owner())),
                ("name", string(descriptor.action_repository().name())),
            ]),
        ),
        (
            "action_object_format",
            string(descriptor.action_object_format().as_str()),
        ),
        (
            "action_commit_oid",
            string(descriptor.action_commit_oid().as_str()),
        ),
        (
            "action_tree_oid",
            string(descriptor.action_tree_oid().as_str()),
        ),
        ("manifest_path", string(descriptor.manifest_path().as_str())),
        (
            "release_manifest_digest",
            digest_value(descriptor.release_manifest_digest()),
        ),
        (
            "selected_platform",
            string(descriptor.selected_platform().as_str()),
        ),
        (
            "required_status_name",
            string(descriptor.required_status_name()),
        ),
        ("bootstrap_contract", string("amiss-action-bootstrap")),
        (
            "bootstrap_digest",
            digest_value(descriptor.bootstrap_digest()),
        ),
    ])
}

fn time_statement_value(statement: &amiss_wire::controls::TrustedTimeStatement) -> Value {
    let mut rows = vec![
        ("schema", string(statement.schema())),
        ("controller", string(statement.controller())),
        (
            "repository",
            object(vec![
                ("host", string(statement.repository().host())),
                ("owner", string(statement.repository().owner())),
                ("name", string(statement.repository().name())),
            ]),
        ),
        ("ref", string(statement.ref_name().as_str())),
        (
            "candidate_identity_digest",
            digest_value(statement.candidate_identity_digest()),
        ),
    ];
    rows.push(("provider", string(statement.provider())));
    rows.extend([
        ("provider_run_id", string(statement.provider_run_id())),
        (
            "provider_run_attempt",
            integer(statement.provider_run_attempt()),
        ),
        (
            "evaluation_instant",
            string(statement.evaluation_instant().as_str()),
        ),
        ("valid_until", string(statement.valid_until().as_str())),
    ]);
    object(rows)
}

struct Counts {
    documents: Value,
    references: Value,
    findings: Value,
}

#[derive(Default)]
struct DocumentCountSet {
    discovered: u64,
    scanned: u64,
    unsupported: u64,
    excluded_builtin: u64,
    frontmatter_documents: u64,
    frontmatter_bytes: u64,
    opaque_mdx_documents: u64,
    opaque_mdx_regions: u64,
    opaque_mdx_bytes: u64,
    opaque_html_documents: u64,
    opaque_html_regions: u64,
    opaque_html_bytes: u64,
}

fn region_bytes(spans: &[(usize, usize)]) -> u64 {
    spans.iter().fold(0, |total, (start, end)| {
        total.saturating_add(u64::try_from(end.saturating_sub(*start)).unwrap_or(u64::MAX))
    })
}

fn document_counts<'a>(
    candidate_records: impl IntoIterator<Item = &'a DocumentRecord>,
    unlinked: u64,
) -> Value {
    let mut counts = DocumentCountSet::default();
    for record in candidate_records {
        counts.discovered = counts.discovered.saturating_add(1);
        match &record.status {
            DocumentStatus::Scanned(scanned) => {
                counts.scanned = counts.scanned.saturating_add(1);
                let opaque = &scanned.opaque;
                counts.frontmatter_documents = counts
                    .frontmatter_documents
                    .saturating_add(u64::from(opaque.frontmatter_bytes > 0));
                counts.frontmatter_bytes = counts
                    .frontmatter_bytes
                    .saturating_add(u64::try_from(opaque.frontmatter_bytes).unwrap_or(u64::MAX));
                counts.opaque_mdx_documents = counts
                    .opaque_mdx_documents
                    .saturating_add(u64::from(!opaque.mdx.is_empty()));
                counts.opaque_mdx_regions = counts
                    .opaque_mdx_regions
                    .saturating_add(u64::try_from(opaque.mdx.len()).unwrap_or(u64::MAX));
                counts.opaque_mdx_bytes = counts
                    .opaque_mdx_bytes
                    .saturating_add(region_bytes(&opaque.mdx));
                counts.opaque_html_documents = counts
                    .opaque_html_documents
                    .saturating_add(u64::from(!opaque.html.is_empty()));
                counts.opaque_html_regions = counts
                    .opaque_html_regions
                    .saturating_add(u64::try_from(opaque.html.len()).unwrap_or(u64::MAX));
                counts.opaque_html_bytes = counts
                    .opaque_html_bytes
                    .saturating_add(region_bytes(&opaque.html));
            }
            DocumentStatus::Unsupported(_) => {
                counts.unsupported = counts.unsupported.saturating_add(1);
            }
            DocumentStatus::ExcludedBuiltIn => {
                counts.excluded_builtin = counts.excluded_builtin.saturating_add(1);
            }
            DocumentStatus::Failed(_) => {}
        }
    }
    object(vec![
        ("discovered", integer(counts.discovered)),
        ("outside_document_set", integer(0)),
        ("scanned", integer(counts.scanned)),
        ("unsupported", integer(counts.unsupported)),
        ("excluded_builtin", integer(counts.excluded_builtin)),
        ("unlinked", integer(unlinked)),
        (
            "frontmatter_documents",
            integer(counts.frontmatter_documents),
        ),
        ("opaque_mdx_documents", integer(counts.opaque_mdx_documents)),
        (
            "opaque_html_documents",
            integer(counts.opaque_html_documents),
        ),
        ("opaque_mdx_regions", integer(counts.opaque_mdx_regions)),
        ("opaque_mdx_bytes", integer(counts.opaque_mdx_bytes)),
        ("opaque_html_regions", integer(counts.opaque_html_regions)),
        ("opaque_html_bytes", integer(counts.opaque_html_bytes)),
        ("frontmatter_regions", integer(counts.frontmatter_documents)),
        ("frontmatter_bytes", integer(counts.frontmatter_bytes)),
    ])
}

#[derive(Default)]
struct ReferenceCountSet {
    extracted: u64,
    explicit_local: u64,
    same_repository: u64,
    external_out_of_scope: u64,
    unsupported: u64,
    resolved: u64,
    missing: u64,
}

fn reference_counts(comparisons: &[Comparison]) -> Value {
    let mut counts = ReferenceCountSet::default();
    for observation in comparisons.iter().flat_map(|comparison| {
        comparison
            .candidate
            .iter()
            .chain(comparison.alternatives_candidate.iter())
    }) {
        counts.extracted = counts.extracted.saturating_add(1);
        match observation.intent.kind {
            IntentKind::RepositoryPath => {
                counts.explicit_local = counts.explicit_local.saturating_add(1);
            }
            IntentKind::SameRepositoryGithub
            | IntentKind::SameRepositoryGitlab
            | IntentKind::SameRepositoryGitea => {
                counts.same_repository = counts.same_repository.saturating_add(1);
            }
            IntentKind::ExternalUrl => {
                counts.external_out_of_scope = counts.external_out_of_scope.saturating_add(1);
            }
            IntentKind::SiteRoute | IntentKind::Unsupported => {
                counts.unsupported = counts.unsupported.saturating_add(1);
            }
            IntentKind::Label => {}
        }
        match &observation.resolution {
            Resolution::Resolved(_) => {
                counts.resolved = counts.resolved.saturating_add(1);
            }
            Resolution::Missing(_) => {
                counts.missing = counts.missing.saturating_add(1);
            }
            Resolution::TypeMismatch(_)
            | Resolution::DeclaredUntracked(_)
            | Resolution::UnsupportedTarget(_)
            | Resolution::UnsupportedSemantics(_)
            | Resolution::UnsupportedVersion(_)
            | Resolution::Invalid(_)
            | Resolution::External(_) => {}
        }
    }
    object(vec![
        ("extracted", integer(counts.extracted)),
        ("explicit_local", integer(counts.explicit_local)),
        ("same_repository", integer(counts.same_repository)),
        (
            "external_out_of_scope",
            integer(counts.external_out_of_scope),
        ),
        ("unsupported", integer(counts.unsupported)),
        ("resolved", integer(counts.resolved)),
        ("missing", integer(counts.missing)),
    ])
}

#[derive(Default)]
struct FindingCountSet {
    record: u64,
    warn: u64,
    fail: u64,
    introduced: u64,
    pre_existing: u64,
    resolved: u64,
    unknown: u64,
    not_applicable: u64,
    debt_tolerated: u64,
    waived: u64,
    unsupported_capabilities: u64,
    unlinked_documents: u64,
}

fn summary_counts(
    paired: &[PairedDocument<'_>],
    comparisons: &[Comparison],
    findings: &[Finding],
    finding_rows_count: u64,
) -> Counts {
    let mut counts = FindingCountSet::default();
    for finding in findings {
        match finding.effective_disposition {
            Disposition::Record => counts.record = counts.record.saturating_add(1),
            Disposition::Warn => counts.warn = counts.warn.saturating_add(1),
            Disposition::Fail => counts.fail = counts.fail.saturating_add(1),
        }
        match finding.attribution {
            Attribution::Introduced => {
                counts.introduced = counts.introduced.saturating_add(1);
            }
            Attribution::PreExisting => {
                counts.pre_existing = counts.pre_existing.saturating_add(1);
            }
            Attribution::Resolved => {
                counts.resolved = counts.resolved.saturating_add(1);
            }
            Attribution::Unknown => {
                counts.unknown = counts.unknown.saturating_add(1);
            }
            Attribution::NotApplicable => {
                counts.not_applicable = counts.not_applicable.saturating_add(1);
            }
        }
        counts.debt_tolerated = counts
            .debt_tolerated
            .saturating_add(u64::from(finding.debt.is_some()));
        counts.waived = counts
            .waived
            .saturating_add(u64::from(finding.waiver.is_some()));
        counts.unsupported_capabilities = counts.unsupported_capabilities.saturating_add(
            u64::from(finding.kind() == FindingKind::UnsupportedCapability),
        );
        counts.unlinked_documents = counts
            .unlinked_documents
            .saturating_add(u64::from(finding.kind() == FindingKind::UnlinkedDocument));
    }
    let documents = document_counts(
        paired.iter().filter_map(|pair| pair.candidate),
        counts.unlinked_documents,
    );
    let findings_value = object(vec![
        ("total", integer(finding_rows_count)),
        ("record", integer(counts.record)),
        ("warn", integer(counts.warn)),
        ("fail", integer(counts.fail)),
        ("introduced", integer(counts.introduced)),
        ("pre_existing", integer(counts.pre_existing)),
        ("resolved", integer(counts.resolved)),
        ("unknown", integer(counts.unknown)),
        ("not_applicable", integer(counts.not_applicable)),
        ("debt_tolerated", integer(counts.debt_tolerated)),
        ("waived", integer(counts.waived)),
        ("analysis_errors", integer(0)),
        (
            "unsupported_capabilities",
            integer(counts.unsupported_capabilities),
        ),
    ]);
    Counts {
        documents,
        references: reference_counts(comparisons),
        findings: findings_value,
    }
}

/// Constructs the complete report for a local commit-pair run with no
/// external controls: canonical payload, envelope, wire bytes, digest, and
/// the process result.
#[must_use]
pub fn construct(
    setup: &Setup,
    base: &SnapshotDiscovery,
    candidate: &SnapshotDiscovery,
    comparisons: Vec<Comparison>,
    claims: &[crate::claim::ClaimOutcome],
) -> Built {
    let paired = paired_documents(base, candidate);
    let (governed, findings, exception_errors) =
        evaluate_paired(setup, &paired, candidate, &comparisons, claims);

    if let Some(crossing) = findings_ceiling_crossing(setup, &findings) {
        let mut details = logical_error_set(&governed, &exception_errors);
        details.push(crossing);
        return construct_incomplete(setup, &details);
    }

    let document_rows: Vec<(RepoPath, Value)> = paired
        .iter()
        .map(|pair| (pair.path.clone(), document_result_value(pair)))
        .collect();
    let error_details = logical_error_set(&governed, &exception_errors);
    if error_details.len() > error_ceiling(setup) {
        return construct_incomplete(setup, &error_details);
    }
    let governed_errors: Vec<Value> = error_details.iter().map(error_row_value).collect();
    let (complete, status, exit_code) = run_result(&findings, &governed_errors);
    let feedback = run_feedback_value(complete, &findings, &comparisons);
    let finding_count = u64::try_from(findings.len()).unwrap_or(u64::MAX);
    let counts = summary_counts(&paired, &comparisons, &findings, finding_count);
    let (governed_claims, unattested_claims) = claim_counters(claims);
    let candidate_start = comparisons.partition_point(|comparison| comparison.candidate.is_none());
    let mut comparison_rows = Vec::with_capacity(comparisons.len());
    for comparison in comparisons {
        let primary = comparison
            .candidate
            .as_ref()
            .or(comparison.base.as_ref())
            .map(|observation| observation.id);
        comparison_rows.push((primary, comparison_value(&comparison)));
    }
    let (base_only_rows, candidate_rows) = comparison_rows.split_at(candidate_start);
    let comparison_runs = [base_only_rows, candidate_rows];
    let finding_rows: Vec<Value> = findings
        .iter()
        .map(|finding| finding_value(finding, comparison_runs, &document_rows))
        .collect();

    let payload = object(vec![
        ("schema", string(PAYLOAD_SCHEMA)),
        ("compatibility", string(COMPATIBILITY)),
        ("engine", engine_block(&setup.engine)),
        ("evaluation", evaluation_value(setup)),
        ("controls", controls_value(setup)),
        (
            "result",
            result_value(
                complete,
                status,
                exit_code,
                finding_count,
                u64::try_from(governed_errors.len()).unwrap_or(u64::MAX),
            ),
        ),
        ("feedback", feedback),
        (
            "summary",
            object(vec![
                ("counts_complete", Value::Bool(true)),
                ("documents", counts.documents),
                ("references", counts.references),
                ("findings", counts.findings),
                governed_claims,
                unattested_claims,
            ]),
        ),
        (
            "documents",
            Value::array(document_rows.into_iter().map(|(_path, row)| row).collect()),
        ),
        (
            "observations",
            Value::array(
                comparison_rows
                    .into_iter()
                    .map(|(_primary, row)| row)
                    .collect(),
            ),
        ),
        ("findings", Value::array(finding_rows)),
        ("errors", Value::array(governed_errors)),
    ]);
    let (payload_digest, payload_length) = hj_with_length(PAYLOAD_SCHEMA, &payload);
    let envelope = object(vec![
        ("schema", string(ENVELOPE_SCHEMA)),
        ("payload", payload),
        ("payload_digest", digest_value(payload_digest)),
    ]);
    let built = Built {
        envelope,
        payload_digest,
        status,
        exit_code,
    };
    output_gate(setup, error_details, payload_length, built)
}

fn result_value(
    complete: bool,
    status: &str,
    exit_code: i64,
    finding_count: u64,
    error_count: u64,
) -> Value {
    object(vec![
        ("complete", Value::Bool(complete)),
        ("status", string(status)),
        ("exit_code", Value::Integer(exit_code)),
        ("finding_count", integer(finding_count)),
        ("error_count", integer(error_count)),
    ])
}

/// The deduplicated logical error set in canonical key order.
fn logical_error_set(
    governed: &[crate::evaluate::GovernedSeed],
    exceptions: &[ErrorDetail],
) -> Vec<ErrorDetail> {
    let mut details = governed_details(governed);
    details.extend(exceptions.iter().cloned());
    details.sort();
    details.dedup();
    details
}

/// A non-error envelope whose wire would exceed the reservation becomes the
/// output-limit fatal projection carrying the exact counted length.
fn output_gate(
    setup: &Setup,
    details: Vec<ErrorDetail>,
    payload_length: u64,
    built: Built,
) -> Built {
    let envelope_shell = object(vec![
        ("schema", string(ENVELOPE_SCHEMA)),
        ("payload", Value::Null),
        ("payload_digest", digest_value(built.payload_digest)),
    ]);
    let wire_length = canonical_length(&envelope_shell)
        .saturating_sub(canonical_length(&Value::Null))
        .saturating_add(payload_length)
        .saturating_add(1);
    debug_assert_eq!(
        wire_length,
        canonical_length(&built.envelope).saturating_add(1)
    );
    if wire_length <= MACHINE_JSON_BYTES {
        return built;
    }
    let mut details = details;
    details.push(ErrorDetail {
        code: AnalysisErrorCode::OutputLimitExceeded,
        path: None,
        path_bytes: None,
        resource: Some((
            ResourceName::MachineJsonBytes,
            MACHINE_JSON_BYTES,
            wire_length,
        )),
    });
    construct_incomplete(setup, &details)
}

fn governed_details(governed: &[crate::evaluate::GovernedSeed]) -> Vec<ErrorDetail> {
    governed
        .iter()
        .map(|seed| ErrorDetail {
            code: AnalysisErrorCode::UnsupportedCapability,
            path: Some(seed.document.clone()),
            path_bytes: None,
            resource: None,
        })
        .collect()
}

/// The effective typed-analysis-errors-retained ceiling `E`, defended to the
/// schema range even if a caller-supplied value strays.
/// The evaluation step of construction: the paired documents projected to
/// evaluator inputs, the governed seeds, and the complete findings with their
/// exception errors.
fn evaluate_paired(
    setup: &Setup,
    paired: &[PairedDocument<'_>],
    candidate: &SnapshotDiscovery,
    comparisons: &[Comparison],
    claims: &[crate::claim::ClaimOutcome],
) -> (
    Vec<crate::evaluate::GovernedSeed>,
    Vec<Finding>,
    Vec<ErrorDetail>,
) {
    let inputs: Vec<DocumentInput> = paired.iter().map(document_input).collect();
    let governed = governed_seeds(candidate, claims);
    let groups = crate::evaluate::claim_groups(claims);
    let (findings, exception_errors) = crate::evaluate::evaluate_with_policy(
        &inputs,
        comparisons,
        setup.profile,
        &setup.policy,
        &governed,
        &groups,
    );
    (governed, findings, exception_errors)
}

/// The complete-findings ceiling, charged against the exact array the report
/// would ship, control rows included, after every exception has been applied.
/// Past it there is no report: a run that produced more findings than the
/// contract admits is incomplete, not truncated.
fn findings_ceiling_crossing(setup: &Setup, findings: &[Finding]) -> Option<ErrorDetail> {
    let finding_total = u64::try_from(findings.len()).unwrap_or(u64::MAX);
    (finding_total > setup.policy.complete_findings).then_some(ErrorDetail {
        code: AnalysisErrorCode::ResourceLimitExceeded,
        path: None,
        path_bytes: None,
        resource: Some((
            ResourceName::CompleteFindings,
            setup.policy.complete_findings,
            finding_total,
        )),
    })
}

fn error_ceiling(setup: &Setup) -> usize {
    usize::try_from(setup.policy.errors_retained.clamp(1, 64)).unwrap_or(64)
}

/// The logical error set law: full tuples deduplicated and sorted by the
/// canonical error key. Retains only the lowest `E` keys; on overflow the
/// first `E - 1` ordinary errors are followed by the `TOO_MANY_ERRORS`
/// sentinel carrying configured limit `E` and observed lower bound `E + 1`.
fn retained_details(details: &[ErrorDetail], ceiling: usize) -> Vec<ErrorDetail> {
    let mut sorted: Vec<ErrorDetail> = details.to_vec();
    sorted.sort();
    sorted.dedup();
    if sorted.len() > ceiling {
        sorted.truncate(ceiling.saturating_sub(1));
        let limit = u64::try_from(ceiling).unwrap_or(64);
        sorted.push(ErrorDetail {
            code: AnalysisErrorCode::TooManyErrors,
            path: None,
            path_bytes: None,
            resource: Some((
                ResourceName::TypedAnalysisErrorsRetained,
                limit,
                limit.saturating_add(1),
            )),
        });
    }
    sorted
}

/// A complete run passes or fails by its effective dispositions; a run with
/// reserved governed declarations is boundary-incomplete with full details
/// and exit class two.
fn run_result(findings: &[Finding], governed_errors: &[Value]) -> (bool, &'static str, i64) {
    if !governed_errors.is_empty() {
        return (false, "incomplete", 2);
    }
    let failing = findings
        .iter()
        .any(|finding| finding.effective_disposition == Disposition::Fail);
    if failing {
        (true, "fail", 1)
    } else {
        (true, "pass", 0)
    }
}

/// One seed per candidate document still holding unanswered reserved
/// definitions: unknown forms always, and recognized claims only when no
/// evaluated outcome answers for their exact span, so a caller that supplies
/// no outcomes keeps the full boundary and never a silent pass. Equal source
/// digests group with exact multiplicity and the least location represents.
fn governed_seeds(
    candidate: &SnapshotDiscovery,
    claims: &[crate::claim::ClaimOutcome],
) -> Vec<crate::evaluate::GovernedSeed> {
    let answered: std::collections::BTreeSet<(&RepoPath, (usize, usize))> = claims
        .iter()
        .map(|outcome| (&outcome.document, outcome.span))
        .collect();
    let mut seeds = Vec::new();
    for record in &candidate.documents {
        let DocumentStatus::Scanned(scanned) = &record.status else {
            continue;
        };
        let unanswered: Vec<&crate::scan::GovernedSource> = scanned
            .governed
            .iter()
            .filter(|governed| {
                matches!(governed.form, crate::claim::GovernedForm::Unknown)
                    || !answered.contains(&(&record.path, governed.span))
            })
            .collect();
        if unanswered.is_empty() {
            continue;
        }
        let sources = crate::evaluate::source_multiplicities(
            unanswered.iter().map(|governed| governed.digest),
        );
        let representative = unanswered.iter().min_by_key(|governed| governed.span);
        seeds.push(crate::evaluate::GovernedSeed {
            document: record.path.clone(),
            member_count: u64::try_from(unanswered.len()).unwrap_or(u64::MAX),
            sources,
            representative_span: representative.map(|governed| governed.span),
            representative_display: representative.map(|governed| governed.display),
        });
    }
    seeds
}

/// The two summary claim counters: evaluated claims, and the defective
/// subset that did not attest.
fn claim_counters(
    claims: &[crate::claim::ClaimOutcome],
) -> ((&'static str, Value), (&'static str, Value)) {
    let unattested = claims
        .iter()
        .filter(|outcome| outcome.verdict != crate::claim::ClaimVerdict::Attested)
        .count();
    (
        (
            "governed_claims",
            integer(u64::try_from(claims.len()).unwrap_or(u64::MAX)),
        ),
        (
            "unattested_claims",
            integer(u64::try_from(unattested).unwrap_or(u64::MAX)),
        ),
    )
}

fn zero_counts(analysis_errors: u64) -> Counts {
    Counts {
        documents: document_counts(std::iter::empty::<&DocumentRecord>(), 0),
        references: reference_counts(&[]),
        findings: object(vec![
            ("total", integer(0)),
            ("record", integer(0)),
            ("warn", integer(0)),
            ("fail", integer(0)),
            ("introduced", integer(0)),
            ("pre_existing", integer(0)),
            ("resolved", integer(0)),
            ("unknown", integer(0)),
            ("not_applicable", integer(0)),
            ("debt_tolerated", integer(0)),
            ("waived", integer(0)),
            ("analysis_errors", integer(analysis_errors)),
            ("unsupported_capabilities", integer(0)),
        ]),
    }
}

/// The fatal-incomplete report for a run whose evaluation identity resolved
/// but whose analysis raised typed errors: resolved evaluation and controls,
/// cleared detail arrays, zeroed inexact summary, every error row retained in
/// canonical order, and exit class two.
#[must_use]
pub fn construct_incomplete(setup: &Setup, details: &[ErrorDetail]) -> Built {
    let retained = retained_details(details, error_ceiling(setup));
    let error_rows: Vec<Value> = retained.iter().map(error_row_value).collect();
    let error_count = u64::try_from(error_rows.len()).unwrap_or(u64::MAX);
    let counts = zero_counts(error_count);

    let payload = object(vec![
        ("schema", string(PAYLOAD_SCHEMA)),
        ("compatibility", string(COMPATIBILITY)),
        ("engine", engine_block(&setup.engine)),
        ("evaluation", evaluation_value(setup)),
        ("controls", controls_value(setup)),
        (
            "result",
            object(vec![
                ("complete", Value::Bool(false)),
                ("status", string("incomplete")),
                ("exit_code", Value::Integer(2)),
                ("finding_count", integer(0)),
                ("error_count", integer(error_count)),
            ]),
        ),
        ("feedback", unavailable_feedback_value()),
        (
            "summary",
            object(vec![
                ("counts_complete", Value::Bool(false)),
                ("documents", counts.documents),
                ("references", counts.references),
                ("findings", counts.findings),
                ("governed_claims", integer(0)),
                ("unattested_claims", integer(0)),
            ]),
        ),
        ("documents", Value::array(Vec::new())),
        ("observations", Value::array(Vec::new())),
        ("findings", Value::array(Vec::new())),
        ("errors", Value::array(error_rows)),
    ]);
    let payload_digest = hj(PAYLOAD_SCHEMA, &payload);
    let envelope = object(vec![
        ("schema", string(ENVELOPE_SCHEMA)),
        ("payload", payload),
        ("payload_digest", digest_value(payload_digest)),
    ]);
    Built {
        envelope,
        payload_digest,
        status: "incomplete",
        exit_code: 2,
    }
}
