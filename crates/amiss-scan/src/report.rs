use amiss_wire::controls::{ContentAvailability, ResourceName};
use amiss_wire::digest::{Digest, hj};
use amiss_wire::json::{Value, canonical, canonical_length};
use amiss_wire::model::Adapter;
use amiss_wire::report::{
    AnalysisErrorCode, Disposition, EngineProvenance, ErrorDetail, FindingKind, FindingScope,
    IntentKind, MACHINE_JSON_BYTES, PAYLOAD_SCHEMA, ResolutionStatus, engine_block,
    error_row_value, sandbox_descriptor,
};

use crate::correlate::{Comparison, Observation, Outcome, Reason, SourceChange, TargetChange};
use crate::discovery::{DocumentRecord, DocumentStatus, SnapshotDiscovery, UnsupportedKind};
use crate::evaluate::{
    Attribution, DocumentInput, DocumentSide, FACT_SCHEMA, Finding, LocationSide,
};
use crate::{Impact, observe};

pub const ENVELOPE_SCHEMA: &str = "amiss/scanner-report-envelope/v1";
pub const INDEX_PROJECTION_SCHEMA: &str = "amiss/scanner-index-projection/v1";
pub const SNAPSHOT_SCHEMA: &str = "amiss/scanner-snapshot/v1";

/// The canonical logical-index projection and the synthetic snapshot input
/// built over it, with both digests.
#[must_use]
pub fn synthetic_candidate(
    base_object_format: &'static str,
    base_commit_oid: &str,
    entries: &[(String, amiss_wire::controls::GitMode, String, bool)],
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
                ("path", string(path)),
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
        ("entries", Value::Array(rows)),
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
    pub enforce: bool,
    pub repository: Option<(String, String)>,
    pub candidate_ref: Option<String>,
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
    Value::String(text.to_owned())
}

fn nullable(text: Option<&str>) -> Value {
    text.map_or(Value::Null, string)
}

fn integer(value: u64) -> Value {
    Value::Integer(i64::try_from(value).unwrap_or(i64::MAX))
}

fn digest_value(digest: Digest) -> Value {
    Value::String(digest.to_string())
}

fn object(members: Vec<(&str, Value)>) -> Value {
    Value::Object(
        members
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    )
}

fn span_value(observation: &Observation) -> Value {
    object(vec![
        (
            "start_byte",
            integer(u64::try_from(observation.span.0).unwrap_or(u64::MAX)),
        ),
        (
            "end_byte",
            integer(u64::try_from(observation.span.1).unwrap_or(u64::MAX)),
        ),
        ("start_line", integer(observation.display.start_line)),
        ("start_column", integer(observation.display.start_column)),
        ("end_line", integer(observation.display.end_line)),
        ("end_column", integer(observation.display.end_column)),
    ])
}

fn occurrence_value(engine: &EngineProvenance, observation: &Observation) -> Value {
    let (input, id) = observe::observation_id(
        engine,
        observation.adapter,
        &observation.document,
        observation.construct,
        &observation.node_path,
        observation.projection_digest,
        &observation.intent,
        observation.raw_destination_digest,
    );
    let resolution = crate::evaluate::resolution_value_public(&observation.resolution);
    object(vec![
        ("observation_id", digest_value(id)),
        ("observation_id_input", input),
        ("adapter_id", string(observation.adapter.adapter_id())),
        ("document", string(&observation.document)),
        ("source_construct", string(observation.construct.as_str())),
        ("source_span", span_value(observation)),
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
    ])
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

fn comparison_value(engine: &EngineProvenance, comparison: &Comparison) -> Value {
    let side = |observation: &Option<Observation>| {
        observation
            .as_ref()
            .map_or(Value::Null, |value| occurrence_value(engine, value))
    };
    let list = |members: &[Observation]| {
        Value::Array(
            members
                .iter()
                .map(|member| occurrence_value(engine, member))
                .collect(),
        )
    };
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
            record.classification.adapter(),
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
    path: String,
    classification: &'static str,
    base: Option<&'a DocumentRecord>,
    candidate: Option<&'a DocumentRecord>,
}

fn paired_documents<'a>(
    base: &'a SnapshotDiscovery,
    candidate: &'a SnapshotDiscovery,
) -> Vec<PairedDocument<'a>> {
    let mut paths: Vec<&String> = base
        .documents
        .iter()
        .chain(candidate.documents.iter())
        .map(|record| &record.path)
        .collect();
    paths.sort();
    paths.dedup();
    paths
        .into_iter()
        .map(|path| {
            let find = |side: &'a SnapshotDiscovery| {
                side.documents.iter().find(|record| &record.path == path)
            };
            let base_record = find(base);
            let candidate_record = find(candidate);
            let classification = candidate_record
                .or(base_record)
                .map_or("structured-markdown", |record| {
                    record.classification.as_str()
                });
            PairedDocument {
                path: path.clone(),
                classification,
                base: base_record,
                candidate: candidate_record,
            }
        })
        .collect()
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
        ("path", string(&paired.path)),
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
    enforce: bool,
    comparison_rows: &[(Option<Digest>, Value)],
    document_rows: &[(String, Value)],
) -> Value {
    let scope = finding.kind.scope();
    let coverage = match scope {
        FindingScope::Control => "control-plane",
        FindingScope::Reference | FindingScope::Observation | FindingScope::Document => "none",
    };
    let candidate_fact = finding
        .candidate_fact
        .clone()
        .or_else(|| nonreference_fact(finding, comparison_rows, document_rows));
    let fact_pair = |fact: &Option<(Value, Digest)>| {
        (
            fact.as_ref()
                .map_or(Value::Null, |(_value, digest)| digest_value(*digest)),
            fact.as_ref()
                .map_or(Value::Null, |(value, _digest)| value.clone()),
        )
    };
    let (base_digest, base_fact) = fact_pair(&finding.base_fact);
    let (candidate_digest, candidate_fact_value) = fact_pair(&candidate_fact);
    let trace = trace_value(finding, enforce);
    let location_span = location_span_value(finding);
    object(vec![
        ("key_input", finding.key_input.clone()),
        ("finding_key", digest_value(finding.finding_key)),
        ("kind", string(finding.kind.as_str())),
        ("coverage_requirement", string(coverage)),
        ("evidence_class", string(finding.kind.evidence_class())),
        ("invariant_class", string(finding.kind.invariant_class())),
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
                    }),
                ),
                ("path", nullable(finding.location.path.as_deref())),
                ("span", location_span),
            ]),
        ),
        (
            "observation_ids",
            Value::Array(
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
        ("policy_trace", Value::Array(trace)),
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

fn tree_identity_value(tree: &amiss_wire::model::TreeIdentity) -> Value {
    object(vec![
        (
            "object_format",
            string(match tree.object_format {
                amiss_wire::model::ObjectFormat::Sha1 => "sha1",
                amiss_wire::model::ObjectFormat::Sha256 => "sha256",
            }),
        ),
        ("tree_oid", string(&tree.tree_oid)),
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
fn trace_value(finding: &Finding, _enforce: bool) -> Vec<Value> {
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
        let display = finding
            .location
            .display
            .unwrap_or(crate::scan::SpanDisplay {
                start_line: 1,
                start_column: 1,
                end_line: 1,
                end_column: 1,
            });
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
    })
}

/// A nonreference finding carries exactly one candidate fact embedding the
/// full constructed comparison or document row it was derived from.
fn nonreference_fact(
    finding: &Finding,
    comparison_rows: &[(Option<Digest>, Value)],
    document_rows: &[(String, Value)],
) -> Option<(Value, Digest)> {
    let evidence = match finding.kind.scope() {
        FindingScope::Reference | FindingScope::Control => return None,
        FindingScope::Observation => {
            let id = finding.observation_ids.first()?;
            let row = comparison_rows
                .iter()
                .find(|(primary, _)| primary.as_ref() == Some(id))
                .map(|(_, value)| value.clone())?;
            object(vec![("kind", string("observation")), ("comparison", row)])
        }
        FindingScope::Document => {
            let path = finding.location.path.as_deref()?;
            let row = document_rows
                .iter()
                .find(|(document, _)| document == path)
                .map(|(_, value)| value.clone())?;
            object(vec![("kind", string("document")), ("document_result", row)])
        }
    };
    let fact = object(vec![
        ("schema", string(FACT_SCHEMA)),
        ("finding_kind", string(finding.kind.as_str())),
        ("key_input", finding.key_input.clone()),
        ("evidence", evidence),
    ]);
    let digest = hj(crate::evaluate::FACT_DOMAIN, &fact);
    Some((fact, digest))
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
                Value::Array(reasons.iter().map(|reason| string(reason)).collect()),
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
            setup
                .repository
                .as_ref()
                .map_or(Value::Null, |(owner, name)| {
                    object(vec![
                        ("host", string("github.com")),
                        ("owner", string(owner)),
                        ("name", string(name)),
                    ])
                }),
        ),
        ("ref", nullable(setup.candidate_ref.as_deref())),
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

pub const CANDIDATE_IDENTITY_DOMAIN: &str = "amiss/scanner-candidate-identity/v1";

/// The candidate-identity digest a trusted-time statement must carry: `HJ`
/// over the resolved evaluation's identity projection before time is added.
#[must_use]
pub fn candidate_identity_digest(setup: &Setup) -> Digest {
    let mut rows = vec![("schema", string(CANDIDATE_IDENTITY_DOMAIN))];
    rows.extend(identity_rows(setup));
    hj(CANDIDATE_IDENTITY_DOMAIN, &object(rows))
}

fn evaluation_value(setup: &Setup) -> Value {
    let mut rows = identity_rows(setup);
    rows.push((
        "evaluation_instant",
        setup.policy.time.as_ref().map_or(Value::Null, |time| {
            string(time.statement.evaluation_instant.as_str())
        }),
    ));
    rows.push(("trusted_time", Value::Bool(setup.policy.time.is_some())));
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
            ("reasons", Value::Array(vec![string(reason)])),
        ]);
    }
    let (descriptor, descriptor_digest) = sandbox_descriptor();
    object(vec![
        (
            "profile",
            string(if setup.enforce { "enforce" } else { "observe" }),
        ),
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
                        ("descriptor_digest", digest_value(descriptor.digest)),
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
                        ("trust_source", string("external-required-workflow")),
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
        ("schema", string("amiss/scanner-execution-constraint/v1")),
        (
            "action_repository",
            object(vec![
                ("host", string("github.com")),
                ("owner", string(&descriptor.action_repository.owner)),
                ("name", string(&descriptor.action_repository.name)),
            ]),
        ),
        (
            "action_object_format",
            string(match descriptor.action_object_format {
                amiss_wire::model::ObjectFormat::Sha1 => "sha1",
                amiss_wire::model::ObjectFormat::Sha256 => "sha256",
            }),
        ),
        (
            "action_commit_oid",
            string(descriptor.action_commit_oid.as_str()),
        ),
        (
            "action_tree_oid",
            string(descriptor.action_tree_oid.as_str()),
        ),
        ("manifest_path", string(descriptor.manifest_path.as_str())),
        (
            "release_manifest_digest",
            digest_value(descriptor.release_manifest_digest),
        ),
        (
            "selected_platform",
            string(descriptor.selected_platform.as_str()),
        ),
        (
            "required_status_name",
            string(&descriptor.required_status_name),
        ),
        ("bootstrap_contract", string("amiss-action-bootstrap-v1")),
        (
            "bootstrap_digest",
            digest_value(descriptor.bootstrap_digest),
        ),
    ])
}

fn time_statement_value(statement: &amiss_wire::controls::TrustedTimeStatement) -> Value {
    object(vec![
        ("schema", string("amiss/scanner-trusted-time-statement/v1")),
        (
            "controller",
            string("github-actions-required-workflow-clock-v1"),
        ),
        (
            "repository",
            object(vec![
                ("host", string("github.com")),
                ("owner", string(&statement.repository.owner)),
                ("name", string(&statement.repository.name)),
            ]),
        ),
        ("ref", string(statement.ref_name.as_str())),
        (
            "candidate_identity_digest",
            digest_value(statement.candidate_identity_digest),
        ),
        ("provider_run_id", string(&statement.provider_run_id)),
        (
            "provider_run_attempt",
            integer(statement.provider_run_attempt),
        ),
        (
            "evaluation_instant",
            string(statement.evaluation_instant.as_str()),
        ),
        ("valid_until", string(statement.valid_until.as_str())),
    ])
}

struct Counts {
    documents: Value,
    references: Value,
    findings: Value,
}

fn document_counts(
    candidate_records: &[&DocumentRecord],
    scanned: &[&crate::scan::Scanned],
    unlinked: u64,
) -> Value {
    let count_where = |predicate: &dyn Fn(&&&DocumentRecord) -> bool| {
        u64::try_from(candidate_records.iter().filter(predicate).count()).unwrap_or(u64::MAX)
    };
    let opaque_sum = |select: &dyn Fn(&crate::scan::Scanned) -> u64| {
        scanned.iter().map(|value| select(value)).sum::<u64>()
    };
    let region_bytes = |spans: &Vec<(usize, usize)>| {
        spans
            .iter()
            .map(|(start, end)| u64::try_from(end.saturating_sub(*start)).unwrap_or(u64::MAX))
            .sum::<u64>()
    };
    object(vec![
        (
            "discovered",
            integer(u64::try_from(candidate_records.len()).unwrap_or(u64::MAX)),
        ),
        ("outside_document_set", integer(0)),
        (
            "scanned",
            integer(count_where(&|record| {
                matches!(record.status, DocumentStatus::Scanned(_))
            })),
        ),
        (
            "unsupported",
            integer(count_where(&|record| {
                matches!(record.status, DocumentStatus::Unsupported(_))
            })),
        ),
        (
            "excluded_builtin",
            integer(count_where(&|record| {
                matches!(record.status, DocumentStatus::ExcludedBuiltIn)
            })),
        ),
        ("unlinked", integer(unlinked)),
        (
            "frontmatter_documents",
            integer(opaque_sum(&|value| {
                u64::from(value.opaque.frontmatter_bytes > 0)
            })),
        ),
        (
            "opaque_mdx_documents",
            integer(opaque_sum(&|value| u64::from(!value.opaque.mdx.is_empty()))),
        ),
        (
            "opaque_html_documents",
            integer(opaque_sum(&|value| {
                u64::from(!value.opaque.html.is_empty())
            })),
        ),
        (
            "opaque_mdx_regions",
            integer(opaque_sum(&|value| {
                u64::try_from(value.opaque.mdx.len()).unwrap_or(u64::MAX)
            })),
        ),
        (
            "opaque_mdx_bytes",
            integer(opaque_sum(&|value| region_bytes(&value.opaque.mdx))),
        ),
        (
            "opaque_html_regions",
            integer(opaque_sum(&|value| {
                u64::try_from(value.opaque.html.len()).unwrap_or(u64::MAX)
            })),
        ),
        (
            "opaque_html_bytes",
            integer(opaque_sum(&|value| region_bytes(&value.opaque.html))),
        ),
        (
            "frontmatter_regions",
            integer(opaque_sum(&|value| {
                u64::from(value.opaque.frontmatter_bytes > 0)
            })),
        ),
        (
            "frontmatter_bytes",
            integer(opaque_sum(&|value| {
                u64::try_from(value.opaque.frontmatter_bytes).unwrap_or(u64::MAX)
            })),
        ),
    ])
}

fn reference_counts(comparisons: &[Comparison]) -> Value {
    let candidate_observations: Vec<&Observation> = comparisons
        .iter()
        .flat_map(|comparison| {
            comparison
                .candidate
                .iter()
                .chain(comparison.alternatives_candidate.iter())
        })
        .collect();
    let bucket = |kind: IntentKind| {
        u64::try_from(
            candidate_observations
                .iter()
                .filter(|observation| observation.intent.kind == kind)
                .count(),
        )
        .unwrap_or(u64::MAX)
    };
    let status_count = |status: ResolutionStatus| {
        u64::try_from(
            candidate_observations
                .iter()
                .filter(|observation| observation.resolution.code.status() == status)
                .count(),
        )
        .unwrap_or(u64::MAX)
    };
    object(vec![
        (
            "extracted",
            integer(u64::try_from(candidate_observations.len()).unwrap_or(u64::MAX)),
        ),
        (
            "explicit_local",
            integer(bucket(IntentKind::RepositoryPath)),
        ),
        (
            "same_repository_github",
            integer(bucket(IntentKind::SameRepositoryGithub)),
        ),
        (
            "external_out_of_scope",
            integer(bucket(IntentKind::ExternalUrl)),
        ),
        (
            "unsupported",
            integer(bucket(IntentKind::SiteRoute).saturating_add(bucket(IntentKind::Unsupported))),
        ),
        (
            "resolved",
            integer(status_count(ResolutionStatus::Resolved)),
        ),
        ("missing", integer(status_count(ResolutionStatus::Missing))),
    ])
}

fn summary_counts(
    paired: &[PairedDocument<'_>],
    comparisons: &[Comparison],
    findings: &[Finding],
    finding_rows_count: u64,
) -> Counts {
    let candidate_records: Vec<&DocumentRecord> =
        paired.iter().filter_map(|pair| pair.candidate).collect();
    let scanned: Vec<&crate::scan::Scanned> = candidate_records
        .iter()
        .filter_map(|record| match &record.status {
            DocumentStatus::Scanned(value) => Some(value),
            DocumentStatus::ExcludedBuiltIn
            | DocumentStatus::Unsupported(_)
            | DocumentStatus::Failed(_) => None,
        })
        .collect();
    let unlinked = findings
        .iter()
        .filter(|finding| finding.kind == FindingKind::UnlinkedDocument)
        .count();
    let documents = document_counts(
        &candidate_records,
        &scanned,
        u64::try_from(unlinked).unwrap_or(u64::MAX),
    );

    let references = reference_counts(comparisons);

    let disposition_count = |disposition: Disposition| {
        u64::try_from(
            findings
                .iter()
                .filter(|finding| finding.effective_disposition == disposition)
                .count(),
        )
        .unwrap_or(u64::MAX)
    };
    let attribution_count = |attribution: Attribution| {
        u64::try_from(
            findings
                .iter()
                .filter(|finding| finding.attribution == attribution)
                .count(),
        )
        .unwrap_or(u64::MAX)
    };
    let findings_value = object(vec![
        ("total", integer(finding_rows_count)),
        ("record", integer(disposition_count(Disposition::Record))),
        ("warn", integer(disposition_count(Disposition::Warn))),
        ("fail", integer(disposition_count(Disposition::Fail))),
        (
            "introduced",
            integer(attribution_count(Attribution::Introduced)),
        ),
        (
            "pre_existing",
            integer(attribution_count(Attribution::PreExisting)),
        ),
        (
            "resolved",
            integer(attribution_count(Attribution::Resolved)),
        ),
        ("unknown", integer(attribution_count(Attribution::Unknown))),
        (
            "not_applicable",
            integer(attribution_count(Attribution::NotApplicable)),
        ),
        (
            "debt_tolerated",
            integer(
                u64::try_from(
                    findings
                        .iter()
                        .filter(|finding| finding.debt.is_some())
                        .count(),
                )
                .unwrap_or(u64::MAX),
            ),
        ),
        (
            "waived",
            integer(
                u64::try_from(
                    findings
                        .iter()
                        .filter(|finding| finding.waiver.is_some())
                        .count(),
                )
                .unwrap_or(u64::MAX),
            ),
        ),
        ("analysis_errors", integer(0)),
        ("unsupported_capabilities", integer(0)),
    ]);
    Counts {
        documents,
        references,
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
    comparisons: &[Comparison],
) -> Built {
    let paired = paired_documents(base, candidate);
    let (governed, findings, exception_errors) =
        evaluate_paired(setup, &paired, candidate, comparisons);

    if let Some(crossing) = findings_ceiling_crossing(setup, &findings) {
        let mut details = logical_error_set(&governed, &exception_errors);
        details.push(crossing);
        return construct_incomplete(setup, &details);
    }

    let document_rows: Vec<(String, Value)> = paired
        .iter()
        .map(|pair| (pair.path.clone(), document_result_value(pair)))
        .collect();
    let comparison_rows: Vec<(Option<Digest>, Value)> = comparisons
        .iter()
        .map(|comparison| {
            let primary = comparison
                .candidate
                .as_ref()
                .or(comparison.base.as_ref())
                .map(|observation| observation.id);
            (primary, comparison_value(&setup.engine, comparison))
        })
        .collect();
    let finding_rows: Vec<Value> = findings
        .iter()
        .map(|finding| finding_value(finding, setup.enforce, &comparison_rows, &document_rows))
        .collect();

    let error_details = logical_error_set(&governed, &exception_errors);
    if error_details.len() > error_ceiling(setup) {
        return construct_incomplete(setup, &error_details);
    }
    let governed_errors: Vec<Value> = error_details.iter().map(error_row_value).collect();
    let (complete, status, exit_code) = run_result(&findings, &governed_errors);
    let finding_count = u64::try_from(finding_rows.len()).unwrap_or(u64::MAX);
    let counts = summary_counts(&paired, comparisons, &findings, finding_count);

    let payload = object(vec![
        ("schema", string(PAYLOAD_SCHEMA)),
        ("compatibility", string("experimental")),
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
        (
            "summary",
            object(vec![
                ("counts_complete", Value::Bool(true)),
                ("documents", counts.documents),
                ("references", counts.references),
                ("findings", counts.findings),
                (
                    "human_details_truncated",
                    integer(finding_count.saturating_sub(200)),
                ),
                ("governed_claims", integer(0)),
                ("unattested_claims", integer(0)),
            ]),
        ),
        (
            "documents",
            Value::Array(document_rows.into_iter().map(|(_path, row)| row).collect()),
        ),
        (
            "observations",
            Value::Array(
                comparison_rows
                    .into_iter()
                    .map(|(_primary, row)| row)
                    .collect(),
            ),
        ),
        ("findings", Value::Array(finding_rows)),
        ("errors", Value::Array(governed_errors)),
    ]);
    let payload_digest = hj(PAYLOAD_SCHEMA, &payload);
    let envelope = object(vec![
        ("schema", string(ENVELOPE_SCHEMA)),
        ("payload", payload),
        ("payload_digest", digest_value(payload_digest)),
    ]);
    output_gate(
        setup,
        error_details,
        Built {
            envelope,
            payload_digest,
            status,
            exit_code,
        },
    )
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

/// The counting canonical-serialization pass: a non-error envelope whose
/// wire would exceed the reservation becomes the output-limit fatal
/// projection carrying the exact counted length.
fn output_gate(setup: &Setup, details: Vec<ErrorDetail>, built: Built) -> Built {
    let wire_length = canonical_length(&built.envelope).saturating_add(1);
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
) -> (
    Vec<crate::evaluate::GovernedSeed>,
    Vec<Finding>,
    Vec<ErrorDetail>,
) {
    let inputs: Vec<DocumentInput> = paired.iter().map(document_input).collect();
    let governed = governed_seeds(candidate);
    let (findings, exception_errors) = crate::evaluate::evaluate_with_policy(
        &inputs,
        comparisons,
        setup.enforce,
        &setup.policy,
        &governed,
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

/// One seed per candidate document holding reserved definitions: equal source
/// digests grouped with exact multiplicity, member count as the total node
/// count, and the least location as the representative.
fn governed_seeds(candidate: &SnapshotDiscovery) -> Vec<crate::evaluate::GovernedSeed> {
    let mut seeds = Vec::new();
    for record in &candidate.documents {
        let DocumentStatus::Scanned(scanned) = &record.status else {
            continue;
        };
        if scanned.governed.is_empty() {
            continue;
        }
        let mut sources: Vec<(Digest, u64)> = Vec::new();
        for governed in &scanned.governed {
            match sources
                .iter_mut()
                .find(|(digest, _)| *digest == governed.digest)
            {
                Some((_, multiplicity)) => *multiplicity = multiplicity.saturating_add(1),
                None => sources.push((governed.digest, 1)),
            }
        }
        sources.sort_by_key(|(digest, _)| *digest);
        let representative = scanned.governed.iter().min_by_key(|governed| governed.span);
        seeds.push(crate::evaluate::GovernedSeed {
            document: record.path.clone(),
            member_count: u64::try_from(scanned.governed.len()).unwrap_or(u64::MAX),
            sources,
            representative_span: representative.map(|governed| governed.span),
            representative_display: representative.map(|governed| governed.display),
        });
    }
    seeds
}

fn zero_counts() -> Counts {
    Counts {
        documents: document_counts(&[], &[], 0),
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
            ("analysis_errors", integer(0)),
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
    let analysis_errors = error_count;
    let counts = zero_counts();
    let findings_with_errors = match counts.findings {
        Value::Object(mut members) => {
            for (key, value) in &mut members {
                if key == "analysis_errors" {
                    *value = integer(analysis_errors);
                }
            }
            Value::Object(members)
        }
        other @ (Value::Null
        | Value::Bool(_)
        | Value::Integer(_)
        | Value::String(_)
        | Value::Array(_)) => other,
    };

    let payload = object(vec![
        ("schema", string(PAYLOAD_SCHEMA)),
        ("compatibility", string("experimental")),
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
        (
            "summary",
            object(vec![
                ("counts_complete", Value::Bool(false)),
                ("documents", counts.documents),
                ("references", counts.references),
                ("findings", findings_with_errors),
                ("human_details_truncated", integer(0)),
                ("governed_claims", integer(0)),
                ("unattested_claims", integer(0)),
            ]),
        ),
        ("documents", Value::Array(Vec::new())),
        ("observations", Value::Array(Vec::new())),
        ("findings", Value::Array(Vec::new())),
        ("errors", Value::Array(error_rows)),
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
