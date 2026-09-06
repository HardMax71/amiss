use std::cmp::Ordering;

use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

use crate::assessment::{AssessmentEngine, AssessmentSubject, AssessmentVerdict, Nullable};
use crate::de::{self, Error, ErrorKind, fail};
use crate::digest::{Digest, hb};
use crate::json;
use crate::model::ArtifactId;
use crate::semantic::producer_version_valid;

use super::evidence::{
    EVIDENCE_DOCUMENT_BYTES, LocaleCoverageEvidence, LocaleCoverageEvidenceEnvelope,
    LocaleTargetOrigin, evidence_payload_digest,
};
use super::{
    LocaleCoveragePlan, LocaleCoveragePlanEnvelope, LocalePageRequirement, plan_payload_digest,
    validate_page_keys,
};

pub const ASSESSMENT_ENVELOPE_SCHEMA: &str = "amiss/locale-coverage-assessment-envelope";
pub const ASSESSMENT_PAYLOAD_SCHEMA: &str = "amiss/locale-coverage-assessment-payload";
pub const ASSESSMENT_DOCUMENT_BYTES: u64 = EVIDENCE_DOCUMENT_BYTES;
pub const ASSESSMENT_PAGE_ITEMS_LIMIT: usize = 200_000;

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Display,
    EnumString,
    SerializeDisplay,
    DeserializeFromStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum LocaleCoverageReason {
    EvidenceAbsent,
    EvidenceUnbound,
    ProducerMismatch,
    DocsMismatch,
    ScopeMismatch,
    SourceIncomplete,
    TargetIncomplete,
    FallbackUnproven,
    LineageUnproven,
    SourceProductUnproven,
    TargetProductUnproven,
    SourceMissing,
    TargetMissing,
    TargetOrphaned,
    FallbackUnauthorized,
    FallbackSourceMismatch,
    LineageStale,
    SourceProductMismatch,
    TargetProductMismatch,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum LocaleFallbackStatus {
    Allowed,
    Unauthorized,
    SourceMismatch,
    SourceUnproven,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum LocaleLineageStatus {
    Current,
    Stale,
    Unproven,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocaleCoverageAssessmentEnvelope {
    pub schema: AssessmentEnvelopeSchema,
    pub payload: LocaleCoverageAssessment,
    pub payload_digest: Digest,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum AssessmentEnvelopeSchema {
    #[strum(serialize = "amiss/locale-coverage-assessment-envelope")]
    Current,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocaleCoverageAssessment {
    pub schema: AssessmentPayloadSchema,
    pub engine: AssessmentEngine,
    pub subject: AssessmentSubject,
    pub verdict: AssessmentVerdict,
    pub reasons: Vec<LocaleCoverageReason>,
    pub coverage: LocaleCoverageResult,
    pub product: Nullable<LocaleProductResult>,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum AssessmentPayloadSchema {
    #[strum(serialize = "amiss/locale-coverage-assessment-payload")]
    Current,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocaleCoverageResult {
    pub complete: bool,
    pub source_missing: Vec<String>,
    pub target_missing: Vec<String>,
    pub target_orphaned: Vec<String>,
    pub fallbacks: Vec<LocaleFallbackResult>,
    pub lineage: Vec<LocaleLineageResult>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocaleFallbackResult {
    pub key: String,
    pub class: ArtifactId,
    pub status: LocaleFallbackStatus,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocaleLineageResult {
    pub key: String,
    pub status: LocaleLineageStatus,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocaleProductResult {
    pub source: AssessmentVerdict,
    pub target: AssessmentVerdict,
}

struct AssessmentOutcome {
    verdict: AssessmentVerdict,
    reasons: Vec<LocaleCoverageReason>,
    coverage: LocaleCoverageResult,
    product: Option<LocaleProductResult>,
}

/// Parses one closed, digest-bound offline locale coverage assessment.
///
/// # Errors
///
/// Fails on oversized or malformed strict JSON, an unknown field, an invalid engine identity,
/// unsorted or repeated rows, an inconsistent verdict, or a payload digest mismatch.
pub fn parse_assessment(bytes: &[u8]) -> Result<LocaleCoverageAssessmentEnvelope, Error> {
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > ASSESSMENT_DOCUMENT_BYTES {
        return fail("$", ErrorKind::LimitExceeded);
    }
    json::parse(bytes).map_err(|defect| Error::new("$", ErrorKind::Json(defect)))?;
    let document: LocaleCoverageAssessmentEnvelope = de::deserialize_json(bytes)?;
    if assessment_payload_digest(&document.payload)? != document.payload_digest {
        return fail("$.payload_digest", ErrorKind::DigestMismatch);
    }
    Ok(document)
}

/// Judges one locale coverage plan against optional producer-normalized page inventories.
///
/// Every reported page is proved by explicit presence on one side and complete absence on the
/// other. A partial inventory can therefore refute a plan, but it cannot manufacture an absence.
/// Every fallback also binds its exact source resource and must match one plan-owned class/page
/// rule. Required target lineage compares only an explicit based-on digest with the observed
/// current source resource. A selected product compares each inventory's independently observed
/// publication resource with the exact plan resource. A matched result is emitted only when every
/// selected comparison is exhaustive.
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
) -> Result<Vec<u8>, Error> {
    if plan_payload_digest(&plan.payload)? != plan.payload_digest {
        return fail("$.plan.payload_digest", ErrorKind::DigestMismatch);
    }
    if let Some(evidence) = evidence
        && evidence_payload_digest(&evidence.payload)? != evidence.payload_digest
    {
        return fail("$.evidence.payload_digest", ErrorKind::DigestMismatch);
    }

    let unavailable = || LocaleCoverageResult {
        complete: false,
        source_missing: Vec::new(),
        target_missing: Vec::new(),
        target_orphaned: Vec::new(),
        fallbacks: Vec::new(),
        lineage: Vec::new(),
    };
    let outcome = match evidence {
        None => AssessmentOutcome {
            verdict: AssessmentVerdict::Unproven,
            reasons: vec![LocaleCoverageReason::EvidenceAbsent],
            coverage: unavailable(),
            product: None,
        },
        Some(evidence) if evidence.payload.plan_payload_digest != plan.payload_digest => {
            AssessmentOutcome {
                verdict: AssessmentVerdict::Unproven,
                reasons: vec![LocaleCoverageReason::EvidenceUnbound],
                coverage: unavailable(),
                product: None,
            }
        }
        Some(evidence) if evidence.payload.producer != plan.payload.producer => AssessmentOutcome {
            verdict: AssessmentVerdict::Unproven,
            reasons: vec![LocaleCoverageReason::ProducerMismatch],
            coverage: unavailable(),
            product: None,
        },
        Some(evidence)
            if evidence.payload.docs != plan.payload.docs
                || evidence.payload.scope != plan.payload.scope =>
        {
            AssessmentOutcome {
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
                product: None,
            }
        }
        Some(evidence) => compare_coverage(&plan.payload, &evidence.payload),
    };

    let assessment = LocaleCoverageAssessment {
        schema: AssessmentPayloadSchema::Current,
        engine: AssessmentEngine {
            engine_version: engine_version.to_owned(),
            engine_digest,
        },
        subject: AssessmentSubject {
            report_payload_digest: plan.payload.report_payload_digest,
            plan_payload_digest: plan.payload_digest,
            evidence_payload_digest: evidence.map_or(Nullable::Null, |evidence| {
                Nullable::Value(evidence.payload_digest)
            }),
        },
        verdict: outcome.verdict,
        reasons: outcome.reasons,
        coverage: outcome.coverage,
        product: outcome.product.map_or(Nullable::Null, Nullable::Value),
    };
    let payload_digest = assessment_payload_digest(&assessment)?;
    let document = LocaleCoverageAssessmentEnvelope {
        schema: AssessmentEnvelopeSchema::Current,
        payload: assessment,
        payload_digest,
    };
    let canonical = serde_json_canonicalizer::to_vec(&document)
        .map_err(|_defect| Error::new("$", ErrorKind::InvalidValue))?;
    if u64::try_from(canonical.len()).unwrap_or(u64::MAX) > ASSESSMENT_DOCUMENT_BYTES {
        return fail("$", ErrorKind::LimitExceeded);
    }
    json::parse(&canonical).map_err(|defect| Error::new("$", ErrorKind::Json(defect)))?;
    Ok(canonical)
}

fn compare_coverage(
    plan: &LocaleCoveragePlan,
    evidence: &LocaleCoverageEvidence,
) -> AssessmentOutcome {
    let source_pages = &evidence.source.pages;
    let target_pages = &evidence.target.pages;
    let source_has = |key: &str| {
        source_pages
            .binary_search_by(|page| page.key.as_str().cmp(key))
            .is_ok()
    };
    let target_has = |key: &str| {
        target_pages
            .binary_search_by(|page| page.key.as_str().cmp(key))
            .is_ok()
    };
    let source_missing = match &plan.policy.required {
        LocalePageRequirement::Named { keys } if evidence.source.complete => keys
            .iter()
            .filter(|key| !source_has(key))
            .cloned()
            .collect(),
        LocalePageRequirement::AllSource | LocalePageRequirement::Named { .. } => Vec::new(),
    };
    let target_missing = if evidence.target.complete {
        match &plan.policy.required {
            LocalePageRequirement::AllSource => source_pages
                .iter()
                .filter(|page| !target_has(&page.key))
                .map(|page| page.key.clone())
                .collect(),
            LocalePageRequirement::Named { keys } => keys
                .iter()
                .filter(|key| source_has(key) && !target_has(key))
                .cloned()
                .collect(),
        }
    } else {
        Vec::new()
    };
    let target_orphaned = if evidence.source.complete {
        target_pages
            .iter()
            .filter(|page| !source_has(&page.key))
            .map(|page| page.key.clone())
            .collect()
    } else {
        Vec::new()
    };
    let TargetRelations { fallbacks, lineage } = compare_target_relations(plan, evidence);

    let requirement_complete = match &plan.policy.required {
        LocalePageRequirement::AllSource => evidence.source.complete,
        LocalePageRequirement::Named { keys } => {
            evidence.source.complete || keys.iter().all(|key| source_has(key))
        }
    };
    let source_resolution_complete =
        evidence.source.complete || target_pages.iter().all(|page| source_has(&page.key));
    let fallback_complete = fallbacks
        .iter()
        .all(|fallback| fallback.status != LocaleFallbackStatus::SourceUnproven);
    let lineage_complete = lineage
        .iter()
        .all(|lineage| lineage.status != LocaleLineageStatus::Unproven);
    let complete = evidence.target.complete
        && requirement_complete
        && source_resolution_complete
        && fallback_complete
        && lineage_complete;
    let coverage = LocaleCoverageResult {
        complete,
        source_missing,
        target_missing,
        target_orphaned,
        fallbacks,
        lineage,
    };
    let product = match &plan.product {
        Nullable::Value(expected) => {
            let compare =
                |observed: &Nullable<crate::publication::PublicationResource>| match observed {
                    Nullable::Value(actual) if actual == expected => AssessmentVerdict::Matched,
                    Nullable::Value(_) => AssessmentVerdict::Refuted,
                    Nullable::Null => AssessmentVerdict::Unproven,
                };
            Some(LocaleProductResult {
                source: compare(&evidence.source.product),
                target: compare(&evidence.target.product),
            })
        }
        Nullable::Null => None,
    };
    classify_coverage(
        coverage,
        product,
        !requirement_complete || !source_resolution_complete,
        !evidence.target.complete,
    )
}

struct TargetRelations {
    fallbacks: Vec<LocaleFallbackResult>,
    lineage: Vec<LocaleLineageResult>,
}

struct CoverageState {
    no_structural_pages: bool,
    no_pages: bool,
    fallback: FallbackState,
    lineage: LineageState,
}

struct FallbackState {
    unproven: bool,
    unauthorized: bool,
    source_mismatch: bool,
}

struct LineageState {
    unproven: bool,
    stale: bool,
}

fn coverage_state(coverage: &LocaleCoverageResult) -> CoverageState {
    let mut fallback = FallbackState {
        unproven: false,
        unauthorized: false,
        source_mismatch: false,
    };
    for row in &coverage.fallbacks {
        fallback.unproven |= row.status == LocaleFallbackStatus::SourceUnproven;
        fallback.unauthorized |= row.status == LocaleFallbackStatus::Unauthorized;
        fallback.source_mismatch |= row.status == LocaleFallbackStatus::SourceMismatch;
    }
    let mut lineage = LineageState {
        unproven: false,
        stale: false,
    };
    for row in &coverage.lineage {
        lineage.unproven |= row.status == LocaleLineageStatus::Unproven;
        lineage.stale |= row.status == LocaleLineageStatus::Stale;
    }
    let no_structural_pages = coverage.source_missing.is_empty()
        && coverage.target_missing.is_empty()
        && coverage.target_orphaned.is_empty();
    CoverageState {
        no_structural_pages,
        no_pages: no_structural_pages
            && coverage.fallbacks.is_empty()
            && coverage.lineage.is_empty(),
        fallback,
        lineage,
    }
}

fn refuted_reasons(
    coverage: &LocaleCoverageResult,
    state: &CoverageState,
    product: Option<&LocaleProductResult>,
) -> Vec<LocaleCoverageReason> {
    [
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
        (
            state.fallback.unauthorized,
            LocaleCoverageReason::FallbackUnauthorized,
        ),
        (
            state.fallback.source_mismatch,
            LocaleCoverageReason::FallbackSourceMismatch,
        ),
        (state.lineage.stale, LocaleCoverageReason::LineageStale),
        (
            product.is_some_and(|product| product.source == AssessmentVerdict::Refuted),
            LocaleCoverageReason::SourceProductMismatch,
        ),
        (
            product.is_some_and(|product| product.target == AssessmentVerdict::Refuted),
            LocaleCoverageReason::TargetProductMismatch,
        ),
    ]
    .into_iter()
    .filter_map(|(present, reason)| present.then_some(reason))
    .collect()
}

fn compare_target_relations(
    plan: &LocaleCoveragePlan,
    evidence: &LocaleCoverageEvidence,
) -> TargetRelations {
    let mut fallbacks = Vec::new();
    let mut lineage = Vec::new();
    let source_page = |key: &String| {
        evidence
            .source
            .pages
            .binary_search_by(|page| page.key.cmp(key))
            .ok()
            .and_then(|index| evidence.source.pages.get(index))
    };
    for page in &evidence.target.pages {
        let key = &page.key;
        match &page.origin {
            LocaleTargetOrigin::Fallback {
                class,
                source_resource_digest,
            } => {
                let authorized = plan.policy.fallbacks.iter().any(|rule| {
                    rule.class == *class
                        && match &rule.pages {
                            LocalePageRequirement::AllSource => true,
                            LocalePageRequirement::Named { keys } => {
                                keys.binary_search(key).is_ok()
                            }
                        }
                });
                let status = if authorized {
                    match source_page(key) {
                        Some(current) if current.resource_digest == *source_resource_digest => {
                            LocaleFallbackStatus::Allowed
                        }
                        Some(_) => LocaleFallbackStatus::SourceMismatch,
                        None if evidence.source.complete => LocaleFallbackStatus::SourceMismatch,
                        None => LocaleFallbackStatus::SourceUnproven,
                    }
                } else {
                    LocaleFallbackStatus::Unauthorized
                };
                fallbacks.push(LocaleFallbackResult {
                    key: key.clone(),
                    class: class.clone(),
                    status,
                });
            }
            LocaleTargetOrigin::TargetResource {
                based_on_source_digest,
            } if plan.policy.require_target_lineage => {
                if let Some(current_source) = source_page(key) {
                    let status = match based_on_source_digest {
                        Nullable::Value(based_on)
                            if *based_on == current_source.resource_digest =>
                        {
                            LocaleLineageStatus::Current
                        }
                        Nullable::Value(_) => LocaleLineageStatus::Stale,
                        Nullable::Null => LocaleLineageStatus::Unproven,
                    };
                    lineage.push(LocaleLineageResult {
                        key: key.clone(),
                        status,
                    });
                }
            }
            LocaleTargetOrigin::TargetResource { .. } => {}
        }
    }
    TargetRelations { fallbacks, lineage }
}

fn classify_coverage(
    coverage: LocaleCoverageResult,
    product: Option<LocaleProductResult>,
    source_incomplete: bool,
    target_incomplete: bool,
) -> AssessmentOutcome {
    let state = coverage_state(&coverage);
    let reasons = refuted_reasons(&coverage, &state, product.as_ref());
    if !reasons.is_empty() {
        AssessmentOutcome {
            verdict: AssessmentVerdict::Refuted,
            reasons,
            coverage,
            product,
        }
    } else if coverage.complete
        && product.as_ref().is_none_or(|product| {
            product.source == AssessmentVerdict::Matched
                && product.target == AssessmentVerdict::Matched
        })
    {
        AssessmentOutcome {
            verdict: AssessmentVerdict::Matched,
            reasons,
            coverage,
            product,
        }
    } else {
        AssessmentOutcome {
            verdict: AssessmentVerdict::Unproven,
            reasons: [
                (source_incomplete, LocaleCoverageReason::SourceIncomplete),
                (target_incomplete, LocaleCoverageReason::TargetIncomplete),
                (
                    state.fallback.unproven,
                    LocaleCoverageReason::FallbackUnproven,
                ),
                (
                    state.lineage.unproven,
                    LocaleCoverageReason::LineageUnproven,
                ),
                (
                    product
                        .as_ref()
                        .is_some_and(|product| product.source == AssessmentVerdict::Unproven),
                    LocaleCoverageReason::SourceProductUnproven,
                ),
                (
                    product
                        .as_ref()
                        .is_some_and(|product| product.target == AssessmentVerdict::Unproven),
                    LocaleCoverageReason::TargetProductUnproven,
                ),
            ]
            .into_iter()
            .filter_map(|(incomplete, reason)| incomplete.then_some(reason))
            .collect(),
            coverage,
            product,
        }
    }
}

fn assessment_payload_digest(assessment: &LocaleCoverageAssessment) -> Result<Digest, Error> {
    validate_assessment(assessment)?;
    serde_json_canonicalizer::to_vec(assessment)
        .map(|canonical| hb(ASSESSMENT_PAYLOAD_SCHEMA, &canonical))
        .map_err(|_defect| Error::new("$.payload", ErrorKind::InvalidValue))
}

fn validate_assessment(assessment: &LocaleCoverageAssessment) -> Result<(), Error> {
    producer_version_valid(&assessment.engine.engine_version)
        .then_some(())
        .ok_or_else(|| Error::new("$.payload.engine.engine_version", ErrorKind::InvalidValue))?;
    (assessment.reasons.len() <= 8)
        .then_some(())
        .ok_or_else(|| Error::new("$.payload.reasons", ErrorKind::LimitExceeded))?;
    assessment
        .reasons
        .iter()
        .zip(assessment.reasons.iter().skip(1))
        .try_for_each(|(previous, current)| match previous.cmp(current) {
            Ordering::Less => Ok(()),
            Ordering::Equal => fail("$.payload.reasons", ErrorKind::DuplicateMember),
            Ordering::Greater => fail("$.payload.reasons", ErrorKind::UnsortedSet),
        })?;

    let coverage = &assessment.coverage;
    validate_page_keys(
        "$.payload.coverage.source_missing",
        coverage.source_missing.iter().map(String::as_str),
        "",
    )?;
    validate_page_keys(
        "$.payload.coverage.target_missing",
        coverage.target_missing.iter().map(String::as_str),
        "",
    )?;
    validate_page_keys(
        "$.payload.coverage.target_orphaned",
        coverage.target_orphaned.iter().map(String::as_str),
        "",
    )?;
    validate_page_keys(
        "$.payload.coverage.fallbacks",
        coverage.fallbacks.iter().map(|row| row.key.as_str()),
        ".key",
    )?;
    validate_page_keys(
        "$.payload.coverage.lineage",
        coverage.lineage.iter().map(|row| row.key.as_str()),
        ".key",
    )?;
    coverage
        .source_missing
        .len()
        .checked_add(coverage.target_missing.len())
        .and_then(|total| total.checked_add(coverage.target_orphaned.len()))
        .and_then(|total| total.checked_add(coverage.fallbacks.len()))
        .and_then(|total| total.checked_add(coverage.lineage.len()))
        .filter(|total| *total <= ASSESSMENT_PAGE_ITEMS_LIMIT)
        .ok_or_else(|| Error::new("$.payload.coverage", ErrorKind::LimitExceeded))?;

    let evidence_payload_digest = match assessment.subject.evidence_payload_digest {
        Nullable::Value(digest) => Some(digest),
        Nullable::Null => None,
    };
    let product = match &assessment.product {
        Nullable::Value(product) => Some(product),
        Nullable::Null => None,
    };
    valid_shape(
        assessment.verdict,
        evidence_payload_digest,
        &assessment.reasons,
        coverage,
        product,
    )
    .then_some(())
    .ok_or_else(|| Error::new("$.payload", ErrorKind::Inconsistent))
}

fn valid_shape(
    verdict: AssessmentVerdict,
    evidence_payload_digest: Option<Digest>,
    reasons: &[LocaleCoverageReason],
    coverage: &LocaleCoverageResult,
    product: Option<&LocaleProductResult>,
) -> bool {
    let state = coverage_state(coverage);
    let source_product_unproven =
        product.is_some_and(|product| product.source == AssessmentVerdict::Unproven);
    let target_product_unproven =
        product.is_some_and(|product| product.target == AssessmentVerdict::Unproven);
    let exact_refutations = refuted_reasons(coverage, &state, product);
    let coverage_nonrefuting =
        !state.fallback.unauthorized && !state.fallback.source_mismatch && !state.lineage.stale;
    let product_nonrefuting = product.is_none_or(|product| {
        product.source != AssessmentVerdict::Refuted && product.target != AssessmentVerdict::Refuted
    });
    let product_matched = product.is_none_or(|product| {
        product.source == AssessmentVerdict::Matched && product.target == AssessmentVerdict::Matched
    });
    let matched = verdict == AssessmentVerdict::Matched
        && evidence_payload_digest.is_some()
        && reasons.is_empty()
        && coverage.complete
        && state.no_structural_pages
        && !state.fallback.unproven
        && !state.lineage.unproven
        && coverage_nonrefuting
        && product_matched;
    let unavailable = verdict == AssessmentVerdict::Unproven
        && !coverage.complete
        && state.no_pages
        && product.is_none()
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
        && (!coverage.complete || source_product_unproven || target_product_unproven)
        && state.no_structural_pages
        && !reasons.is_empty()
        && reasons.iter().all(|reason| {
            matches!(
                reason,
                LocaleCoverageReason::SourceIncomplete
                    | LocaleCoverageReason::TargetIncomplete
                    | LocaleCoverageReason::FallbackUnproven
                    | LocaleCoverageReason::LineageUnproven
                    | LocaleCoverageReason::SourceProductUnproven
                    | LocaleCoverageReason::TargetProductUnproven
            )
        })
        && reasons.contains(&LocaleCoverageReason::FallbackUnproven) == state.fallback.unproven
        && reasons.contains(&LocaleCoverageReason::LineageUnproven) == state.lineage.unproven
        && coverage_nonrefuting
        && reasons.contains(&LocaleCoverageReason::SourceProductUnproven)
            == source_product_unproven
        && reasons.contains(&LocaleCoverageReason::TargetProductUnproven)
            == target_product_unproven
        && product_nonrefuting;
    let binding_refuted = verdict == AssessmentVerdict::Refuted
        && evidence_payload_digest.is_some()
        && !coverage.complete
        && state.no_pages
        && product.is_none()
        && !reasons.is_empty()
        && reasons.iter().all(|reason| {
            matches!(
                reason,
                LocaleCoverageReason::DocsMismatch | LocaleCoverageReason::ScopeMismatch
            )
        });
    let relation_refuted = verdict == AssessmentVerdict::Refuted
        && evidence_payload_digest.is_some()
        && (!coverage.complete || (!state.fallback.unproven && !state.lineage.unproven))
        && !exact_refutations.is_empty()
        && reasons == exact_refutations;
    matched || unavailable || incomplete || binding_refuted || relation_refuted
}
