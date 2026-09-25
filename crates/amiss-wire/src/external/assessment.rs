use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

use crate::de::{Document, Error, ErrorKind};
use crate::envelope::{Envelope, Payload, Sealing, transcoded_digest};
use crate::model::Digest;

use super::evidence::{
    EvidenceDefect, ExternalEvidence, ExternalEvidenceProducer, ExternalEvidenceRow,
    ForgeRepository, ForgeTail, ProbeMethod,
};
use super::plan::{ExternalDestination, ExternalEngine, ExternalPlan, ExternalRepository};
use super::{ASSESSMENT_PAYLOAD_SCHEMA, EVIDENCE_SCHEMA, EXTERNAL_DOCUMENT_BYTES};

/// Why a plan and evidence could not yield an assessment.
#[derive(Debug, thiserror::Error)]
pub enum AssessDefect {
    #[error("external plan is invalid: {0}")]
    Plan(#[from] Error),
    #[error("external evidence is invalid: {0}")]
    Evidence(#[from] EvidenceDefect),
    #[error("the evidence binds plan {bound}, not this plan {plan}")]
    ForeignPlan { bound: Digest, plan: Digest },
    #[error("evidence row {row} names {destination}, which the plan did not introduce")]
    UnplannedDestination { row: usize, destination: String },
    #[error("evidence row {row} repeats {destination}, which an earlier row already answered")]
    RepeatedDestination { row: usize, destination: String },
    #[error(
        "evidence row {row} answers {destination} through a forge API, but the plan introduced it as a plain URL"
    )]
    NotAForgeDestination { row: usize, destination: String },
    #[error(
        "evidence row {row} resolves a tail of {destination}, but the plan's shape for it carries none"
    )]
    UnplannedTail { row: usize, destination: String },
    #[error(transparent)]
    Assessment(#[from] AssessmentDefect),
}

#[derive(
    Default,
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Display,
    EnumString,
    SerializeDisplay,
    DeserializeFromStr,
)]
pub enum ExternalAssessmentEnvelopeSchema {
    #[default]
    #[strum(serialize = "amiss/external-assessment-envelope")]
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

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum ExternalAssessmentPayloadSchema {
    #[strum(serialize = "amiss/external-assessment-payload")]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<ExternalReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(length(chars, 1..=16_384))]
    pub retarget: Option<String>,
}

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
#[strum(serialize_all = "lowercase")]
pub enum ExternalVerdict {
    Reachable,
    Refuted,
    Unproven,
}

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
    strum::AsRefStr,
)]
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
    #[error("external assessment violates its contract: {0:?}")]
    Contract(wary::Report),
}

impl From<Error> for AssessmentDefect {
    fn from(defect: Error) -> Self {
        Self::Wire(defect)
    }
}

impl Payload for ExternalAssessment {
    type Schema = ExternalAssessmentEnvelopeSchema;
    type Defect = AssessmentDefect;
    const DOMAIN: &'static str = ASSESSMENT_PAYLOAD_SCHEMA;
    const DOCUMENT_BYTES: u64 = EXTERNAL_DOCUMENT_BYTES;
    const SEALING: Sealing = Sealing::Received;

    fn validate(&self) -> Result<(), AssessmentDefect> {
        wary::Validate::validate(self, &()).map_err(AssessmentDefect::Contract)
    }
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
    let plan = ExternalPlan::parse(plan)?;
    let evidence = ExternalEvidence::parse(evidence_bytes)?;
    let evidence_digest = transcoded_digest(EVIDENCE_SCHEMA, evidence_bytes)
        .ok_or_else(|| EvidenceDefect::Wire(Error::new("$", ErrorKind::InvalidValue)))?;
    if evidence.plan_payload_digest != plan.payload_digest {
        return Err(AssessDefect::ForeignPlan {
            bound: evidence.plan_payload_digest,
            plan: plan.payload_digest,
        });
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
    payload.emit().map_err(AssessDefect::Assessment)
}

fn bound_rows<'e>(
    plan: &Envelope<ExternalPlan>,
    evidence: &'e ExternalEvidence,
) -> Result<BTreeMap<&'e str, &'e ExternalEvidenceRow>, AssessDefect> {
    let introduced: BTreeMap<&str, &ExternalDestination> = plan
        .payload
        .introduced
        .iter()
        .map(|row| (row.destination.as_str(), row))
        .collect();
    let mut observed = BTreeMap::new();
    for (index, row) in evidence.rows.iter().enumerate() {
        let destination = match row {
            ExternalEvidenceRow::HttpProbe { destination, .. }
            | ExternalEvidenceRow::ForgeApi { destination, .. } => destination.as_str(),
        };
        let (row_number, named) = (index.saturating_add(1), destination.to_owned());
        let Some(planned) = introduced.get(destination) else {
            return Err(AssessDefect::UnplannedDestination {
                row: row_number,
                destination: named,
            });
        };
        if let ExternalEvidenceRow::ForgeApi { tail, .. } = row {
            let Some(repository) = planned.repository.as_ref() else {
                return Err(AssessDefect::NotAForgeDestination {
                    row: row_number,
                    destination: named,
                });
            };
            if tail.is_some() && repository.tail.is_none() {
                return Err(AssessDefect::UnplannedTail {
                    row: row_number,
                    destination: named,
                });
            }
        }
        if observed.insert(destination, row).is_some() {
            return Err(AssessDefect::RepeatedDestination {
                row: row_number,
                destination: named,
            });
        }
    }
    Ok(observed)
}

fn verdict_rows(
    plan: &Envelope<ExternalPlan>,
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
