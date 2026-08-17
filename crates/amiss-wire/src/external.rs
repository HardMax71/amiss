use std::collections::{BTreeMap, BTreeSet};

use crate::digest::hj;
use crate::json::Value;
use crate::model::ForgeDialect;
use crate::report::{ENVELOPE_SCHEMA, PAYLOAD_SCHEMA};

pub const PLAN_ENVELOPE_SCHEMA: &str = "amiss/external-plan-envelope";
pub const PLAN_PAYLOAD_SCHEMA: &str = "amiss/external-plan-payload";
pub const EVIDENCE_SCHEMA: &str = "amiss/external-evidence";
pub const ASSESSMENT_ENVELOPE_SCHEMA: &str = "amiss/external-assessment-envelope";
pub const ASSESSMENT_PAYLOAD_SCHEMA: &str = "amiss/external-assessment-payload";

/// Why a report yields no plan: the first defect found, in reading order.
/// Classification only; the command projecting a defect owns its wording.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanDefect {
    NotAReport,
    DigestMismatch,
    Incomplete,
    MalformedExternal,
}

/// One side's view of a destination: its scheme and every document naming it.
struct Entry {
    scheme: String,
    documents: BTreeSet<String>,
}

/// Derives the external plan from one complete scanner report: the distinct
/// external destinations the candidate introduced and the base lost, each
/// with its documents, bound to the payload digest the derivation verified.
/// The engine never fetches a destination; the plan only names the work an
/// evidence producer may do.
///
/// # Errors
///
/// Returns the first [`PlanDefect`] when the value is not a report envelope,
/// its digest does not hold, it is incomplete, or an external occurrence
/// lacks a field the exactly-when contract promises.
pub fn plan(
    envelope: &Value,
    engine_version: &str,
    engine_digest: &str,
) -> Result<Value, PlanDefect> {
    if envelope.text("schema") != Some(ENVELOPE_SCHEMA) {
        return Err(PlanDefect::NotAReport);
    }
    let Some(payload) = envelope.member("payload") else {
        return Err(PlanDefect::NotAReport);
    };
    if payload.text("schema") != Some(PAYLOAD_SCHEMA) {
        return Err(PlanDefect::NotAReport);
    }
    let Some(recorded) = envelope.text("payload_digest") else {
        return Err(PlanDefect::NotAReport);
    };
    if hj(PAYLOAD_SCHEMA, payload).to_string() != recorded {
        return Err(PlanDefect::DigestMismatch);
    }
    let complete = payload
        .member("result")
        .and_then(|result| result.member("complete"));
    if complete != Some(&Value::Bool(true)) {
        return Err(PlanDefect::Incomplete);
    }
    let Some(Value::Array(observations)) = payload.member("observations") else {
        return Err(PlanDefect::NotAReport);
    };
    let Some(evaluation) = payload.member("evaluation") else {
        return Err(PlanDefect::NotAReport);
    };
    let (Some(base_identity), Some(candidate_identity), Some(mode)) = (
        evaluation.member("base"),
        evaluation.member("candidate"),
        evaluation.member("mode"),
    ) else {
        return Err(PlanDefect::NotAReport);
    };

    let base = collect(observations, "base")?;
    let candidate = collect(observations, "candidate")?;
    let retained = candidate
        .keys()
        .filter(|destination| base.contains_key(*destination))
        .count();
    let recognition = Recognition {
        declared: evaluation
            .member("repository")
            .and_then(|repository| repository.text("host"))
            .zip(evaluation.text("forge")),
    };

    let plan_payload = object(vec![
        ("schema", string(PLAN_PAYLOAD_SCHEMA)),
        (
            "engine",
            object(vec![
                ("engine_version", string(engine_version)),
                ("engine_digest", string(engine_digest)),
            ]),
        ),
        (
            "report",
            object(vec![
                ("payload_digest", string(recorded)),
                ("base", base_identity.clone()),
                ("candidate", candidate_identity.clone()),
                ("mode", mode.clone()),
            ]),
        ),
        ("introduced", rows(&candidate, &base, &recognition)),
        ("removed", rows(&base, &candidate, &recognition)),
        (
            "retained_count",
            Value::Integer(i64::try_from(retained).unwrap_or(i64::MAX)),
        ),
    ]);
    let digest = hj(PLAN_PAYLOAD_SCHEMA, &plan_payload);
    Ok(object(vec![
        ("schema", string(PLAN_ENVELOPE_SCHEMA)),
        ("payload", plan_payload),
        ("payload_digest", string(&digest.to_string())),
    ]))
}

/// One side's destination map, from every observation row's occurrence on
/// that side whose resolution is external.
fn collect(observations: &[Value], side: &str) -> Result<BTreeMap<String, Entry>, PlanDefect> {
    let mut entries: BTreeMap<String, Entry> = BTreeMap::new();
    for row in observations {
        let Some(occurrence) = row.member(side) else {
            continue;
        };
        let resolution = occurrence.member("resolution");
        if resolution.and_then(|value| value.text("kind")) != Some("external") {
            continue;
        }
        let destination = occurrence
            .text("external_destination")
            .filter(|value| !value.is_empty());
        let document = occurrence
            .text("document")
            .filter(|value| !value.is_empty());
        let scheme = occurrence
            .member("intent")
            .and_then(|intent| intent.text("external_scheme"))
            .filter(|value| !value.is_empty());
        let (Some(destination), Some(document), Some(scheme)) = (destination, document, scheme)
        else {
            return Err(PlanDefect::MalformedExternal);
        };
        let entry = entries
            .entry(destination.to_owned())
            .or_insert_with(|| Entry {
                scheme: scheme.to_owned(),
                documents: BTreeSet::new(),
            });
        if entry.scheme != scheme {
            return Err(PlanDefect::MalformedExternal);
        }
        entry.documents.insert(document.to_owned());
    }
    Ok(entries)
}

/// The destinations present here and absent on the other side, one sorted
/// row each, with the forge shape attached where a host is recognized.
fn rows(
    entries: &BTreeMap<String, Entry>,
    other: &BTreeMap<String, Entry>,
    recognition: &Recognition<'_>,
) -> Value {
    Value::Array(
        entries
            .iter()
            .filter(|(destination, _)| !other.contains_key(*destination))
            .map(|(destination, entry)| {
                let mut members = vec![
                    ("destination", string(destination)),
                    ("scheme", string(&entry.scheme)),
                    (
                        "documents",
                        Value::Array(entry.documents.iter().map(|path| string(path)).collect()),
                    ),
                ];
                if let Some(repository) = repository_value(destination, recognition) {
                    members.push(("repository", repository));
                }
                object(members)
            })
            .collect(),
    )
}

/// The forge hosts this run can name: the built-in table plus the report's
/// own declared identity, whose dialect the evaluation already carries.
struct Recognition<'a> {
    declared: Option<(&'a str, &'a str)>,
}

impl Recognition<'_> {
    fn dialect(&self, host: &str) -> Option<&str> {
        if let Some((declared, dialect)) = self.declared
            && declared == host
        {
            return Some(dialect);
        }
        ForgeDialect::default_for_host(host).map(<&'static str>::from)
    }
}

/// The forge shape of one destination, structure only: owner and name split
/// by the dialect's grammar, the segment after them verbatim as the form,
/// and everything later as one opaque tail, since splitting revision from
/// path needs the other repository's refs, which branch slashes hide.
fn repository_value(destination: &str, recognition: &Recognition<'_>) -> Option<Value> {
    let rest = destination.strip_prefix("https://")?;
    let (host, path) = rest.split_at(rest.find(['/', '?', '#']).unwrap_or(rest.len()));
    let dialect = recognition.dialect(host)?;
    let path = path.strip_prefix('/')?.split(['?', '#']).next()?;
    let directory = path.ends_with('/');
    let mut segments: Vec<&str> = path.split('/').collect();
    if segments.last() == Some(&"") {
        segments.pop();
    }
    if segments.len() < 2 || segments.iter().any(|segment| segment.is_empty()) {
        return None;
    }
    let (project, form, tail) = if dialect == "gitlab" {
        // Without the separator a legacy file URL and a nested project page
        // are indistinguishable, so only the two-segment form is a shape.
        match segments.iter().position(|segment| *segment == "-") {
            Some(separator) if separator >= 2 => (
                segments.get(..separator)?,
                segments.get(separator.saturating_add(1)).copied(),
                segments
                    .get(separator.saturating_add(2)..)
                    .unwrap_or_default(),
            ),
            None if segments.len() == 2 => (segments.as_slice(), None, [].as_slice()),
            Some(_) | None => return None,
        }
    } else {
        (
            segments.get(..2)?,
            segments.get(2).copied(),
            segments.get(3..).unwrap_or_default(),
        )
    };
    let (name, owner) = project.split_last()?;
    let mut members = vec![
        ("dialect", string(dialect)),
        ("host", string(host)),
        ("name", string(name)),
        ("owner", string(&owner.join("/"))),
    ];
    if let Some(form) = form {
        members.push(("form", string(form)));
        if !tail.is_empty() {
            let mut tail = tail.join("/");
            if directory {
                tail.push('/');
            }
            members.push(("tail", string(&tail)));
        }
    }
    Some(object(members))
}

/// Why a plan and evidence yield no assessment: the first defect found.
/// Classification only; the command projecting a defect owns its wording.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssessDefect {
    NotAPlan,
    PlanDigestMismatch,
    NotEvidence,
    UnboundEvidence,
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
            Value::Array(verdict_rows(introduced, &observed)),
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
                    if matches!(failure.as_str(), "dns" | "tls" | "timeout" | "refused") =>
                {
                    None
                }
                (_, _) => return Err(AssessDefect::MalformedEvidence),
            };
            let retarget = match row.member("final_destination") {
                Some(Value::String(final_destination)) if !final_destination.is_empty() => {
                    Some(final_destination.clone())
                }
                None | Some(Value::Null) => None,
                Some(_) => return Err(AssessDefect::MalformedEvidence),
            };
            Ok(Observed::Probe {
                method_get,
                status,
                retarget,
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
                (Repository::Readable, Some(Value::String(tail))) => match tail.as_str() {
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

/// Whether the value is an external plan envelope whose payload matches its
/// recorded digest: the check a producer makes before spending calls on it.
#[must_use]
pub fn bound_plan(plan: &Value) -> bool {
    let (Some(payload), Some(recorded)) = (plan.member("payload"), plan.text("payload_digest"))
    else {
        return false;
    };
    plan.text("schema") == Some(PLAN_ENVELOPE_SCHEMA)
        && payload.text("schema") == Some(PLAN_PAYLOAD_SCHEMA)
        && hj(PLAN_PAYLOAD_SCHEMA, payload).to_string() == recorded
}

/// One producer's evidence file over a plan: the binding digest is read from
/// the plan itself, so a producer never computes one.
#[must_use]
pub fn evidence_file(
    plan: &Value,
    producer_name: &str,
    producer_version: &str,
    rows: Vec<Value>,
) -> Option<Value> {
    let digest = plan.text("payload_digest")?;
    Some(object(vec![
        ("schema", string(EVIDENCE_SCHEMA)),
        ("plan_payload_digest", string(digest)),
        (
            "producer",
            object(vec![
                ("name", string(producer_name)),
                ("version", string(producer_version)),
            ]),
        ),
        ("rows", Value::Array(rows)),
    ]))
}

/// One http-probe observation row: the final status or the transport
/// failure, exactly one of the two, and where redirects ended when that
/// differs from the destination.
#[must_use]
pub fn probe_evidence_row(
    destination: &str,
    method: &str,
    status: Option<i64>,
    failure: Option<&str>,
    final_destination: Option<&str>,
    checked_at: &str,
) -> Value {
    let mut members = vec![
        ("kind", string("http-probe")),
        ("destination", string(destination)),
        ("method", string(method)),
        ("checked_at", string(checked_at)),
    ];
    if let Some(status) = status {
        members.push(("status", Value::Integer(status)));
    }
    if let Some(failure) = failure {
        members.push(("failure", string(failure)));
    }
    if let Some(final_destination) = final_destination {
        members.push(("final_destination", string(final_destination)));
    }
    object(members)
}

/// One forge-api observation row, tail present only when a resolution was
/// actually established.
#[must_use]
pub fn forge_evidence_row(
    destination: &str,
    repository: &str,
    tail: Option<&str>,
    checked_at: &str,
) -> Value {
    let mut members = vec![
        ("kind", string("forge-api")),
        ("destination", string(destination)),
        ("repository", string(repository)),
        ("checked_at", string(checked_at)),
    ];
    if let Some(tail) = tail {
        members.push(("tail", string(tail)));
    }
    object(members)
}

/// A plan object in canonical member order; the parser demands sorted keys,
/// so the builder sorts rather than trusting the spelling site.
fn object(members: Vec<(&str, Value)>) -> Value {
    let mut members: Vec<(String, Value)> = members
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value))
        .collect();
    members.sort_by(|left, right| left.0.cmp(&right.0));
    Value::Object(members)
}

fn string(value: &str) -> Value {
    Value::String(value.to_owned())
}
