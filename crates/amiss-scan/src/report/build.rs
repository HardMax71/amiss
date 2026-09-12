use amiss_wire::controls::ResourceName;
use amiss_wire::digest::{Digest, hj_with_length};
use amiss_wire::json::{Value, canonical_length};
use amiss_wire::model::RepoPath;
use amiss_wire::report::{
    AnalysisErrorCode, COMPATIBILITY, Disposition, ErrorDetail, MACHINE_JSON_BYTES, PAYLOAD_SCHEMA,
    Summary, engine_block, error_row_value,
};
use amiss_wire::{codec, de::Error};
use serde::Serialize;

use crate::correlate::Comparison;
use crate::discovery::{DocumentStatus, SnapshotDiscovery};
use crate::evaluate::{DocumentInput, Finding};

use super::analysis::{
    ComparisonRow, FeedbackProjection, FindingProjection, comparison_value, document_input,
    feedback_value, finding_value,
};
use super::documents::{DocumentResult, PairedDocument, document_result_value, paired_documents};
use super::identity::{controls_value, evaluation_value};
use super::summary::{summary_counts, zero_counts};
use super::{Built, ENVELOPE_SCHEMA, Setup};

/// Constructs the complete report for a local commit-pair run with no
/// external controls: canonical payload, envelope, wire bytes, digest, and
/// the process result.
///
/// # Errors
///
/// The report cannot be represented in the strict JSON profile.
pub fn construct(
    setup: &Setup,
    base: &SnapshotDiscovery,
    candidate: &SnapshotDiscovery,
    comparisons: Vec<Comparison>,
    claims: &[crate::claim::ClaimOutcome],
) -> Result<Built, Error> {
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

#[expect(
    clippy::needless_pass_by_value,
    reason = "report construction owns the completed run observations"
)]
pub(crate) fn construct_with_site(
    setup: &Setup,
    base: &SnapshotDiscovery,
    candidate: &SnapshotDiscovery,
    comparisons: Vec<Comparison>,
    site: &crate::semantic::SiteEvaluation,
    claims: &[crate::claim::ClaimOutcome],
    projections: &[crate::projection::Outcome],
) -> Result<Built, Error> {
    let paired = paired_documents(base, candidate);
    let (governed, findings, exception_errors) = evaluate_paired(
        setup,
        &paired,
        candidate,
        &comparisons,
        site,
        claims,
        projections,
    );

    if let Some(crossing) = findings_ceiling_crossing(setup, &findings) {
        let mut details = logical_error_set(&governed, &exception_errors);
        details.push(crossing);
        return construct_incomplete(setup, &details);
    }

    let document_rows: Vec<(&RepoPath, DocumentResult<'_>)> = paired
        .iter()
        .map(|pair| (&pair.path, document_result_value(pair)))
        .collect();
    let error_details = logical_error_set(&governed, &exception_errors);
    if error_details.len() > error_ceiling(setup) {
        return construct_incomplete(setup, &error_details);
    }
    let governed_errors: Vec<Value> = error_details
        .iter()
        .map(error_row_value)
        .collect::<Result<_, _>>()?;
    let (complete, status, exit_code) = run_result(&findings, &governed_errors);
    let feedback = feedback_value(complete, &findings, &comparisons);
    let finding_count = u64::try_from(findings.len()).unwrap_or(u64::MAX);
    let mut counts = summary_counts(&paired, &comparisons, &findings, finding_count);
    let (governed_claims, unattested_claims) = claim_counters(claims);
    counts.governed_claims = governed_claims;
    counts.unattested_claims = unattested_claims;
    let (candidate_start, comparison_rows) = report_comparisons(&comparisons)?;
    let (base_only_rows, candidate_rows) = comparison_rows.split_at(candidate_start);
    let comparison_runs = [base_only_rows, candidate_rows];
    let finding_rows: Vec<FindingProjection<'_>> = findings
        .iter()
        .map(|finding| finding_value(finding, comparison_runs, &document_rows))
        .collect::<Result<_, _>>()?;

    let payload = codec::to_value(&Payload {
        compatibility: COMPATIBILITY,
        controls: controls_value(setup)?,
        documents: document_rows.iter().map(|(_, row)| row).collect(),
        engine: engine_block(&setup.engine)?,
        errors: governed_errors,
        evaluation: evaluation_value(setup)?,
        feedback,
        findings: finding_rows,
        observations: comparison_rows.iter().map(|(_, row)| row).collect(),
        result: RunResult {
            complete,
            error_count: u64::try_from(error_details.len()).unwrap_or(u64::MAX),
            exit_code,
            finding_count,
            status,
        },
        schema: PAYLOAD_SCHEMA,
        summary: counts,
    })?;
    let (payload_digest, payload_length) = hj_with_length(PAYLOAD_SCHEMA, &payload);
    let envelope = codec::envelope_value(ENVELOPE_SCHEMA, payload, payload_digest)?;
    let built = Built {
        envelope,
        payload_digest,
        status,
        exit_code,
    };
    output_gate(setup, error_details, payload_length, built)
}

#[derive(Serialize)]
struct RunResult {
    complete: bool,
    error_count: u64,
    exit_code: i64,
    finding_count: u64,
    status: &'static str,
}

#[derive(Serialize)]
struct Payload<'a, 'b> {
    compatibility: &'static str,
    controls: Value,
    documents: Vec<&'a DocumentResult<'b>>,
    engine: Value,
    errors: Vec<Value>,
    evaluation: Value,
    feedback: FeedbackProjection,
    findings: Vec<FindingProjection<'a>>,
    observations: Vec<&'a ComparisonRow<'b>>,
    result: RunResult,
    schema: &'static str,
    summary: Summary,
}

type ComparisonRows<'a> = Vec<(Option<Digest>, ComparisonRow<'a>)>;

fn report_comparisons(comparisons: &[Comparison]) -> Result<(usize, ComparisonRows<'_>), Error> {
    let candidate_start = comparisons.partition_point(|comparison| comparison.candidate.is_none());
    let rows = comparisons
        .iter()
        .map(|comparison| {
            let primary = comparison
                .candidate
                .as_ref()
                .or(comparison.base.as_ref())
                .map(|observation| observation.id);
            Ok((primary, comparison_value(comparison)?))
        })
        .collect::<Result<_, Error>>()?;
    Ok((candidate_start, rows))
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
) -> Result<Built, Error> {
    let envelope_shell = codec::envelope_value(ENVELOPE_SCHEMA, Value::Null, built.payload_digest)?;
    let wire_length = canonical_length(&envelope_shell)
        .saturating_sub(canonical_length(&Value::Null))
        .saturating_add(payload_length)
        .saturating_add(1);
    debug_assert_eq!(
        wire_length,
        canonical_length(&built.envelope).saturating_add(1)
    );
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
    site: &crate::semantic::SiteEvaluation,
    claims: &[crate::claim::ClaimOutcome],
    projections: &[crate::projection::Outcome],
) -> (
    Vec<crate::evaluate::GovernedSeed>,
    Vec<Finding>,
    Vec<ErrorDetail>,
) {
    let inputs: Vec<DocumentInput> = paired.iter().map(document_input).collect();
    let governed = governed_seeds(candidate, claims, projections);
    let groups = crate::evaluate::claim_groups(claims);
    let (findings, exception_errors) = crate::evaluate::evaluate_with_site(
        &inputs,
        comparisons,
        setup.profile,
        &setup.policy,
        crate::evaluate::GovernedInputs {
            site,
            governed: &governed,
            claims: &groups,
            projections,
        },
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

/// The two summary claim counters: evaluated claims, and the defective
/// subset that did not attest.
fn claim_counters(claims: &[crate::claim::ClaimOutcome]) -> (u64, u64) {
    let unattested = claims
        .iter()
        .filter(|outcome| outcome.verdict != crate::claim::ClaimVerdict::Attested)
        .count();
    (
        u64::try_from(claims.len()).unwrap_or(u64::MAX),
        u64::try_from(unattested).unwrap_or(u64::MAX),
    )
}

/// The fatal-incomplete report for a run whose evaluation identity resolved
/// but whose analysis raised typed errors: resolved evaluation and controls,
/// cleared detail arrays, zeroed inexact summary, every error row retained in
/// canonical order, and exit class two.
///
/// # Errors
///
/// The report cannot be represented in the strict JSON profile.
pub fn construct_incomplete(setup: &Setup, details: &[ErrorDetail]) -> Result<Built, Error> {
    let retained = retained_details(details, error_ceiling(setup));
    let error_rows: Vec<Value> = retained
        .iter()
        .map(error_row_value)
        .collect::<Result<_, _>>()?;
    let error_count = u64::try_from(error_rows.len()).unwrap_or(u64::MAX);
    let counts = zero_counts(error_count);

    let payload = codec::to_value(&Payload {
        compatibility: COMPATIBILITY,
        controls: controls_value(setup)?,
        documents: Vec::new(),
        engine: engine_block(&setup.engine)?,
        errors: error_rows,
        evaluation: evaluation_value(setup)?,
        feedback: feedback_value(false, &[], &[]),
        findings: Vec::new(),
        observations: Vec::new(),
        result: RunResult {
            complete: false,
            error_count,
            exit_code: 2,
            finding_count: 0,
            status: "incomplete",
        },
        schema: PAYLOAD_SCHEMA,
        summary: counts,
    })?;
    let (payload_digest, _) = hj_with_length(PAYLOAD_SCHEMA, &payload);
    let envelope = codec::envelope_value(ENVELOPE_SCHEMA, payload, payload_digest)?;
    Ok(Built {
        envelope,
        payload_digest,
        status: "incomplete",
        exit_code: 2,
    })
}
