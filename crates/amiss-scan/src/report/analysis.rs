use amiss_wire::digest::Digest;
use amiss_wire::json::Value;
use amiss_wire::model::RepoPath;
use amiss_wire::report::FindingScope;

use crate::correlate::{Comparison, Observation};
use crate::discovery::{DocumentRecord, DocumentStatus};
use crate::evaluate::{DocumentInput, DocumentSide, Finding, FindingFact, FindingFix};
use crate::feedback;
use crate::{SpanDisplay, observe};

use super::documents;
use super::{digest_value, integer, nullable_path, object, string};

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
        ("adapter_id", string(observation.adapter.as_ref())),
        ("document", observation.document.to_value()),
        ("source_construct", string(observation.construct.as_ref())),
        (
            "source_span",
            source_span_value(observation.span, observation.display),
        ),
        ("block_kind", string(observation.block_kind.as_ref())),
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

pub(super) fn comparison_value(comparison: &Comparison) -> Value {
    let side = |observation: &Option<Observation>| {
        observation.as_ref().map_or(Value::Null, occurrence_value)
    };
    let list =
        |members: &[Observation]| Value::array(members.iter().map(occurrence_value).collect());
    object(vec![
        ("base", side(&comparison.base)),
        ("candidate", side(&comparison.candidate)),
        ("correlation", string(comparison.outcome.as_ref())),
        ("correlation_reason", string(comparison.reason.as_ref())),
        (
            "alternatives",
            object(vec![
                ("base", list(&comparison.alternatives_base)),
                ("candidate", list(&comparison.alternatives_candidate)),
            ]),
        ),
        ("source_change", string(comparison.source_change.as_ref())),
        ("target_change", string(comparison.target_change.as_ref())),
        ("impact", string(comparison.impact.as_ref())),
    ])
}

pub(super) fn document_input(paired: &documents::PairedDocument<'_>) -> DocumentInput {
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

pub(super) fn finding_value(
    finding: &Finding,
    comparison_runs: [&[(Option<Digest>, Value)]; 2],
    document_rows: &[(RepoPath, Value)],
) -> Value {
    let kind = finding.kind();
    let metadata = kind.metadata();
    let coverage = match metadata.scope {
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
    let location_span = finding.location.span.map_or(Value::Null, |span| {
        let display = finding.location.display.unwrap_or(SpanDisplay {
            start_line: 1,
            start_column: 1,
            end_line: 1,
            end_column: 1,
        });
        source_span_value(span, display)
    });
    let (debt, waiver) = application_values(finding);
    object(vec![
        ("key_input", finding.key().to_value()),
        ("finding_key", digest_value(finding.key().digest())),
        ("kind", string(kind.as_ref())),
        ("description", string(kind.meaning())),
        ("fix", finding.fix().map_or(Value::Null, fix_value)),
        ("coverage_requirement", string(coverage)),
        ("evidence_class", string(metadata.evidence_class)),
        ("invariant_class", string(metadata.invariant_class)),
        ("attribution", string(finding.attribution.as_ref())),
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
                ("side", string(finding.location.side.as_ref())),
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
            string(finding.configured_disposition.as_ref()),
        ),
        (
            "effective_disposition",
            string(finding.effective_disposition.as_ref()),
        ),
        ("policy_trace", Value::array(trace_value(finding))),
        ("debt", debt),
        ("waiver", waiver),
    ])
}

fn available_feedback_value(projected: feedback::Feedback) -> Value {
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
                ("action", string(item.action.as_ref())),
                ("target", nullable_path(item.target.as_ref())),
                (
                    "finding_kinds",
                    Value::array(
                        item.finding_kinds
                            .into_iter()
                            .map(|kind| string(kind.as_ref()))
                            .collect(),
                    ),
                ),
                ("location_count", integer(item.location_count)),
                (
                    "effective_disposition",
                    string(item.effective_disposition.as_ref()),
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

pub(super) fn feedback_value(
    complete: bool,
    findings: &[Finding],
    comparisons: &[Comparison],
) -> Value {
    if complete {
        available_feedback_value(feedback::project(findings, comparisons))
    } else {
        object(vec![("status", string("unavailable"))])
    }
}

fn tree_identity_value(tree: &amiss_wire::model::TreeIdentity) -> Value {
    object(vec![
        ("object_format", string(tree.object_format().as_ref())),
        ("tree_oid", string(tree.tree_oid())),
    ])
}

fn application_rows(
    owner: &str,
    reason: &str,
    created_at: &str,
    expires_at: &str,
) -> [(&'static str, Value); 4] {
    [
        ("owner", string(owner)),
        ("reason", string(reason)),
        ("created_at", string(created_at)),
        ("expires_at", string(expires_at)),
    ]
}

fn application_values(finding: &Finding) -> (Value, Value) {
    let debt = finding.debt.as_ref().map_or(Value::Null, |applied| {
        let mut rows = vec![
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
        ];
        rows.extend(application_rows(
            applied.item.owner.as_str(),
            &applied.item.reason,
            applied.item.created_at.as_str(),
            applied.item.expires_at.as_str(),
        ));
        object(rows)
    });
    let waiver = finding.waiver.as_ref().map_or(Value::Null, |applied| {
        let mut rows = vec![
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
            ("issuer", string(applied.item.issuer.as_str())),
            ("not_before", string(applied.item.not_before.as_str())),
            ("residual_disposition", string("warn")),
        ];
        rows.extend(application_rows(
            applied.item.owner.as_str(),
            &applied.item.reason,
            applied.item.created_at.as_str(),
            applied.item.expires_at.as_str(),
        ));
        object(rows)
    });
    (debt, waiver)
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
                ("before", string(step.before.as_ref())),
                ("after", string(step.after.as_ref())),
            ])
        })
        .collect()
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
    let evidence = match finding.kind().metadata().scope {
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
