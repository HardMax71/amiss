use amiss_wire::controls::GitMode;
use amiss_wire::digest::{Digest, hj_serde};
use amiss_wire::model::{RepoPath, RepoPathText};
use amiss_wire::report::FindingScope;
use amiss_wire::report::model;
use amiss_wire::resolution::Resolution;

use crate::correlate::{Comparison, Observation};
use crate::discovery::{DocumentRecord, DocumentStatus};
use crate::evaluate::{DocumentInput, DocumentSide, Finding, FindingFact};
use crate::feedback;
use crate::{SpanDisplay, observe};

use super::documents;

fn source_span(span: (usize, usize), display: SpanDisplay) -> model::SourceSpan {
    model::SourceSpan {
        start_byte: u64::try_from(span.0).unwrap_or(u64::MAX),
        end_byte: u64::try_from(span.1).unwrap_or(u64::MAX),
        start_line: display.start_line,
        start_column: display.start_column,
        end_line: display.end_line,
        end_column: display.end_column,
    }
}

fn occurrence(
    observation: Observation,
) -> Result<model::Occurrence<RepoPath, Resolution<RepoPath>>, crate::Error> {
    let identity = observe::ObservationIdentity {
        adapter: observation.adapter,
        contract_digest: observation.adapter_contract_digest,
        document: observation.document.clone(),
        repository_path: observation.intent.repository_path.clone(),
        construct: observation.construct,
        node_path: &observation.node_path,
        projection_digest: observation.projection_digest,
        intent: &observation.intent,
        raw_destination_digest: observation.raw_destination_digest,
    };
    let input = observe::observation_input(identity)?;
    let id = hj_serde(observe::OBSERVATION_ID_DOMAIN, |writer| {
        serde_json::to_writer(writer, &input)
    })
    .map_err(|_defect| crate::Error::Internal)?;
    Ok(model::Occurrence {
        observation_id: id,
        intent: input.extracted_intent.clone(),
        observation_id_input: input,
        adapter_id: observation.adapter,
        document: observation.document,
        source_construct: observation.construct,
        source_span: source_span(observation.span, observation.display),
        block_kind: observation.block_kind,
        source_projection_digest: observation.projection_digest,
        resolution: observation.resolution,
        external_destination: observation.external_destination,
    })
}

pub(super) fn comparison(
    comparison: Comparison,
) -> Result<model::ObservationComparison<RepoPath, Resolution<RepoPath>>, crate::Error> {
    let [base, candidate] =
        [comparison.base, comparison.candidate].map(|side| side.map(occurrence).transpose());
    let [alternatives_base, alternatives_candidate] = [
        comparison.alternatives_base,
        comparison.alternatives_candidate,
    ]
    .map(|side| side.into_iter().map(occurrence).collect::<Result<_, _>>());
    Ok(model::ObservationComparison {
        base: base?,
        candidate: candidate?,
        correlation: comparison.outcome,
        correlation_reason: comparison.reason,
        alternatives: model::CorrelationAlternatives {
            base: alternatives_base?,
            candidate: alternatives_candidate?,
        },
        source_change: comparison.source_change,
        target_change: comparison.target_change,
        impact: comparison.impact,
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

pub(super) fn finding<E>(finding: Finding<E>) -> model::Finding<RepoPath, E> {
    let kind = finding.key_input.finding_kind;
    let metadata = kind.metadata();
    model::Finding {
        key_input: finding.key_input,
        finding_key: finding.finding_key,
        kind,
        description: kind.meaning().to_owned(),
        fix: finding.fix.map(|fix| model::FindingFix {
            path: fix.path,
            span: model::ByteSpan {
                start_byte: u64::try_from(fix.span.0).unwrap_or(u64::MAX),
                end_byte: u64::try_from(fix.span.1).unwrap_or(u64::MAX),
            },
            replacement: fix.replacement,
            description: fix.kind.meaning().to_owned(),
        }),
        coverage_requirement: match metadata.scope {
            FindingScope::Control => model::CoverageRequirement::ControlPlane,
            FindingScope::Reference | FindingScope::Observation | FindingScope::Document => {
                model::CoverageRequirement::None
            }
        },
        evidence_class: metadata.evidence_class,
        invariant_class: metadata.invariant_class,
        attribution: finding.attribution,
        base_fact_digest: finding.base_fact.as_ref().map(|fact| fact.digest),
        base_fact: finding.base_fact.map(|fact| fact.input),
        candidate_fact_digest: finding.candidate_fact.as_ref().map(|fact| fact.digest),
        candidate_fact: finding.candidate_fact.map(|fact| fact.input),
        aggregation: model::FindingAggregation {
            strategy: model::AggregationStrategy::OnePerFindingKey,
            member_count: finding.member_count,
            locations_omitted: 0,
            representative_rule: model::RepresentativeRule::LowestLocationThenObservationId,
        },
        location: model::FindingLocation {
            side: finding.location.side,
            path: finding.location.path,
            span: finding.location.span.map(|span| {
                source_span(
                    span,
                    finding.location.display.unwrap_or(SpanDisplay {
                        start_line: 1,
                        start_column: 1,
                        end_line: 1,
                        end_column: 1,
                    }),
                )
            }),
        },
        observation_ids: finding.observation_ids,
        configured_disposition: finding.configured_disposition,
        effective_disposition: finding.effective_disposition,
        policy_trace: finding.steps,
        debt: finding.debt.map(|applied| model::DebtApplication {
            debt_id: applied.item.debt_id,
            debt_snapshot_digest: applied.snapshot_digest,
            adoption_tree: applied.adoption_tree,
            accepted_fact_digest: applied.item.accepted_fact_digest,
            owner: applied.item.owner,
            reason: applied.item.reason,
            created_at: applied.item.created_at,
            expires_at: applied.item.expires_at,
        }),
        waiver: finding.waiver.map(|applied| model::WaiverApplication {
            waiver_id: applied.item.waiver_id,
            waiver_bundle_digest: applied.bundle_digest,
            candidate_tree: applied.item.candidate_tree,
            authorized_fact_digest: applied.item.authorized_fact_digest,
            issuer: applied.item.issuer,
            not_before: applied.item.not_before,
            residual_disposition: applied.item.residual_disposition,
            owner: applied.item.owner,
            reason: applied.item.reason,
            created_at: applied.item.created_at,
            expires_at: applied.item.expires_at,
        }),
    }
}

pub(super) fn feedback(
    complete: bool,
    findings: &[Finding],
    comparisons: &[Comparison],
) -> Result<model::Feedback<RepoPath>, crate::Error> {
    if !complete {
        return Ok(model::Feedback::Unavailable(model::UnavailableFeedback {
            status: model::UnavailableStatus::Unavailable,
        }));
    }
    let projected = feedback::project(findings, comparisons);
    let items = projected
        .items
        .into_iter()
        .map(|item| {
            Ok(model::FeedbackItem {
                action: item.action,
                target: item.target,
                finding_kinds: item.finding_kinds,
                location_count: item
                    .location_count
                    .try_into()
                    .map_err(|_defect| crate::Error::Internal)?,
                effective_disposition: item.effective_disposition,
                annotation: item
                    .annotation
                    .map(|annotation| -> Result<_, crate::Error> {
                        Ok(model::FeedbackAnnotation {
                            path: RepoPathText::new(annotation.path)
                                .ok_or(crate::Error::Internal)?,
                            span: source_span(annotation.span, annotation.display),
                        })
                    })
                    .transpose()?,
            })
        })
        .collect::<Result<_, crate::Error>>()?;
    Ok(model::Feedback::Available(model::AvailableFeedback {
        status: model::AvailableFeedbackStatus::Available,
        items,
        existing_count: projected.existing_count,
    }))
}

#[expect(
    clippy::type_complexity,
    reason = "the two ordered original-ID runs must survive observation rehashing"
)]
pub(super) fn nonreference_fact(
    finding: &Finding,
    comparison_runs: [&[(
        Option<Digest>,
        model::ObservationComparison<RepoPath, Resolution<RepoPath>>,
    )]; 2],
    document_rows: &[model::DocumentResult<RepoPath, model::DocumentSide<GitMode>>],
) -> Result<Option<FindingFact>, crate::Error> {
    let evidence = match finding.key_input.finding_kind.metadata().scope {
        FindingScope::Reference | FindingScope::Control => None,
        FindingScope::Observation => finding.observation_ids.first().and_then(|id| {
            comparison_runs.into_iter().find_map(|rows| {
                rows.binary_search_by_key(&Some(*id), |(primary, _)| *primary)
                    .ok()
                    .and_then(|index| rows.get(index))
                    .map(|(_, row)| model::FindingFactEvidence::Observation {
                        kind: model::ObservationFactEvidenceKind::Observation,
                        comparison: Box::new(row.clone()),
                    })
            })
        }),
        FindingScope::Document => finding.location.path.as_ref().and_then(|path| {
            document_rows
                .binary_search_by(|document| document.path.cmp(path))
                .ok()
                .and_then(|index| document_rows.get(index))
                .map(|row| model::FindingFactEvidence::Document {
                    kind: model::DocumentFactEvidenceKind::Document,
                    document_result: row.clone(),
                })
        }),
    };
    evidence
        .map(|evidence| crate::evaluate::fact(&finding.key_input, evidence))
        .transpose()
}
