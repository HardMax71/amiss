use amiss_wire::controls::ResourceName;
use amiss_wire::digest::hj_serde;
use amiss_wire::model::RepoPath;
use amiss_wire::report::model;
use amiss_wire::report::{
    AnalysisErrorCode, Disposition, ErrorDetail, MACHINE_JSON_BYTES, PAYLOAD_SCHEMA, engine_block,
    error_row,
};

use crate::correlate::Comparison;
use crate::discovery::{DocumentStatus, SnapshotDiscovery};
use crate::evaluate::{DocumentInput, Finding};

use super::analysis::{self, document_input};
use super::documents::{PairedDocument, document_result, paired_documents};
use super::identity::{controls, evaluation};
use super::summary::summary_counts;
use super::{Built, Setup};

/// Constructs the complete report for a local commit-pair run with no
/// external controls: canonical payload, envelope, wire bytes, digest, and
/// the process result.
///
/// # Errors
/// Returns an internal error if the report cannot be serialized or hashed.
pub fn construct(
    setup: &Setup,
    base: &SnapshotDiscovery,
    candidate: &SnapshotDiscovery,
    comparisons: Vec<Comparison>,
    claims: &[crate::claim::ClaimOutcome],
) -> Result<Built, crate::Error> {
    construct_with_site(
        setup,
        base,
        candidate,
        comparisons,
        &crate::semantic::SiteEvaluation::default(),
        claims,
        &[],
    )
}

pub(crate) fn construct_with_site(
    setup: &Setup,
    base: &SnapshotDiscovery,
    candidate: &SnapshotDiscovery,
    comparisons: Vec<Comparison>,
    site: &crate::semantic::SiteEvaluation,
    claims: &[crate::claim::ClaimOutcome],
    projections: &[crate::projection::Outcome],
) -> Result<Built, crate::Error> {
    let paired = paired_documents(base, candidate);
    let governed = governed_seeds(candidate, claims, projections);
    let (findings, exception_errors) = match evaluate_paired(
        setup,
        &paired,
        &governed,
        &comparisons,
        site,
        claims,
        projections,
    ) {
        Ok(evaluated) => evaluated,
        Err(defect) => {
            return construct_incomplete(setup, &[crate::pipeline::detail(&defect, None)]);
        }
    };

    if let Some(crossing) = findings_ceiling_crossing(setup, &findings) {
        let mut details = logical_error_set(&governed, &exception_errors);
        details.push(crossing);
        return construct_incomplete(setup, &details);
    }

    let document_rows: Vec<_> = paired.iter().map(document_result).collect();
    let error_details = logical_error_set(&governed, &exception_errors);
    if error_details.len() > error_ceiling(setup) {
        return construct_incomplete(setup, &error_details);
    }
    let errors: Vec<_> = error_details.iter().map(error_row).collect();
    let (complete, status, exit_code) = run_result(&findings, &errors);
    let feedback = analysis::feedback(complete, &findings, &comparisons)?;
    let finding_count = u64::try_from(findings.len()).unwrap_or(u64::MAX);
    let summary = summary_counts(&paired, &comparisons, &findings, claims, true, 0);
    let candidate_start = comparisons.partition_point(|comparison| comparison.candidate.is_none());
    let comparison_rows = comparisons
        .into_iter()
        .map(|comparison| {
            let primary = comparison
                .candidate
                .as_ref()
                .or(comparison.base.as_ref())
                .map(|observation| observation.id);
            analysis::comparison(comparison).map(|row| (primary, row))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let (base_only_rows, candidate_rows) = comparison_rows.split_at(candidate_start);
    let finding_rows = findings
        .into_iter()
        .map(|mut finding| {
            if finding.candidate_fact.is_none() {
                finding.candidate_fact = analysis::nonreference_fact(
                    &finding,
                    [base_only_rows, candidate_rows],
                    &document_rows,
                )?;
            }
            Ok(analysis::finding(finding))
        })
        .collect::<Result<_, crate::Error>>()?;

    let payload = model::ReportPayload {
        schema: model::ReportPayloadSchema::Current,
        compatibility: model::ReportCompatibility::One,
        engine: engine_block(&setup.engine).map_err(|_defect| crate::Error::Internal)?,
        evaluation: model::Evaluation::Resolved(Box::new(evaluation(setup))),
        controls: controls(setup)?,
        result: model::ReportResult {
            complete,
            status,
            exit_code,
            finding_count,
            error_count: u64::try_from(errors.len()).unwrap_or(u64::MAX),
        },
        feedback,
        summary,
        documents: document_rows,
        observations: comparison_rows.into_iter().map(|(_, row)| row).collect(),
        findings: finding_rows,
        errors,
    };
    let mut payload_length = 0;
    let payload_digest = hj_serde(PAYLOAD_SCHEMA, |writer| {
        let mut counter = countio::Counter::new(writer);
        serde_json_canonicalizer::to_writer(&payload, &mut counter)?;
        payload_length = u64::try_from(counter.writer_bytes()).unwrap_or(u64::MAX);
        Ok(())
    })
    .map_err(|_defect| crate::Error::Internal)?;
    let built = Built {
        envelope: model::ReportEnvelope {
            schema: model::ReportEnvelopeSchema::Current,
            payload,
            payload_digest,
        },
        payload_digest,
        status,
        exit_code,
    };
    output_gate(setup, error_details, payload_length, built)
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
) -> Result<Built, crate::Error> {
    let envelope_shell = model::ReportEnvelope {
        schema: model::ReportEnvelopeSchema::Current,
        payload: (),
        payload_digest: built.payload_digest,
    };
    let mut counter = countio::Counter::new(std::io::sink());
    serde_json::to_writer(&mut counter, &envelope_shell)
        .map_err(|_defect| crate::Error::Internal)?;
    let wire_length = u64::try_from(counter.writer_bytes())
        .unwrap_or(u64::MAX)
        .saturating_sub(4)
        .saturating_add(payload_length)
        .saturating_add(1);
    if wire_length <= MACHINE_JSON_BYTES {
        return Ok(built);
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

/// The evaluation step of construction: the paired documents projected to
/// evaluator inputs, the governed seeds, and the complete findings with their
/// exception errors.
fn evaluate_paired(
    setup: &Setup,
    paired: &[PairedDocument<'_>],
    governed: &[crate::evaluate::GovernedSeed],
    comparisons: &[Comparison],
    site: &crate::semantic::SiteEvaluation,
    claims: &[crate::claim::ClaimOutcome],
    projections: &[crate::projection::Outcome],
) -> Result<(Vec<Finding>, Vec<ErrorDetail>), crate::Error> {
    let inputs: Vec<DocumentInput> = paired.iter().map(document_input).collect();
    let groups = crate::evaluate::claim_groups(claims);
    crate::evaluate::evaluate_with_site(
        &inputs,
        comparisons,
        setup.profile,
        &setup.policy,
        crate::evaluate::GovernedInputs {
            site,
            governed,
            claims: &groups,
            projections,
        },
    )
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
fn run_result(
    findings: &[Finding],
    governed_errors: &[model::AnalysisError<RepoPath>],
) -> (bool, model::ReportStatus, u8) {
    if !governed_errors.is_empty() {
        return (false, model::ReportStatus::Incomplete, 2);
    }
    let failing = findings
        .iter()
        .any(|finding| finding.effective_disposition == Disposition::Fail);
    if failing {
        (true, model::ReportStatus::Fail, 1)
    } else {
        (true, model::ReportStatus::Pass, 0)
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
    projections: &[crate::projection::Outcome],
) -> Vec<crate::evaluate::GovernedSeed> {
    let mut answered: std::collections::BTreeMap<
        RepoPath,
        std::collections::BTreeSet<(usize, usize)>,
    > = std::collections::BTreeMap::new();
    for outcome in claims {
        answered
            .entry(outcome.document.clone())
            .or_default()
            .insert(outcome.span);
    }
    for outcome in projections {
        answered
            .entry(RepoPath::from(&outcome.assertion.document))
            .or_default()
            .extend(outcome.answered_spans.iter().copied());
    }
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
                    || !answered
                        .get(&record.path)
                        .is_some_and(|spans| spans.contains(&governed.span))
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

/// The fatal-incomplete report for a run whose evaluation identity resolved
/// but whose analysis raised typed errors: resolved evaluation and controls,
/// cleared detail arrays, zeroed inexact summary, every error row retained in
/// canonical order, and exit class two.
///
/// # Errors
/// Returns an internal error if the report cannot be serialized or hashed.
pub fn construct_incomplete(setup: &Setup, details: &[ErrorDetail]) -> Result<Built, crate::Error> {
    let retained = retained_details(details, error_ceiling(setup));
    let errors: Vec<_> = retained.iter().map(error_row).collect();
    let error_count = u64::try_from(errors.len()).unwrap_or(u64::MAX);
    let payload = model::ReportPayload {
        schema: model::ReportPayloadSchema::Current,
        compatibility: model::ReportCompatibility::One,
        engine: engine_block(&setup.engine).map_err(|_defect| crate::Error::Internal)?,
        evaluation: model::Evaluation::Resolved(Box::new(evaluation(setup))),
        controls: controls(setup)?,
        result: model::ReportResult {
            complete: false,
            status: model::ReportStatus::Incomplete,
            exit_code: 2,
            finding_count: 0,
            error_count,
        },
        feedback: analysis::feedback(false, &[], &[])?,
        summary: summary_counts(&[], &[], &[], &[], false, error_count),
        documents: Vec::new(),
        observations: Vec::new(),
        findings: Vec::new(),
        errors,
    };
    let payload_digest = hj_serde(PAYLOAD_SCHEMA, |writer| {
        serde_json::to_writer(writer, &payload)
    })
    .map_err(|_defect| crate::Error::Internal)?;
    Ok(Built {
        envelope: model::ReportEnvelope {
            schema: model::ReportEnvelopeSchema::Current,
            payload,
            payload_digest,
        },
        payload_digest,
        status: model::ReportStatus::Incomplete,
        exit_code: 2,
    })
}
