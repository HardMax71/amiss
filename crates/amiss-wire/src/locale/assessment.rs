use strum::{AsRefStr, EnumString};

use crate::assessment::{AssessmentVerdict, bindings_value, decode_bindings};
use crate::controls::{decode_enum, value};
use crate::de::{self, Error, ErrorKind, Obj, fail};
use crate::digest::Digest;
use crate::json::Value;

use super::evidence::{
    EVIDENCE_DOCUMENT_BYTES, LocaleCoverageEvidence, LocaleCoverageEvidenceEnvelope,
    evidence as build_evidence,
};
use super::{
    LocaleCoveragePlan, LocaleCoveragePlanEnvelope, LocalePageRequirement, PAGE_ITEMS_LIMIT,
    PAGE_KEY_BYTES, plan as build_plan,
};

pub const ASSESSMENT_ENVELOPE_SCHEMA: &str = "amiss/locale-coverage-assessment-envelope";
pub const ASSESSMENT_PAYLOAD_SCHEMA: &str = "amiss/locale-coverage-assessment-payload";
pub const ASSESSMENT_DOCUMENT_BYTES: u64 = EVIDENCE_DOCUMENT_BYTES;
pub const ASSESSMENT_PAGE_ITEMS_LIMIT: usize = 200_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, AsRefStr, EnumString)]
#[strum(serialize_all = "kebab-case")]
pub enum LocaleCoverageReason {
    EvidenceAbsent,
    EvidenceUnbound,
    ProducerMismatch,
    DocsMismatch,
    ScopeMismatch,
    SourceIncomplete,
    TargetIncomplete,
    SourceMissing,
    TargetMissing,
    TargetOrphaned,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocaleCoverageAssessmentEnvelope {
    pub payload: LocaleCoverageAssessment,
    pub payload_digest: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocaleCoverageAssessment {
    pub engine_version: String,
    pub engine_digest: Digest,
    pub report_payload_digest: Digest,
    pub plan_payload_digest: Digest,
    pub evidence_payload_digest: Option<Digest>,
    pub verdict: AssessmentVerdict,
    pub reasons: Vec<LocaleCoverageReason>,
    pub coverage: LocaleCoverageResult,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocaleCoverageResult {
    pub complete: bool,
    pub source_missing: Vec<String>,
    pub target_missing: Vec<String>,
    pub target_orphaned: Vec<String>,
}

struct CoverageOutcome {
    verdict: AssessmentVerdict,
    reasons: Vec<LocaleCoverageReason>,
    coverage: LocaleCoverageResult,
}

/// Parses one closed, digest-bound offline locale coverage assessment.
///
/// # Errors
///
/// Fails on oversized or malformed strict JSON, an unknown field, an invalid engine identity,
/// unsorted or repeated rows, an inconsistent verdict, or a payload digest mismatch.
pub fn parse_assessment(bytes: &[u8]) -> Result<LocaleCoverageAssessmentEnvelope, Error> {
    let (payload, payload_digest) = crate::bounded_envelope::parse(
        bytes,
        ASSESSMENT_ENVELOPE_SCHEMA,
        ASSESSMENT_PAYLOAD_SCHEMA,
        ASSESSMENT_DOCUMENT_BYTES,
        decode_assessment,
    )?;
    Ok(LocaleCoverageAssessmentEnvelope {
        payload,
        payload_digest,
    })
}

/// Judges one locale coverage plan against optional producer-normalized page inventories.
///
/// Every reported page is proved by explicit presence on one side and complete absence on the
/// other. A partial inventory can therefore refute a plan, but it cannot manufacture an absence.
/// A matched result is emitted only when the policy-scoped comparison is exhaustive.
///
/// # Errors
///
/// Fails when either typed envelope no longer reproduces its own digest, a public field violates
/// its source contract, the result exceeds its resource budget, or the engine version is invalid.
pub fn assess(
    plan: &LocaleCoveragePlanEnvelope,
    evidence: Option<&LocaleCoverageEvidenceEnvelope>,
    engine_version: &str,
    engine_digest: Digest,
) -> Result<Value, Error> {
    let rebuilt_plan = build_plan(&plan.payload)?;
    if rebuilt_plan.text("payload_digest") != Some(&plan.payload_digest.to_string()) {
        return fail("$.plan.payload_digest", ErrorKind::DigestMismatch);
    }
    if let Some(evidence) = evidence {
        let rebuilt_evidence = build_evidence(&evidence.payload)?;
        if rebuilt_evidence.text("payload_digest") != Some(&evidence.payload_digest.to_string()) {
            return fail("$.evidence.payload_digest", ErrorKind::DigestMismatch);
        }
    }

    let unavailable = || LocaleCoverageResult {
        complete: false,
        source_missing: Vec::new(),
        target_missing: Vec::new(),
        target_orphaned: Vec::new(),
    };
    let outcome = match evidence {
        None => CoverageOutcome {
            verdict: AssessmentVerdict::Unproven,
            reasons: vec![LocaleCoverageReason::EvidenceAbsent],
            coverage: unavailable(),
        },
        Some(evidence) if evidence.payload.plan_payload_digest != plan.payload_digest => {
            CoverageOutcome {
                verdict: AssessmentVerdict::Unproven,
                reasons: vec![LocaleCoverageReason::EvidenceUnbound],
                coverage: unavailable(),
            }
        }
        Some(evidence) if evidence.payload.producer != plan.payload.producer => CoverageOutcome {
            verdict: AssessmentVerdict::Unproven,
            reasons: vec![LocaleCoverageReason::ProducerMismatch],
            coverage: unavailable(),
        },
        Some(evidence)
            if evidence.payload.docs != plan.payload.docs
                || evidence.payload.scope != plan.payload.scope =>
        {
            CoverageOutcome {
                verdict: AssessmentVerdict::Refuted,
                reasons: [
                    (
                        evidence.payload.docs != plan.payload.docs,
                        LocaleCoverageReason::DocsMismatch,
                    ),
                    (
                        evidence.payload.scope != plan.payload.scope,
                        LocaleCoverageReason::ScopeMismatch,
                    ),
                ]
                .into_iter()
                .filter_map(|(different, reason)| different.then_some(reason))
                .collect(),
                coverage: unavailable(),
            }
        }
        Some(evidence) => compare_coverage(&plan.payload, &evidence.payload),
    };

    let assessment = LocaleCoverageAssessment {
        engine_version: engine_version.to_owned(),
        engine_digest,
        report_payload_digest: plan.payload.report_payload_digest,
        plan_payload_digest: plan.payload_digest,
        evidence_payload_digest: evidence.map(|evidence| evidence.payload_digest),
        verdict: outcome.verdict,
        reasons: outcome.reasons,
        coverage: outcome.coverage,
    };
    let payload = assessment_value(&assessment);
    let _validated = decode_assessment("$.payload", payload.clone())?;
    crate::bounded_envelope::build(
        payload,
        ASSESSMENT_ENVELOPE_SCHEMA,
        ASSESSMENT_PAYLOAD_SCHEMA,
        ASSESSMENT_DOCUMENT_BYTES,
    )
}

fn compare_coverage(
    plan: &LocaleCoveragePlan,
    evidence: &LocaleCoverageEvidence,
) -> CoverageOutcome {
    let source_missing = match &plan.policy.required {
        LocalePageRequirement::Named(keys) if evidence.source.complete => keys
            .iter()
            .filter(|key| !evidence.source.pages.contains_key(*key))
            .cloned()
            .collect(),
        LocalePageRequirement::AllSource | LocalePageRequirement::Named(_) => Vec::new(),
    };
    let target_missing = if evidence.target.complete {
        match &plan.policy.required {
            LocalePageRequirement::AllSource => evidence
                .source
                .pages
                .keys()
                .filter(|key| !evidence.target.pages.contains_key(*key))
                .cloned()
                .collect(),
            LocalePageRequirement::Named(keys) => keys
                .iter()
                .filter(|key| {
                    evidence.source.pages.contains_key(*key)
                        && !evidence.target.pages.contains_key(*key)
                })
                .cloned()
                .collect(),
        }
    } else {
        Vec::new()
    };
    let target_orphaned = if evidence.source.complete {
        evidence
            .target
            .pages
            .keys()
            .filter(|key| !evidence.source.pages.contains_key(*key))
            .cloned()
            .collect()
    } else {
        Vec::new()
    };

    let requirement_complete = match &plan.policy.required {
        LocalePageRequirement::AllSource => evidence.source.complete,
        LocalePageRequirement::Named(keys) => {
            evidence.source.complete
                || keys
                    .iter()
                    .all(|key| evidence.source.pages.contains_key(key))
        }
    };
    let source_resolution_complete = evidence.source.complete
        || evidence
            .target
            .pages
            .keys()
            .all(|key| evidence.source.pages.contains_key(key));
    let complete = evidence.target.complete && requirement_complete && source_resolution_complete;
    let coverage = LocaleCoverageResult {
        complete,
        source_missing,
        target_missing,
        target_orphaned,
    };
    classify_coverage(
        coverage,
        !requirement_complete || !source_resolution_complete,
        !evidence.target.complete,
    )
}

fn classify_coverage(
    coverage: LocaleCoverageResult,
    source_incomplete: bool,
    target_incomplete: bool,
) -> CoverageOutcome {
    let reasons: Vec<_> = [
        (
            !coverage.source_missing.is_empty(),
            LocaleCoverageReason::SourceMissing,
        ),
        (
            !coverage.target_missing.is_empty(),
            LocaleCoverageReason::TargetMissing,
        ),
        (
            !coverage.target_orphaned.is_empty(),
            LocaleCoverageReason::TargetOrphaned,
        ),
    ]
    .into_iter()
    .filter_map(|(present, reason)| present.then_some(reason))
    .collect();
    if !reasons.is_empty() {
        CoverageOutcome {
            verdict: AssessmentVerdict::Refuted,
            reasons,
            coverage,
        }
    } else if coverage.complete {
        CoverageOutcome {
            verdict: AssessmentVerdict::Matched,
            reasons,
            coverage,
        }
    } else {
        CoverageOutcome {
            verdict: AssessmentVerdict::Unproven,
            reasons: [
                (source_incomplete, LocaleCoverageReason::SourceIncomplete),
                (target_incomplete, LocaleCoverageReason::TargetIncomplete),
            ]
            .into_iter()
            .filter_map(|(incomplete, reason)| incomplete.then_some(reason))
            .collect(),
            coverage,
        }
    }
}

fn decode_assessment(path: &str, input: Value) -> Result<LocaleCoverageAssessment, Error> {
    let mut assessment = Obj::new(path, input)?;
    assessment.required("schema", |path, value| {
        de::const_str(path, value, ASSESSMENT_PAYLOAD_SCHEMA)
    })?;
    let bindings = decode_bindings(&mut assessment)?;
    let verdict = assessment.required("verdict", decode_enum)?;
    let reasons_path = assessment.field("reasons");
    let reasons = de::sorted_items(
        &reasons_path,
        assessment.take("reasons")?,
        10,
        decode_enum,
        |reason| reason,
    )?;
    let coverage = assessment.required("coverage", decode_coverage)?;
    assessment.finish()?;
    if !valid_shape(
        verdict,
        bindings.evidence_payload_digest,
        &reasons,
        &coverage,
    ) {
        return fail(path, ErrorKind::Inconsistent);
    }
    Ok(LocaleCoverageAssessment {
        engine_version: bindings.engine_version,
        engine_digest: bindings.engine_digest,
        report_payload_digest: bindings.report_payload_digest,
        plan_payload_digest: bindings.plan_payload_digest,
        evidence_payload_digest: bindings.evidence_payload_digest,
        verdict,
        reasons,
        coverage,
    })
}

fn decode_coverage(path: &str, input: Value) -> Result<LocaleCoverageResult, Error> {
    let mut coverage = Obj::new(path, input)?;
    let complete = coverage.required("complete", de::boolean)?;
    let source_missing = coverage.required("source_missing", decode_page_keys)?;
    let target_missing = coverage.required("target_missing", decode_page_keys)?;
    let target_orphaned = coverage.required("target_orphaned", decode_page_keys)?;
    coverage.finish()?;
    source_missing
        .len()
        .checked_add(target_missing.len())
        .and_then(|total| total.checked_add(target_orphaned.len()))
        .filter(|total| *total <= ASSESSMENT_PAGE_ITEMS_LIMIT)
        .ok_or_else(|| Error::new(path, ErrorKind::LimitExceeded))?;
    Ok(LocaleCoverageResult {
        complete,
        source_missing,
        target_missing,
        target_orphaned,
    })
}

fn decode_page_keys(path: &str, value: Value) -> Result<Vec<String>, Error> {
    de::sorted_items(
        path,
        value,
        PAGE_ITEMS_LIMIT,
        |path, value| de::bounded_text(path, value, PAGE_KEY_BYTES),
        |key| key,
    )
}

fn valid_shape(
    verdict: AssessmentVerdict,
    evidence_payload_digest: Option<Digest>,
    reasons: &[LocaleCoverageReason],
    coverage: &LocaleCoverageResult,
) -> bool {
    let no_pages = coverage.source_missing.is_empty()
        && coverage.target_missing.is_empty()
        && coverage.target_orphaned.is_empty();
    let matched = verdict == AssessmentVerdict::Matched
        && evidence_payload_digest.is_some()
        && reasons.is_empty()
        && coverage.complete
        && no_pages;
    let unavailable = verdict == AssessmentVerdict::Unproven
        && !coverage.complete
        && no_pages
        && matches!(
            (evidence_payload_digest, reasons),
            (None, [LocaleCoverageReason::EvidenceAbsent])
                | (
                    Some(_),
                    [LocaleCoverageReason::EvidenceUnbound
                        | LocaleCoverageReason::ProducerMismatch]
                )
        );
    let incomplete = verdict == AssessmentVerdict::Unproven
        && evidence_payload_digest.is_some()
        && !coverage.complete
        && no_pages
        && !reasons.is_empty()
        && reasons.iter().all(|reason| {
            matches!(
                reason,
                LocaleCoverageReason::SourceIncomplete | LocaleCoverageReason::TargetIncomplete
            )
        });
    let binding_refuted = verdict == AssessmentVerdict::Refuted
        && evidence_payload_digest.is_some()
        && !coverage.complete
        && no_pages
        && !reasons.is_empty()
        && reasons.iter().all(|reason| {
            matches!(
                reason,
                LocaleCoverageReason::DocsMismatch | LocaleCoverageReason::ScopeMismatch
            )
        });
    let page_refuted = verdict == AssessmentVerdict::Refuted
        && evidence_payload_digest.is_some()
        && !no_pages
        && reasons
            == [
                (!coverage.source_missing.is_empty())
                    .then_some(LocaleCoverageReason::SourceMissing),
                (!coverage.target_missing.is_empty())
                    .then_some(LocaleCoverageReason::TargetMissing),
                (!coverage.target_orphaned.is_empty())
                    .then_some(LocaleCoverageReason::TargetOrphaned),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
    matched || unavailable || incomplete || binding_refuted || page_refuted
}

fn assessment_value(assessment: &LocaleCoverageAssessment) -> Value {
    let page_keys =
        |keys: &[String]| Value::array(keys.iter().map(|key| value::text(key)).collect());
    let (engine, subject) = bindings_value(
        &assessment.engine_version,
        assessment.engine_digest,
        assessment.report_payload_digest,
        assessment.plan_payload_digest,
        assessment.evidence_payload_digest,
    );
    value::object(vec![
        ("schema", value::text(ASSESSMENT_PAYLOAD_SCHEMA)),
        ("engine", engine),
        ("subject", subject),
        ("verdict", value::text(assessment.verdict.as_ref())),
        (
            "reasons",
            Value::array(
                assessment
                    .reasons
                    .iter()
                    .map(|reason| value::text(reason.as_ref()))
                    .collect(),
            ),
        ),
        (
            "coverage",
            value::object(vec![
                ("complete", Value::Bool(assessment.coverage.complete)),
                (
                    "source_missing",
                    page_keys(&assessment.coverage.source_missing),
                ),
                (
                    "target_missing",
                    page_keys(&assessment.coverage.target_missing),
                ),
                (
                    "target_orphaned",
                    page_keys(&assessment.coverage.target_orphaned),
                ),
            ]),
        ),
    ])
}
