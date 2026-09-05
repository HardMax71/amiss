use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use wary::Validate;

use crate::de::{self, Error, ErrorKind};
use crate::digest::{Digest, hb, hj};
use crate::json;

use super::evidence::{
    EvidenceDefect, ExternalEvidence, ExternalEvidenceProducer, ExternalEvidenceRow,
    ForgeRepository, ForgeTail, ProbeMethod, parse_evidence,
};
use super::plan::{
    ExternalDestination, ExternalEngine, ExternalPlanEnvelope, ExternalRepository, parse_plan,
};
use super::{ASSESSMENT_PAYLOAD_SCHEMA, EVIDENCE_SCHEMA, EXTERNAL_DOCUMENT_BYTES};

/// Why a plan and evidence could not yield an assessment.
#[derive(Debug, thiserror::Error)]
pub enum AssessDefect {
    #[error("external plan is invalid: {0}")]
    Plan(#[from] Error),
    #[error("external evidence is invalid: {0}")]
    Evidence(#[from] EvidenceDefect),
    #[error(
        "the evidence binds another plan, repeats a destination, names one the plan did not introduce, or resolves a tail the plan's shape does not carry"
    )]
    UnboundEvidence,
    #[error(transparent)]
    Assessment(#[from] AssessmentDefect),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalAssessmentEnvelope {
    pub schema: ExternalAssessmentEnvelopeSchema,
    pub payload: ExternalAssessment,
    pub payload_digest: Digest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExternalAssessmentEnvelopeSchema {
    #[serde(rename = "amiss/external-assessment-envelope")]
    Current,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, wary::Wary)]
#[validate(func = |_, assessment: &ExternalAssessment| {
    (assessment
        .verdicts
        .iter()
        .map(|row| row.destination.as_str())
        .collect::<BTreeSet<_>>()
        .len()
        == assessment.verdicts.len())
        .then_some(())
        .ok_or_else(|| wary::Error::new("duplicate_external_destination"))
})]
pub struct ExternalAssessment {
    pub schema: ExternalAssessmentPayloadSchema,
    #[validate(dive)]
    pub engine: ExternalEngine,
    pub subject: ExternalAssessmentSubject,
    #[validate(dive)]
    pub producer: ExternalEvidenceProducer,
    #[validate(inner(dive))]
    pub verdicts: Vec<ExternalVerdictRow>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExternalAssessmentPayloadSchema {
    #[serde(rename = "amiss/external-assessment-payload")]
    Current,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalAssessmentSubject {
    pub report_payload_digest: Digest,
    pub plan_payload_digest: Digest,
    pub evidence_digest: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, wary::Wary)]
#[validate(func = |_, row: &ExternalVerdictRow| {
    let documents_are_unique =
        row.documents.iter().collect::<BTreeSet<_>>().len() == row.documents.len();
    let verdict_has_its_reason =
        (row.verdict == ExternalVerdict::Reachable) == row.reason.is_none();
    (documents_are_unique && verdict_has_its_reason)
        .then_some(())
        .ok_or_else(|| wary::Error::new("invalid_external_verdict"))
})]
pub struct ExternalVerdictRow {
    #[validate(length(chars, 1..=16_384))]
    pub destination: String,
    #[validate(length(1..), inner(length(chars, 1..)))]
    pub documents: Vec<String>,
    pub verdict: ExternalVerdict,
    #[serde(
        default,
        deserialize_with = "json_serde::deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub reason: Option<ExternalReason>,
    #[serde(
        default,
        deserialize_with = "json_serde::deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    #[validate(length(chars, 1..=16_384))]
    pub retarget: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExternalVerdict {
    Reachable,
    Refuted,
    Unproven,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, strum::AsRefStr,
)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum ExternalReason {
    Gone,
    PathMissing,
    RevisionMissing,
    Unexamined,
    Denied,
    RateLimited,
    Unavailable,
    Unconfirmed,
    Unfollowed,
    RepositoryUnseen,
}

#[derive(Debug, thiserror::Error)]
pub enum AssessmentDefect {
    #[error(transparent)]
    Wire(Error),
    #[error("external assessment violates its contract: {0}")]
    Contract(wary::Report),
}

/// Parses one strict, digest-bound external assessment. Additive fields are inert.
///
/// # Errors
///
/// Fails on an oversized or malformed strict document, a malformed known
/// field, a schema law reported by the derived validator, or a digest mismatch.
pub fn parse_assessment(bytes: &[u8]) -> Result<ExternalAssessmentEnvelope, AssessmentDefect> {
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > EXTERNAL_DOCUMENT_BYTES {
        return Err(AssessmentDefect::Wire(Error::new(
            "$",
            ErrorKind::LimitExceeded,
        )));
    }
    let payload_digest = {
        let strict = json::parse(bytes)
            .map_err(|defect| AssessmentDefect::Wire(Error::new("$", ErrorKind::Json(defect))))?;
        let payload = strict.member("payload").ok_or_else(|| {
            AssessmentDefect::Wire(Error::new("$.payload", ErrorKind::MissingField))
        })?;
        hj(ASSESSMENT_PAYLOAD_SCHEMA, payload)
    };
    let document: ExternalAssessmentEnvelope =
        de::deserialize_json(bytes).map_err(AssessmentDefect::Wire)?;
    document
        .payload
        .validate(&())
        .map_err(AssessmentDefect::Contract)?;
    if payload_digest != document.payload_digest {
        return Err(AssessmentDefect::Wire(Error::new(
            "$.payload_digest",
            ErrorKind::DigestMismatch,
        )));
    }
    Ok(document)
}

/// Judges one complete external plan against one producer's evidence.
///
/// Every introduced destination gets a verdict in plan order, missing
/// evidence stays unproven, and evidence outside the plan invalidates the
/// complete assessment. The same inputs always produce the same output.
///
/// # Errors
///
/// Fails when either input violates its typed contract, the evidence is not
/// bound one-to-one to introduced destinations, or the result cannot satisfy
/// the assessment contract.
pub fn assess(
    plan: &[u8],
    evidence_bytes: &[u8],
    engine_version: &str,
    engine_digest: Digest,
) -> Result<Vec<u8>, AssessDefect> {
    let plan = parse_plan(plan)?;
    let evidence = parse_evidence(evidence_bytes)?;
    let evidence_digest = hj(
        EVIDENCE_SCHEMA,
        &json::parse(evidence_bytes)
            .map_err(|defect| EvidenceDefect::Wire(Error::new("$", ErrorKind::Json(defect))))?,
    );
    if evidence.plan_payload_digest != plan.payload_digest {
        return Err(AssessDefect::UnboundEvidence);
    }
    let observed = bound_rows(&plan, &evidence)?;
    let verdicts = verdict_rows(&plan, &observed);
    let payload = ExternalAssessment {
        schema: ExternalAssessmentPayloadSchema::Current,
        engine: ExternalEngine {
            engine_version: engine_version.to_owned(),
            engine_digest,
        },
        subject: ExternalAssessmentSubject {
            report_payload_digest: plan.payload.report.payload_digest,
            plan_payload_digest: plan.payload_digest,
            evidence_digest,
        },
        producer: evidence.producer,
        verdicts,
    };
    let payload_digest = assessment_payload_digest(&payload)?;
    let document = ExternalAssessmentEnvelope {
        schema: ExternalAssessmentEnvelopeSchema::Current,
        payload,
        payload_digest,
    };
    let canonical = serde_json_canonicalizer::to_vec(&document)
        .map_err(|_defect| AssessmentDefect::Wire(Error::new("$", ErrorKind::InvalidValue)))?;
    if u64::try_from(canonical.len()).unwrap_or(u64::MAX) > EXTERNAL_DOCUMENT_BYTES {
        return Err(AssessmentDefect::Wire(Error::new("$", ErrorKind::LimitExceeded)).into());
    }
    json::parse(&canonical)
        .map_err(|defect| AssessmentDefect::Wire(Error::new("$", ErrorKind::Json(defect))))?;
    Ok(canonical)
}

fn assessment_payload_digest(assessment: &ExternalAssessment) -> Result<Digest, AssessmentDefect> {
    assessment
        .validate(&())
        .map_err(AssessmentDefect::Contract)?;
    serde_json_canonicalizer::to_vec(assessment)
        .map(|canonical| hb(ASSESSMENT_PAYLOAD_SCHEMA, &canonical))
        .map_err(|_defect| AssessmentDefect::Wire(Error::new("$.payload", ErrorKind::InvalidValue)))
}

fn bound_rows<'e>(
    plan: &ExternalPlanEnvelope,
    evidence: &'e ExternalEvidence,
) -> Result<BTreeMap<&'e str, &'e ExternalEvidenceRow>, AssessDefect> {
    let introduced: BTreeMap<&str, &ExternalDestination> = plan
        .payload
        .introduced
        .iter()
        .map(|row| (row.destination.as_str(), row))
        .collect();
    let mut observed = BTreeMap::new();
    for row in &evidence.rows {
        let destination = match row {
            ExternalEvidenceRow::HttpProbe { destination, .. }
            | ExternalEvidenceRow::ForgeApi { destination, .. } => destination.as_str(),
        };
        let planned = introduced
            .get(destination)
            .ok_or(AssessDefect::UnboundEvidence)?;
        if let ExternalEvidenceRow::ForgeApi { tail, .. } = row {
            let repository = planned
                .repository
                .as_ref()
                .ok_or(AssessDefect::UnboundEvidence)?;
            if tail.is_some() && repository.tail.is_none() {
                return Err(AssessDefect::UnboundEvidence);
            }
        }
        if observed.insert(destination, row).is_some() {
            return Err(AssessDefect::UnboundEvidence);
        }
    }
    Ok(observed)
}

fn verdict_rows(
    plan: &ExternalPlanEnvelope,
    observed: &BTreeMap<&str, &ExternalEvidenceRow>,
) -> Vec<ExternalVerdictRow> {
    plan.payload
        .introduced
        .iter()
        .map(|planned| {
            let (verdict, reason, retarget) = judge(
                observed.get(planned.destination.as_str()).copied(),
                planned.repository.as_ref(),
            );
            ExternalVerdictRow {
                destination: planned.destination.clone(),
                documents: planned.documents.clone(),
                verdict,
                reason,
                retarget,
            }
        })
        .collect()
}

/// Denial and rate limits are never death, a 404 counts only after GET, and
/// a forge refutes only below a repository it established was readable.
fn judge(
    observed: Option<&ExternalEvidenceRow>,
    shape: Option<&ExternalRepository>,
) -> (ExternalVerdict, Option<ExternalReason>, Option<String>) {
    match observed {
        None => (
            ExternalVerdict::Unproven,
            Some(ExternalReason::Unexamined),
            None,
        ),
        Some(ExternalEvidenceRow::HttpProbe {
            method,
            status,
            final_destination,
            redirect_chain_permanent,
            ..
        }) => {
            let (verdict, reason) = match status {
                Some(404 | 410) if *method == ProbeMethod::Get => {
                    (ExternalVerdict::Refuted, Some(ExternalReason::Gone))
                }
                Some(404 | 410) => (ExternalVerdict::Unproven, Some(ExternalReason::Unconfirmed)),
                Some(200..=299) => (ExternalVerdict::Reachable, None),
                Some(300..=399) => (ExternalVerdict::Unproven, Some(ExternalReason::Unfollowed)),
                Some(401 | 403 | 999) => (ExternalVerdict::Unproven, Some(ExternalReason::Denied)),
                Some(429) => (ExternalVerdict::Unproven, Some(ExternalReason::RateLimited)),
                None | Some(_) => (ExternalVerdict::Unproven, Some(ExternalReason::Unavailable)),
            };
            let retarget = redirect_chain_permanent
                .is_some_and(|permanent| permanent)
                .then(|| final_destination.clone())
                .flatten();
            (verdict, reason, retarget)
        }
        Some(ExternalEvidenceRow::ForgeApi {
            repository, tail, ..
        }) => match (repository, tail) {
            (ForgeRepository::Missing, None) => (
                ExternalVerdict::Unproven,
                Some(ExternalReason::RepositoryUnseen),
                None,
            ),
            (ForgeRepository::Denied, None) => (
                ExternalVerdict::Unproven,
                Some(ExternalReason::Denied),
                None,
            ),
            (ForgeRepository::Readable, Some(ForgeTail::Resolved)) => {
                (ExternalVerdict::Reachable, None, None)
            }
            (ForgeRepository::Readable, Some(ForgeTail::PathMissing)) => (
                ExternalVerdict::Refuted,
                Some(ExternalReason::PathMissing),
                None,
            ),
            (ForgeRepository::Readable, Some(ForgeTail::RevisionMissing)) => (
                ExternalVerdict::Refuted,
                Some(ExternalReason::RevisionMissing),
                None,
            ),
            (ForgeRepository::Readable, None)
                if shape.is_some_and(|shape| shape.tail.is_some()) =>
            {
                (
                    ExternalVerdict::Unproven,
                    Some(ExternalReason::Unconfirmed),
                    None,
                )
            }
            (ForgeRepository::Readable, None) => (ExternalVerdict::Reachable, None, None),
            (ForgeRepository::Missing | ForgeRepository::Denied, Some(_)) => (
                ExternalVerdict::Unproven,
                Some(ExternalReason::Unavailable),
                None,
            ),
        },
    }
}
