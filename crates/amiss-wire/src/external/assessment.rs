use std::collections::BTreeMap;

use crate::digest::hj;
use crate::json::Value;

use super::{
    ASSESSMENT_ENVELOPE_SCHEMA, ASSESSMENT_PAYLOAD_SCHEMA, EVIDENCE_SCHEMA, PLAN_ENVELOPE_SCHEMA,
    PLAN_PAYLOAD_SCHEMA, object, string,
};

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

/// One validated evidence observation, either transport or forge facts.
enum Observed {
    Probe {
        method_get: bool,
        status: Option<i64>,
        retarget: Option<String>,
    },
    Forge {
        repository: Repository,
        tail: Option<Tail>,
    },
}

#[derive(Clone, Copy)]
enum Repository {
    Readable,
    Missing,
    Denied,
}

#[derive(Clone, Copy)]
enum Tail {
    Resolved,
    PathMissing,
    RevisionMissing,
}

/// Judges one complete external plan against one producer's evidence: every
/// introduced destination gets a verdict, missing evidence stays unproven,
/// and evidence naming anything outside the introduced set invalidates the
/// whole assessment rather than being skipped. Pure: the same plan and
/// evidence always yield the same assessment, digest included.
///
/// # Errors
///
/// Returns the first [`AssessDefect`] when the plan is not one or fails its
/// digest, the evidence is not evidence, it binds another plan or names an
/// unknown destination, or a row breaks its own kind's grammar.
pub fn assess(
    plan: &Value,
    evidence: &Value,
    engine_version: &str,
    engine_digest: &str,
) -> Result<Value, AssessDefect> {
    let (Some(payload), Some(recorded)) = (plan.member("payload"), plan.text("payload_digest"))
    else {
        return Err(AssessDefect::NotAPlan);
    };
    if plan.text("schema") != Some(PLAN_ENVELOPE_SCHEMA)
        || payload.text("schema") != Some(PLAN_PAYLOAD_SCHEMA)
    {
        return Err(AssessDefect::NotAPlan);
    }
    if hj(PLAN_PAYLOAD_SCHEMA, payload).to_string() != recorded {
        return Err(AssessDefect::PlanDigestMismatch);
    }
    let report_digest = payload
        .member("report")
        .and_then(|report| report.text("payload_digest"));
    let (Some(report_digest), Some(Value::Array(introduced))) =
        (report_digest, payload.member("introduced"))
    else {
        return Err(AssessDefect::NotAPlan);
    };

    if evidence.text("schema") != Some(EVIDENCE_SCHEMA) {
        return Err(AssessDefect::NotEvidence);
    }
    let producer = evidence
        .member("producer")
        .filter(|producer| {
            producer.text("name").is_some_and(|name| !name.is_empty())
                && producer
                    .text("version")
                    .is_some_and(|version| !version.is_empty())
        })
        .ok_or(AssessDefect::NotEvidence)?;
    if evidence.text("plan_payload_digest") != Some(recorded) {
        return Err(AssessDefect::UnboundEvidence);
    }
    let Some(Value::Array(evidence_rows)) = evidence.member("rows") else {
        return Err(AssessDefect::NotEvidence);
    };

    // The digest only proves the plan is whole; the judged fields must still
    // fit the assessment contract, so a hand-built plan cannot smuggle rows
    // the published schema would reject.
    let mut introduced_by_destination = BTreeMap::new();
    for introduced_row in introduced {
        let destination = introduced_row
            .text("destination")
            .filter(|destination| !destination.is_empty());
        let documents = matches!(
            introduced_row.member("documents"),
            Some(Value::Array(items)) if !items.is_empty()
        );
        let (Some(destination), true) = (destination, documents) else {
            return Err(AssessDefect::NotAPlan);
        };
        if introduced_by_destination
            .insert(destination, introduced_row.member("repository"))
            .is_some()
        {
            return Err(AssessDefect::NotAPlan);
        }
    }
    let observed = observed_rows(evidence_rows, &introduced_by_destination)?;
    let assessment = object(vec![
        ("schema", string(ASSESSMENT_PAYLOAD_SCHEMA)),
        (
            "engine",
            object(vec![
                ("engine_version", string(engine_version)),
                ("engine_digest", string(engine_digest)),
            ]),
        ),
        (
            "subject",
            object(vec![
                ("report_payload_digest", string(report_digest)),
                ("plan_payload_digest", string(recorded)),
                (
                    "evidence_digest",
                    string(&hj(EVIDENCE_SCHEMA, evidence).to_string()),
                ),
            ]),
        ),
        ("producer", producer.clone()),
        (
            "verdicts",
            Value::Array(verdict_rows(introduced, &observed).into_boxed_slice()),
        ),
    ]);
    let digest = hj(ASSESSMENT_PAYLOAD_SCHEMA, &assessment);
    Ok(object(vec![
        ("schema", string(ASSESSMENT_ENVELOPE_SCHEMA)),
        ("payload", assessment),
        ("payload_digest", string(&digest.to_string())),
    ]))
}

/// Every evidence row validated and keyed: a row must name an introduced
/// destination, name it once, and carry an observation instant.
fn observed_rows<'e>(
    evidence_rows: &'e [Value],
    introduced: &BTreeMap<&str, Option<&Value>>,
) -> Result<BTreeMap<&'e str, Observed>, AssessDefect> {
    let mut observed: BTreeMap<&str, Observed> = BTreeMap::new();
    for row in evidence_rows {
        let destination = row
            .text("destination")
            .ok_or(AssessDefect::MalformedEvidence)?;
        let shape = introduced
            .get(destination)
            .copied()
            .ok_or(AssessDefect::UnboundEvidence)?;
        if row.text("checked_at").is_none_or(str::is_empty) {
            return Err(AssessDefect::MalformedEvidence);
        }
        let observation = observe(row, shape)?;
        if observed.insert(destination, observation).is_some() {
            return Err(AssessDefect::UnboundEvidence);
        }
    }
    Ok(observed)
}

/// One verdict row per introduced destination, in the plan's own order.
fn verdict_rows(introduced: &[Value], observed: &BTreeMap<&str, Observed>) -> Vec<Value> {
    introduced
        .iter()
        .map(|row| {
            let destination = row.text("destination").unwrap_or_default();
            let mut members = vec![
                ("destination", string(destination)),
                (
                    "documents",
                    row.member("documents").cloned().unwrap_or(Value::Null),
                ),
            ];
            let (verdict, reason, retarget) = match observed.get(destination) {
                None => ("unproven", Some("unexamined"), None),
                Some(seen) => judge(seen, row.member("repository")),
            };
            members.push(("verdict", string(verdict)));
            if let Some(reason) = reason {
                members.push(("reason", string(reason)));
            }
            if let Some(retarget) = retarget {
                members.push(("retarget", string(&retarget)));
            }
            object(members)
        })
        .collect()
}

/// One evidence row into its validated observation; forge facts are only
/// admissible for a destination the plan shaped, and a tail resolution only
/// where the shape carries a tail to resolve.
fn observe(row: &Value, shape: Option<&Value>) -> Result<Observed, AssessDefect> {
    match row.text("kind") {
        Some("http-probe") => {
            let method_get = match row.text("method") {
                Some("get") => true,
                Some("head") => false,
                Some(_) | None => return Err(AssessDefect::MalformedEvidence),
            };
            let status = row.member("status");
            let failure = row.member("failure");
            let status = match (status, failure) {
                (Some(Value::Integer(status)), None | Some(Value::Null))
                    if (100..=999).contains(status) =>
                {
                    Some(*status)
                }
                (None | Some(Value::Null), Some(Value::String(failure)))
                    if matches!(failure.as_ref(), "dns" | "tls" | "timeout" | "refused") =>
                {
                    None
                }
                (_, _) => return Err(AssessDefect::MalformedEvidence),
            };
            let final_destination = match row.member("final_destination") {
                Some(Value::String(destination)) if !destination.is_empty() => {
                    Some(destination.as_ref())
                }
                None | Some(Value::Null) => None,
                Some(_) => return Err(AssessDefect::MalformedEvidence),
            };
            let redirect_chain_permanent = match row.member("redirect_chain_permanent") {
                Some(Value::Bool(true)) if final_destination.is_some() => true,
                None | Some(Value::Null) => false,
                Some(_) => return Err(AssessDefect::MalformedEvidence),
            };
            Ok(Observed::Probe {
                method_get,
                status,
                retarget: final_destination
                    .filter(|_destination| redirect_chain_permanent)
                    .map(str::to_owned),
            })
        }
        Some("forge-api") => {
            let Some(shape) = shape else {
                return Err(AssessDefect::UnboundEvidence);
            };
            let repository = match row.text("repository") {
                Some("readable") => Repository::Readable,
                Some("missing") => Repository::Missing,
                Some("denied") => Repository::Denied,
                Some(_) | None => return Err(AssessDefect::MalformedEvidence),
            };
            let tail = match (repository, row.member("tail")) {
                (_, None | Some(Value::Null)) => None,
                (Repository::Readable, Some(Value::String(tail))) => match tail.as_ref() {
                    "resolved" => Some(Tail::Resolved),
                    "path-missing" => Some(Tail::PathMissing),
                    "revision-missing" => Some(Tail::RevisionMissing),
                    _ => return Err(AssessDefect::MalformedEvidence),
                },
                (_, Some(_)) => return Err(AssessDefect::MalformedEvidence),
            };
            if tail.is_some() && shape.member("tail").is_none() {
                return Err(AssessDefect::UnboundEvidence);
            }
            Ok(Observed::Forge { repository, tail })
        }
        Some(_) | None => Err(AssessDefect::MalformedEvidence),
    }
}

/// The fixed judgment policy: denial and rate limits are never death, a 404
/// counts only when a GET confirmed it, a missing repository may be a
/// private one, and a path is absent only after the repository and revision
/// resolved.
fn judge(
    observed: &Observed,
    shape: Option<&Value>,
) -> (&'static str, Option<&'static str>, Option<String>) {
    match observed {
        Observed::Probe {
            method_get,
            status,
            retarget,
        } => {
            let (verdict, reason) = match status {
                Some(404 | 410) if *method_get => ("refuted", Some("gone")),
                Some(404 | 410) => ("unproven", Some("unconfirmed")),
                Some(200..=299) => ("reachable", None),
                Some(300..=399) => ("unproven", Some("unfollowed")),
                Some(401 | 403 | 999) => ("unproven", Some("denied")),
                Some(429) => ("unproven", Some("rate-limited")),
                None | Some(_) => ("unproven", Some("unavailable")),
            };
            (verdict, reason, retarget.clone())
        }
        Observed::Forge { repository, tail } => match (repository, tail) {
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
                let unresolved = shape.is_some_and(|shape| shape.member("tail").is_some());
                if unresolved {
                    ("unproven", Some("unconfirmed"), None)
                } else {
                    ("reachable", None, None)
                }
            }
        },
    }
}
