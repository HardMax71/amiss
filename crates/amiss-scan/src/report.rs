use amiss_wire::controls::ContentAvailability;
use amiss_wire::digest::{Digest, hj};
use amiss_wire::json::{Value, canonical};
use amiss_wire::model::Adapter;
use amiss_wire::report::{
    Disposition, EngineProvenance, ErrorDetail, FindingKind, FindingScope, IntentKind,
    PAYLOAD_SCHEMA, ResolutionStatus, engine_block, error_row_value, sandbox_descriptor,
};

use crate::correlate::{Comparison, Observation, Outcome, Reason, SourceChange, TargetChange};
use crate::discovery::{DocumentRecord, DocumentStatus, SnapshotDiscovery, UnsupportedKind};
use crate::evaluate::{
    Attribution, DocumentInput, DocumentSide, FACT_SCHEMA, Finding, LocationSide, evaluate,
};
use crate::{Impact, observe};

pub const ENVELOPE_SCHEMA: &str = "amiss/scanner-report-envelope/v1";

/// One snapshot's identity in the evaluation block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotIdentity {
    pub object_format: &'static str,
    pub commit_oid: String,
    pub tree_oid: String,
}

/// The run identity a complete local commit-pair report carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Setup {
    pub engine: EngineProvenance,
    pub enforce: bool,
    pub repository: Option<(String, String)>,
    pub candidate_ref: Option<String>,
    pub default_branch_ref: Option<String>,
    pub base: SnapshotIdentity,
    pub candidate: SnapshotIdentity,
}

/// A constructed complete report: the envelope value, its canonical wire
/// bytes with the trailing newline, the payload digest, and the result the
/// process must exit with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Built {
    pub envelope: Value,
    pub wire: Vec<u8>,
    pub payload_digest: Digest,
    pub status: &'static str,
    pub exit_code: i64,
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
            Some(record.classification.adapter()),
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
        ("debt", Value::Null),
        ("waiver", Value::Null),
    ])
}

/// The policy trace: the one built-in step, or the one resolved-projection
/// step for a base-only resolved reference.
fn trace_value(finding: &Finding, enforce: bool) -> Vec<Value> {
    if finding.attribution == Attribution::Resolved {
        return vec![object(vec![
            ("source", string("resolved-projection")),
            ("rule_id", string("resolved-projection-v1")),
            ("before", string("record")),
            ("after", string("record")),
        ])];
    }
    let profile = if enforce { "enforce" } else { "observe" };
    vec![object(vec![
        ("source", string("built-in")),
        (
            "rule_id",
            string(&format!(
                "scanner-policy-defaults-v1/{}/{profile}",
                finding.kind.as_str()
            )),
        ),
        ("before", string(finding.configured_disposition.as_str())),
        ("after", string(finding.effective_disposition.as_str())),
    ])]
}

fn location_span_value(finding: &Finding) -> Value {
    finding.location.span.map_or(Value::Null, |span| {
        object(vec![
            (
                "start_byte",
                integer(u64::try_from(span.0).unwrap_or(u64::MAX)),
            ),
            (
                "end_byte",
                integer(u64::try_from(span.1).unwrap_or(u64::MAX)),
            ),
            ("start_line", integer(1)),
            ("start_column", integer(1)),
            ("end_line", integer(1)),
            ("end_column", integer(1)),
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

fn evaluation_value(setup: &Setup) -> Value {
    object(vec![
        ("mode", string("commit-pair")),
        ("event_kind", string("explicit-commit-pair")),
        ("finality", string("explicit-replay")),
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
        ("candidate", snapshot_value(&setup.candidate)),
        ("materialization", string("git-objects")),
        ("skip_worktree_paths", integer(0)),
        ("index_only_materialized_paths", integer(0)),
        ("evaluation_instant", Value::Null),
        ("trusted_time", Value::Bool(false)),
    ])
}

fn controls_value(setup: &Setup) -> Value {
    let none_provenance = || {
        object(vec![
            ("status", string("none")),
            ("digest", Value::Null),
            ("trust_source", string("none")),
        ])
    };
    let (descriptor, descriptor_digest) = sandbox_descriptor();
    object(vec![
        (
            "profile",
            string(if setup.enforce { "enforce" } else { "observe" }),
        ),
        ("base_repository_policy_digest", Value::Null),
        ("candidate_repository_policy_digest", Value::Null),
        ("organization_floor", none_provenance()),
        ("debt_snapshot", none_provenance()),
        ("waiver_bundle", none_provenance()),
        (
            "execution_constraint",
            object(vec![("status", string("none"))]),
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
            object(vec![("status", string("none"))]),
        ),
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
        ("debt_tolerated", integer(0)),
        ("waived", integer(0)),
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
    let inputs: Vec<DocumentInput> = paired.iter().map(document_input).collect();
    let findings = evaluate(&inputs, comparisons, setup.enforce);

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

    let failing = findings
        .iter()
        .any(|finding| finding.effective_disposition == Disposition::Fail);
    let (status, exit_code) = if failing { ("fail", 1) } else { ("pass", 0) };
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
            object(vec![
                ("complete", Value::Bool(true)),
                ("status", string(status)),
                ("exit_code", Value::Integer(exit_code)),
                ("finding_count", integer(finding_count)),
                ("error_count", integer(0)),
            ]),
        ),
        (
            "summary",
            object(vec![
                ("counts_complete", Value::Bool(true)),
                ("documents", counts.documents),
                ("references", counts.references),
                ("findings", counts.findings),
                ("human_details_truncated", integer(0)),
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
        ("errors", Value::Array(Vec::new())),
    ]);
    let payload_digest = hj(PAYLOAD_SCHEMA, &payload);
    let envelope = object(vec![
        ("schema", string(ENVELOPE_SCHEMA)),
        ("payload", payload),
        ("payload_digest", digest_value(payload_digest)),
    ]);
    let mut wire = canonical(&envelope);
    wire.push(b'\n');
    Built {
        envelope,
        wire,
        payload_digest,
        status,
        exit_code,
    }
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
    let mut sorted: Vec<&ErrorDetail> = details.iter().collect();
    sorted.sort();
    sorted.dedup();
    let error_rows: Vec<Value> = sorted
        .iter()
        .map(|detail| error_row_value(detail))
        .collect();
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
    let mut wire = canonical(&envelope);
    wire.push(b'\n');
    Built {
        envelope,
        wire,
        payload_digest,
        status: "incomplete",
        exit_code: 2,
    }
}
