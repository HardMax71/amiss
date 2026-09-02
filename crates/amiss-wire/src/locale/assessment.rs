use strum::{AsRefStr, EnumString};

use crate::assessment::{AssessmentVerdict, Nullable, bindings_value, decode_bindings};
use crate::controls::{decode_enum, value};
use crate::de::{self, Error, ErrorKind, fail};
use crate::digest::Digest;
use crate::json::Value;
use crate::model::ArtifactId;
use crate::publication::decode_identity;

use super::evidence::{
    EVIDENCE_DOCUMENT_BYTES, LocaleCoverageEvidence, LocaleCoverageEvidenceEnvelope,
    LocaleTargetOrigin, evidence_payload_digest,
};
use super::{
    LocaleCoveragePlan, LocaleCoveragePlanEnvelope, LocalePageRequirement, PAGE_ITEMS_LIMIT,
    PAGE_KEY_BYTES, plan_payload_digest,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, AsRefStr, EnumString)]
#[strum(serialize_all = "kebab-case")]
pub enum LocaleFallbackStatus {
    Allowed,
    Unauthorized,
    SourceMismatch,
    SourceUnproven,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, AsRefStr, EnumString)]
#[strum(serialize_all = "kebab-case")]
pub enum LocaleLineageStatus {
    Current,
    Stale,
    Unproven,
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
    pub product: Option<LocaleProductResult>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocaleCoverageResult {
    pub complete: bool,
    pub source_missing: Vec<String>,
    pub target_missing: Vec<String>,
    pub target_orphaned: Vec<String>,
    pub fallbacks: Vec<LocaleFallbackResult>,
    pub lineage: Vec<LocaleLineageResult>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocaleFallbackResult {
    pub key: String,
    pub class: ArtifactId,
    pub status: LocaleFallbackStatus,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocaleLineageResult {
    pub key: String,
    pub status: LocaleLineageStatus,
}

#[derive(Clone, Debug, PartialEq, Eq)]
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
) -> Result<Value, Error> {
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
        engine_version: engine_version.to_owned(),
        engine_digest,
        report_payload_digest: plan.payload.report_payload_digest,
        plan_payload_digest: plan.payload_digest,
        evidence_payload_digest: evidence.map(|evidence| evidence.payload_digest),
        verdict: outcome.verdict,
        reasons: outcome.reasons,
        coverage: outcome.coverage,
        product: outcome.product,
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

fn decode_assessment(path: &str, input: Value) -> Result<LocaleCoverageAssessment, Error> {
    de::closed_object(path, input, |assessment| {
        assessment.required("schema", |path, value| {
            de::const_str(path, value, ASSESSMENT_PAYLOAD_SCHEMA)
        })?;
        let bindings = decode_bindings(assessment)?;
        let verdict = assessment.required("verdict", decode_enum)?;
        let reasons_path = assessment.field("reasons");
        let reasons = de::sorted_items(
            &reasons_path,
            assessment.take("reasons")?,
            19,
            decode_enum,
            |reason| reason,
        )?;
        let coverage = assessment.required("coverage", |path, input| {
            de::closed_object(path, input, |coverage| {
                let complete = coverage.required("complete", de::boolean)?;
                let source_missing = coverage.required("source_missing", decode_page_keys)?;
                let target_missing = coverage.required("target_missing", decode_page_keys)?;
                let target_orphaned = coverage.required("target_orphaned", decode_page_keys)?;
                let fallbacks_path = coverage.field("fallbacks");
                let fallbacks = de::sorted_items(
                    &fallbacks_path,
                    coverage.take("fallbacks")?,
                    PAGE_ITEMS_LIMIT,
                    |path, value| {
                        de::closed_object(path, value, |fallback| {
                            let key = fallback.required("key", |path, value| {
                                de::bounded_text(path, value, PAGE_KEY_BYTES)
                            })?;
                            let class = fallback.required("class", decode_identity)?;
                            let status = fallback.required("status", decode_enum)?;
                            Ok(LocaleFallbackResult { key, class, status })
                        })
                    },
                    |fallback| fallback.key.as_str(),
                )?;
                let lineage_path = coverage.field("lineage");
                let lineage = de::sorted_items(
                    &lineage_path,
                    coverage.take("lineage")?,
                    PAGE_ITEMS_LIMIT,
                    |path, value| {
                        de::closed_object(path, value, |lineage| {
                            let key = lineage.required("key", |path, value| {
                                de::bounded_text(path, value, PAGE_KEY_BYTES)
                            })?;
                            let status = lineage.required("status", decode_enum)?;
                            Ok(LocaleLineageResult { key, status })
                        })
                    },
                    |lineage| lineage.key.as_str(),
                )?;
                source_missing
                    .len()
                    .checked_add(target_missing.len())
                    .and_then(|total| total.checked_add(target_orphaned.len()))
                    .and_then(|total| total.checked_add(fallbacks.len()))
                    .and_then(|total| total.checked_add(lineage.len()))
                    .filter(|total| *total <= ASSESSMENT_PAGE_ITEMS_LIMIT)
                    .ok_or_else(|| Error::new(path, ErrorKind::LimitExceeded))?;
                Ok(LocaleCoverageResult {
                    complete,
                    source_missing,
                    target_missing,
                    target_orphaned,
                    fallbacks,
                    lineage,
                })
            })
        })?;
        let product_path = assessment.field("product");
        let product =
            de::decode_nullable(&product_path, assessment.take("product")?, decode_product)?;
        if !valid_shape(
            verdict,
            bindings.evidence_payload_digest,
            &reasons,
            &coverage,
            product.as_ref(),
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
            product,
        })
    })
}

fn decode_product(path: &str, input: Value) -> Result<LocaleProductResult, Error> {
    de::closed_object(path, input, |product| {
        let source = product.required("source", decode_enum)?;
        let target = product.required("target", decode_enum)?;
        Ok(LocaleProductResult { source, target })
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

fn coverage_value(coverage: &LocaleCoverageResult) -> Value {
    let page_keys =
        |keys: &[String]| Value::array(keys.iter().map(|key| value::text(key)).collect());
    value::object(vec![
        ("complete", Value::Bool(coverage.complete)),
        ("source_missing", page_keys(&coverage.source_missing)),
        ("target_missing", page_keys(&coverage.target_missing)),
        ("target_orphaned", page_keys(&coverage.target_orphaned)),
        (
            "fallbacks",
            Value::array(
                coverage
                    .fallbacks
                    .iter()
                    .map(|fallback| {
                        value::object(vec![
                            ("key", value::text(&fallback.key)),
                            ("class", value::text(fallback.class.as_str())),
                            ("status", value::text(fallback.status.as_ref())),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "lineage",
            Value::array(
                coverage
                    .lineage
                    .iter()
                    .map(|lineage| {
                        value::object(vec![
                            ("key", value::text(&lineage.key)),
                            ("status", value::text(lineage.status.as_ref())),
                        ])
                    })
                    .collect(),
            ),
        ),
    ])
}

fn assessment_value(assessment: &LocaleCoverageAssessment) -> Value {
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
        ("coverage", coverage_value(&assessment.coverage)),
        (
            "product",
            assessment.product.as_ref().map_or(Value::Null, |product| {
                value::object(vec![
                    ("source", value::text(product.source.as_ref())),
                    ("target", value::text(product.target.as_ref())),
                ])
            }),
        ),
    ])
}
