use super::documents::{self, DocumentResult};
use crate::correlate::{Comparison, Observation};
use crate::discovery::{DocumentRecord, DocumentStatus};
use crate::evaluate::{DocumentInput, DocumentSide, Finding, FindingFact, FindingFix};
use crate::{SpanDisplay, feedback, observe};
use amiss_wire::codec;
use amiss_wire::controls::SourceConstruct;
use amiss_wire::de::{Error, ErrorKind};
use amiss_wire::digest::Digest;
use amiss_wire::json::Value;
use amiss_wire::model::{Adapter, RepoPath, TreeIdentity};
use amiss_wire::report::FindingScope;
use serde::Serialize;

#[derive(Serialize)]
struct SourceSpan {
    end_byte: usize,
    end_column: u64,
    end_line: u64,
    start_byte: usize,
    start_column: u64,
    start_line: u64,
}

fn source_span(span: (usize, usize), display: SpanDisplay) -> SourceSpan {
    SourceSpan {
        end_byte: span.1,
        end_column: display.end_column,
        end_line: display.end_line,
        start_byte: span.0,
        start_column: display.start_column,
        start_line: display.start_line,
    }
}

#[derive(Serialize)]
struct Occurrence<'a> {
    adapter_id: Adapter,
    block_kind: &'a str,
    document: &'a RepoPath,
    #[serde(skip_serializing_if = "Option::is_none")]
    external_destination: Option<&'a str>,
    intent: observe::ExtractedIntent<'a>,
    observation_id: Digest,
    observation_id_input: observe::ObservationInput<'a>,
    resolution: &'a crate::resolve::Resolution,
    source_construct: SourceConstruct,
    source_projection_digest: Digest,
    source_span: SourceSpan,
}

fn occurrence(observation: &Observation) -> Result<Occurrence<'_>, Error> {
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
    let observation_id = observe::observation_digest(&identity).map_err(|defect| {
        Error::described("$".to_owned(), ErrorKind::InvalidValue, defect.to_string())
    })?;
    Ok(Occurrence {
        adapter_id: observation.adapter,
        block_kind: observation.block_kind.as_ref(),
        document: &observation.document,
        external_destination: observation.external_destination.as_deref(),
        intent: observe::extracted_intent(&observation.intent, observation.raw_destination_digest),
        observation_id,
        observation_id_input: observe::observation_projection(&identity),
        resolution: &observation.resolution,
        source_construct: observation.construct,
        source_projection_digest: observation.projection_digest,
        source_span: source_span(observation.span, observation.display),
    })
}

#[derive(Serialize)]
struct Alternatives<'a> {
    base: Vec<Occurrence<'a>>,
    candidate: Vec<Occurrence<'a>>,
}

#[derive(Serialize)]
pub(super) struct ComparisonRow<'a> {
    alternatives: Alternatives<'a>,
    base: Option<Occurrence<'a>>,
    candidate: Option<Occurrence<'a>>,
    correlation: &'a str,
    correlation_reason: &'a str,
    impact: &'a str,
    source_change: &'a str,
    target_change: &'a str,
}

pub(super) fn comparison_value(comparison: &Comparison) -> Result<ComparisonRow<'_>, Error> {
    Ok(ComparisonRow {
        alternatives: Alternatives {
            base: comparison
                .alternatives_base
                .iter()
                .map(occurrence)
                .collect::<Result<_, _>>()?,
            candidate: comparison
                .alternatives_candidate
                .iter()
                .map(occurrence)
                .collect::<Result<_, _>>()?,
        },
        base: comparison.base.as_ref().map(occurrence).transpose()?,
        candidate: comparison.candidate.as_ref().map(occurrence).transpose()?,
        correlation: comparison.outcome.as_ref(),
        correlation_reason: comparison.reason.as_ref(),
        impact: comparison.impact.as_ref(),
        source_change: comparison.source_change.as_ref(),
        target_change: comparison.target_change.as_ref(),
    })
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

#[derive(Serialize)]
struct Aggregation {
    locations_omitted: u8,
    member_count: u64,
    representative_rule: &'static str,
    strategy: &'static str,
}

#[derive(Serialize)]
struct Location<'a> {
    path: Option<&'a RepoPath>,
    side: &'a str,
    span: Option<SourceSpan>,
}

#[derive(Serialize)]
struct TraceStep<'a> {
    after: &'a str,
    before: &'a str,
    rule_id: &'a str,
    source: &'static str,
}

#[derive(Serialize)]
struct DebtProjection<'a> {
    accepted_fact_digest: Digest,
    adoption_tree: &'a TreeIdentity,
    created_at: &'a str,
    debt_id: &'a str,
    debt_snapshot_digest: Digest,
    expires_at: &'a str,
    owner: &'a str,
    reason: &'a str,
}

#[derive(Serialize)]
struct WaiverProjection<'a> {
    authorized_fact_digest: Digest,
    candidate_tree: &'a TreeIdentity,
    created_at: &'a str,
    expires_at: &'a str,
    issuer: &'a str,
    not_before: &'a str,
    owner: &'a str,
    reason: &'a str,
    residual_disposition: &'static str,
    waiver_bundle_digest: Digest,
    waiver_id: &'a str,
}

#[derive(Serialize)]
struct FindingRow<'a> {
    aggregation: Aggregation,
    attribution: &'a str,
    base_fact: Option<&'a Value>,
    base_fact_digest: Option<Digest>,
    candidate_fact: Option<&'a Value>,
    candidate_fact_digest: Option<Digest>,
    configured_disposition: &'a str,
    coverage_requirement: &'static str,
    debt: Option<DebtProjection<'a>>,
    description: &'static str,
    effective_disposition: &'a str,
    evidence_class: &'static str,
    finding_key: Digest,
    fix: Option<Fix<'a>>,
    invariant_class: &'static str,
    key_input: Value,
    kind: &'a str,
    location: Location<'a>,
    observation_ids: &'a [Digest],
    policy_trace: Vec<TraceStep<'a>>,
    waiver: Option<WaiverProjection<'a>>,
}

pub(super) struct FindingProjection<'a> {
    finding: &'a Finding,
    candidate_fact: Option<FindingFact>,
}

pub(super) fn finding_value<'a>(
    finding: &'a Finding,
    comparison_runs: [&[(Option<Digest>, ComparisonRow<'_>)]; 2],
    document_rows: &[(&RepoPath, DocumentResult<'_>)],
) -> Result<FindingProjection<'a>, Error> {
    let candidate_fact = if finding.candidate_fact().is_some() {
        None
    } else {
        nonreference_fact(finding, comparison_runs, document_rows)?
    };
    Ok(FindingProjection {
        finding,
        candidate_fact,
    })
}

impl Serialize for FindingProjection<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let finding = self.finding;
        let kind = finding.kind();
        let metadata = kind.metadata();
        let candidate_fact = finding.candidate_fact().or(self.candidate_fact.as_ref());
        let coverage_requirement = match metadata.scope {
            FindingScope::Control => "control-plane",
            FindingScope::Reference | FindingScope::Observation | FindingScope::Document => "none",
        };
        let span = finding.location.span.map(|span| {
            source_span(
                span,
                finding.location.display.unwrap_or(SpanDisplay {
                    start_line: 1,
                    start_column: 1,
                    end_line: 1,
                    end_column: 1,
                }),
            )
        });
        FindingRow {
            aggregation: Aggregation {
                locations_omitted: 0,
                member_count: finding.member_count,
                representative_rule: "lowest-location-then-observation-id",
                strategy: "one-per-finding-key",
            },
            attribution: finding.attribution.as_ref(),
            base_fact: finding.base_fact().map(FindingFact::value),
            base_fact_digest: finding.base_fact().map(FindingFact::digest),
            candidate_fact: candidate_fact.map(FindingFact::value),
            candidate_fact_digest: candidate_fact.map(FindingFact::digest),
            configured_disposition: finding.configured_disposition.as_ref(),
            coverage_requirement,
            debt: finding.debt.as_ref().map(|applied| DebtProjection {
                accepted_fact_digest: applied.item.accepted_fact_digest,
                adoption_tree: &applied.adoption_tree,
                created_at: applied.item.created_at.as_str(),
                debt_id: applied.item.debt_id.as_str(),
                debt_snapshot_digest: applied.snapshot_digest,
                expires_at: applied.item.expires_at.as_str(),
                owner: applied.item.owner.as_str(),
                reason: &applied.item.reason,
            }),
            description: kind.meaning(),
            effective_disposition: finding.effective_disposition.as_ref(),
            evidence_class: metadata.evidence_class,
            finding_key: finding.key().digest(),
            fix: finding.fix().map(fix_projection),
            invariant_class: metadata.invariant_class,
            key_input: finding.key().to_value(),
            kind: kind.as_ref(),
            location: Location {
                path: finding.location.path.as_ref(),
                side: finding.location.side.as_ref(),
                span,
            },
            observation_ids: &finding.observation_ids,
            policy_trace: finding
                .steps
                .iter()
                .map(|step| TraceStep {
                    after: step.after.as_ref(),
                    before: step.before.as_ref(),
                    rule_id: &step.rule_id,
                    source: step.source,
                })
                .collect(),
            waiver: finding.waiver.as_ref().map(|applied| WaiverProjection {
                authorized_fact_digest: applied.item.authorized_fact_digest,
                candidate_tree: &applied.item.candidate_tree,
                created_at: applied.item.created_at.as_str(),
                expires_at: applied.item.expires_at.as_str(),
                issuer: applied.item.issuer.as_str(),
                not_before: applied.item.not_before.as_str(),
                owner: applied.item.owner.as_str(),
                reason: &applied.item.reason,
                residual_disposition: "warn",
                waiver_bundle_digest: applied.bundle_digest,
                waiver_id: applied.item.waiver_id.as_str(),
            }),
        }
        .serialize(serializer)
    }
}

pub(super) struct FeedbackProjection(Option<feedback::Feedback>);

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
enum FeedbackView<'a> {
    Available {
        existing_count: u64,
        items: Vec<FeedbackItem<'a>>,
    },
    Unavailable,
}

#[derive(Serialize)]
struct FeedbackItem<'a> {
    action: &'a str,
    annotation: Option<Annotation<'a>>,
    effective_disposition: &'a str,
    finding_kinds: Vec<&'a str>,
    location_count: u64,
    target: Option<&'a RepoPath>,
}

#[derive(Serialize)]
struct Annotation<'a> {
    path: &'a str,
    span: SourceSpan,
}

impl Serialize for FeedbackProjection {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let view = self
            .0
            .as_ref()
            .map_or(FeedbackView::Unavailable, |feedback| {
                FeedbackView::Available {
                    existing_count: feedback.existing_count,
                    items: feedback
                        .items
                        .iter()
                        .map(|item| FeedbackItem {
                            action: item.action.as_ref(),
                            annotation: item.annotation.as_ref().map(|annotation| Annotation {
                                path: &annotation.path,
                                span: source_span(annotation.span, annotation.display),
                            }),
                            effective_disposition: item.effective_disposition.as_ref(),
                            finding_kinds: item.finding_kinds.iter().map(AsRef::as_ref).collect(),
                            location_count: item.location_count,
                            target: item.target.as_ref(),
                        })
                        .collect(),
                }
            });
        view.serialize(serializer)
    }
}

pub(super) fn feedback_value(
    complete: bool,
    findings: &[Finding],
    comparisons: &[Comparison],
) -> FeedbackProjection {
    FeedbackProjection(complete.then(|| feedback::project(findings, comparisons)))
}

#[derive(Serialize)]
struct ByteSpan {
    end_byte: usize,
    start_byte: usize,
}

#[derive(Serialize)]
struct Fix<'a> {
    description: &'static str,
    path: &'a RepoPath,
    replacement: &'a str,
    span: ByteSpan,
}

fn fix_projection(fix: &FindingFix) -> Fix<'_> {
    Fix {
        description: fix.kind.meaning(),
        path: &fix.path,
        replacement: &fix.replacement,
        span: ByteSpan {
            end_byte: fix.span.1,
            start_byte: fix.span.0,
        },
    }
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
enum NonreferenceEvidence<'a, 'b> {
    Observation {
        comparison: &'a ComparisonRow<'b>,
    },
    Document {
        document_result: &'a DocumentResult<'b>,
    },
}

fn nonreference_fact(
    finding: &Finding,
    comparison_runs: [&[(Option<Digest>, ComparisonRow<'_>)]; 2],
    document_rows: &[(&RepoPath, DocumentResult<'_>)],
) -> Result<Option<FindingFact>, Error> {
    let evidence = match finding.kind().metadata().scope {
        FindingScope::Reference | FindingScope::Control => return Ok(None),
        FindingScope::Observation => {
            let Some(id) = finding.observation_ids.first() else {
                return Ok(None);
            };
            let Some(row) = comparison_runs.into_iter().find_map(|rows| {
                rows.binary_search_by_key(&Some(*id), |(primary, _)| *primary)
                    .ok()
                    .and_then(|index| rows.get(index))
                    .map(|(_, value)| value)
            }) else {
                return Ok(None);
            };
            NonreferenceEvidence::Observation { comparison: row }
        }
        FindingScope::Document => {
            let Some(path) = finding.location.path.as_ref() else {
                return Ok(None);
            };
            let Some(row) = document_rows
                .binary_search_by(|(document, _)| (*document).cmp(path))
                .ok()
                .and_then(|index| document_rows.get(index))
                .map(|(_, value)| value)
            else {
                return Ok(None);
            };
            NonreferenceEvidence::Document {
                document_result: row,
            }
        }
    };
    Ok(Some(FindingFact::new(
        finding.key(),
        codec::to_value(&evidence)?,
    )))
}
