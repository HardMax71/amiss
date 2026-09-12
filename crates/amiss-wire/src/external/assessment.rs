use std::collections::BTreeMap;

use garde::Validate;
use serde::Serialize;

use crate::codec::{self, Document, Envelope, Schema};
use crate::de::ErrorKind;
use crate::digest::Digest;
use crate::json::Value;

use super::document::{DestinationRow, Engine, Plan, RepositoryShape};
use super::evidence::{Evidence, EvidenceRow, Method, Producer, Repository, Tail};
use super::{ASSESSMENT_ENVELOPE_SCHEMA, ASSESSMENT_PAYLOAD_SCHEMA, EVIDENCE_SCHEMA};

/// Why a plan and evidence yield no assessment: the first defect found.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AssessDefect {
    #[error("the input is not an external plan envelope")]
    NotAPlan,
    #[error("the plan payload does not match its recorded digest")]
    PlanDigestMismatch,
    #[error("the input is not an external evidence file")]
    NotEvidence,
    #[error(
        "the evidence binds another plan, repeats a destination, names one the plan did not introduce, or resolves a tail the plan's shape does not carry"
    )]
    UnboundEvidence,
    #[error("an evidence row breaks its own kind's grammar")]
    MalformedEvidence,
}

#[derive(Serialize, Validate)]
#[garde(allow_unvalidated)]
struct Assessment<'a> {
    schema: Schema<Self>,
    engine: Engine,
    subject: Subject,
    producer: Producer,
    verdicts: Vec<JudgmentRow<'a>>,
}

impl Document for Assessment<'_> {
    const PAYLOAD_SCHEMA: &'static str = ASSESSMENT_PAYLOAD_SCHEMA;
    const ENVELOPE_SCHEMA: &'static str = ASSESSMENT_ENVELOPE_SCHEMA;
    const LIMIT: u64 = u64::MAX;
}

#[derive(Serialize)]
struct Subject {
    #[serde(rename = "report_payload_digest")]
    report: Digest,
    #[serde(rename = "plan_payload_digest")]
    plan: Digest,
    #[serde(rename = "evidence_digest")]
    evidence: Digest,
}

#[derive(Serialize)]
struct JudgmentRow<'a> {
    destination: &'a str,
    documents: &'a [String],
    verdict: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    retarget: Option<&'a str>,
}

pub(super) fn checked_plan(value: &Value) -> Result<Envelope<Plan>, AssessDefect> {
    Envelope::from_value(value).map_err(|defect| {
        if defect.kind == ErrorKind::DigestMismatch {
            AssessDefect::PlanDigestMismatch
        } else {
            AssessDefect::NotAPlan
        }
    })
}

/// Judges every introduced destination under the fixed external evidence policy.
///
/// # Errors
///
/// The plan or evidence violates its shape, digest binding, or domain constraints.
pub fn assess(
    plan: &Value,
    evidence: &Value,
    engine_version: &str,
    engine_digest: &str,
) -> Result<Value, AssessDefect> {
    let plan = checked_plan(plan)?;
    let source = evidence;
    let evidence: Evidence = codec::from_value("$", source).map_err(|defect| {
        if defect.path.starts_with("$.rows") {
            AssessDefect::MalformedEvidence
        } else if defect.path == "$.plan_payload_digest" {
            AssessDefect::UnboundEvidence
        } else {
            AssessDefect::NotEvidence
        }
    })?;
    evidence
        .check_header()
        .map_err(|_defect| AssessDefect::NotEvidence)?;
    if evidence.plan_payload_digest != plan.payload_digest {
        return Err(AssessDefect::UnboundEvidence);
    }
    let introduced = plan
        .payload
        .introduced
        .iter()
        .map(|row| (row.destination.as_str(), row.repository.as_ref()))
        .collect();
    let observed = observed_rows(&evidence.rows, &introduced)?;
    let assessment = Assessment {
        schema: Schema::default(),
        engine: Engine::new(engine_version, engine_digest)
            .map_err(|_defect| AssessDefect::NotAPlan)?,
        subject: Subject {
            report: plan.payload.report.payload_digest,
            plan: plan.payload_digest,
            evidence: codec::digest(EVIDENCE_SCHEMA, source)
                .map_err(|_defect| AssessDefect::MalformedEvidence)?,
        },
        producer: evidence.producer.clone(),
        verdicts: verdict_rows(&plan.payload.introduced, &observed),
    };
    codec::seal_value(&assessment).map_err(|_defect| AssessDefect::MalformedEvidence)
}

fn observed_rows<'e>(
    rows: &'e [EvidenceRow],
    introduced: &BTreeMap<&str, Option<&RepositoryShape>>,
) -> Result<BTreeMap<&'e str, &'e EvidenceRow>, AssessDefect> {
    let mut observed = BTreeMap::new();
    for (index, row) in rows.iter().enumerate() {
        let destination = row.destination();
        let shape = introduced
            .get(destination)
            .copied()
            .ok_or(AssessDefect::UnboundEvidence)?;
        row.check(&format!("$.rows[{index}]"))
            .map_err(|_defect| AssessDefect::MalformedEvidence)?;
        if let EvidenceRow::ForgeApi(forge) = row {
            let shape = shape.ok_or(AssessDefect::UnboundEvidence)?;
            if forge.tail.is_some() && shape.tail.is_none() {
                return Err(AssessDefect::UnboundEvidence);
            }
        }
        if observed.insert(destination, row).is_some() {
            return Err(AssessDefect::UnboundEvidence);
        }
    }
    Ok(observed)
}

fn verdict_rows<'a>(
    introduced: &'a [DestinationRow],
    observed: &BTreeMap<&str, &'a EvidenceRow>,
) -> Vec<JudgmentRow<'a>> {
    introduced
        .iter()
        .map(|row| {
            let (verdict, reason, retarget) = observed
                .get(row.destination.as_str())
                .map_or(("unproven", Some("unexamined"), None), |seen| {
                    judge(seen, row.repository.as_ref())
                });
            JudgmentRow {
                destination: &row.destination,
                documents: &row.documents,
                verdict,
                reason,
                retarget,
            }
        })
        .collect()
}

fn judge<'a>(
    observed: &'a EvidenceRow,
    shape: Option<&RepositoryShape>,
) -> (&'static str, Option<&'static str>, Option<&'a str>) {
    match observed {
        EvidenceRow::HttpProbe(probe) => {
            let (verdict, reason) = match probe.status {
                Some(404 | 410) if probe.method == Method::Get => ("refuted", Some("gone")),
                Some(404 | 410) => ("unproven", Some("unconfirmed")),
                Some(200..=299) => ("reachable", None),
                Some(300..=399) => ("unproven", Some("unfollowed")),
                Some(401 | 403 | 999) => ("unproven", Some("denied")),
                Some(429) => ("unproven", Some("rate-limited")),
                None | Some(_) => ("unproven", Some("unavailable")),
            };
            let retarget = probe
                .final_destination
                .as_deref()
                .filter(|_destination| probe.redirect_chain_permanent == Some(true));
            (verdict, reason, retarget)
        }
        EvidenceRow::ForgeApi(forge) => match (forge.repository, forge.tail) {
            (Repository::Missing, _) => ("unproven", Some("repository-unseen"), None),
            (Repository::Denied, _) => ("unproven", Some("denied"), None),
            (Repository::Readable, Some(Tail::Resolved)) => ("reachable", None, None),
            (Repository::Readable, Some(Tail::PathMissing)) => {
                ("refuted", Some("path-missing"), None)
            }
            (Repository::Readable, Some(Tail::RevisionMissing)) => {
                ("refuted", Some("revision-missing"), None)
            }
            (Repository::Readable, None) => {
                if shape.is_some_and(|shape| shape.tail.is_some()) {
                    ("unproven", Some("unconfirmed"), None)
                } else {
                    ("reachable", None, None)
                }
            }
        },
    }
}
