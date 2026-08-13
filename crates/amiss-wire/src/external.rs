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
    if text(envelope, "schema") != Some(ENVELOPE_SCHEMA) {
        return Err(PlanDefect::NotAReport);
    }
    let Some(payload) = member(envelope, "payload") else {
        return Err(PlanDefect::NotAReport);
    };
    if text(payload, "schema") != Some(PAYLOAD_SCHEMA) {
        return Err(PlanDefect::NotAReport);
    }
    let Some(recorded) = text(envelope, "payload_digest") else {
        return Err(PlanDefect::NotAReport);
    };
    if hj(PAYLOAD_SCHEMA, payload).to_string() != recorded {
        return Err(PlanDefect::DigestMismatch);
    }
    let complete = member(payload, "result").and_then(|result| member(result, "complete"));
    if complete != Some(&Value::Bool(true)) {
        return Err(PlanDefect::Incomplete);
    }
    let Some(Value::Array(observations)) = member(payload, "observations") else {
        return Err(PlanDefect::NotAReport);
    };
    let Some(evaluation) = member(payload, "evaluation") else {
        return Err(PlanDefect::NotAReport);
    };
    let (Some(base_identity), Some(candidate_identity), Some(mode)) = (
        member(evaluation, "base"),
        member(evaluation, "candidate"),
        member(evaluation, "mode"),
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
        declared: member(evaluation, "repository")
            .and_then(|repository| text(repository, "host"))
            .zip(text(evaluation, "forge")),
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
        let Some(occurrence) = member(row, side) else {
            continue;
        };
        let resolution = member(occurrence, "resolution");
        if resolution.and_then(|value| text(value, "kind")) != Some("external") {
            continue;
        }
        let destination =
            text(occurrence, "external_destination").filter(|value| !value.is_empty());
        let document = text(occurrence, "document").filter(|value| !value.is_empty());
        let scheme = member(occurrence, "intent")
            .and_then(|intent| text(intent, "external_scheme"))
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
    if text(plan, "schema") != Some(PLAN_ENVELOPE_SCHEMA) {
        return Err(AssessDefect::NotAPlan);
    }
    let (Some(payload), Some(recorded)) = (member(plan, "payload"), text(plan, "payload_digest"))
    else {
        return Err(AssessDefect::NotAPlan);
    };
    if hj(PLAN_PAYLOAD_SCHEMA, payload).to_string() != recorded {
        return Err(AssessDefect::PlanDigestMismatch);
    }
    let report_digest = member(payload, "report").and_then(|report| text(report, "payload_digest"));
    let (Some(report_digest), Some(Value::Array(introduced))) =
        (report_digest, member(payload, "introduced"))
    else {
        return Err(AssessDefect::NotAPlan);
    };

    if text(evidence, "schema") != Some(EVIDENCE_SCHEMA) {
        return Err(AssessDefect::NotEvidence);
    }
    let producer = member(evidence, "producer")
        .filter(|producer| {
            text(producer, "name").is_some_and(|name| !name.is_empty())
                && text(producer, "version").is_some()
        })
        .ok_or(AssessDefect::NotEvidence)?;
    if text(evidence, "plan_payload_digest") != Some(recorded) {
        return Err(AssessDefect::UnboundEvidence);
    }
    let Some(Value::Array(evidence_rows)) = member(evidence, "rows") else {
        return Err(AssessDefect::NotEvidence);
    };

    let observed = observed_rows(evidence_rows, introduced)?;
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
    introduced: &[Value],
) -> Result<BTreeMap<&'e str, Observed>, AssessDefect> {
    let mut observed: BTreeMap<&str, Observed> = BTreeMap::new();
    for row in evidence_rows {
        let destination = text(row, "destination").ok_or(AssessDefect::MalformedEvidence)?;
        let shaped = introduced.iter().find_map(|candidate| {
            (text(candidate, "destination") == Some(destination))
                .then(|| member(candidate, "repository"))
        });
        let Some(shape) = shaped else {
            return Err(AssessDefect::UnboundEvidence);
        };
        if text(row, "checked_at").is_none_or(str::is_empty) {
            return Err(AssessDefect::MalformedEvidence);
        }
        let observation = observe(row, shape.is_some())?;
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
            let destination = text(row, "destination").unwrap_or_default();
            let mut members = vec![
                ("destination", string(destination)),
                (
                    "documents",
                    member(row, "documents").cloned().unwrap_or(Value::Null),
                ),
            ];
            let (verdict, reason, retarget) = match observed.get(destination) {
                None => ("unproven", Some("unexamined"), None),
                Some(seen) => judge(seen, member(row, "repository")),
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
/// admissible for a destination the plan shaped.
fn observe(row: &Value, shaped: bool) -> Result<Observed, AssessDefect> {
    match text(row, "kind") {
        Some("http-probe") => {
            let method_get = match text(row, "method") {
                Some("get") => true,
                Some("head") => false,
                Some(_) | None => return Err(AssessDefect::MalformedEvidence),
            };
            let status = member(row, "status");
            let failure = member(row, "failure");
            let status = match (status, failure) {
                (Some(Value::Integer(status)), None | Some(Value::Null)) => Some(*status),
                (None | Some(Value::Null), Some(Value::String(failure)))
                    if matches!(failure.as_str(), "dns" | "tls" | "timeout" | "refused") =>
                {
                    None
                }
                (_, _) => return Err(AssessDefect::MalformedEvidence),
            };
            let retarget = match member(row, "final_destination") {
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
            if !shaped {
                return Err(AssessDefect::UnboundEvidence);
            }
            let repository = match text(row, "repository") {
                Some("readable") => Repository::Readable,
                Some("missing") => Repository::Missing,
                Some("denied") => Repository::Denied,
                Some(_) | None => return Err(AssessDefect::MalformedEvidence),
            };
            let tail = match (repository, member(row, "tail")) {
                (_, None | Some(Value::Null)) => None,
                (Repository::Readable, Some(Value::String(tail))) => match tail.as_str() {
                    "resolved" => Some(Tail::Resolved),
                    "path-missing" => Some(Tail::PathMissing),
                    "revision-missing" => Some(Tail::RevisionMissing),
                    _ => return Err(AssessDefect::MalformedEvidence),
                },
                (_, Some(_)) => return Err(AssessDefect::MalformedEvidence),
            };
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
                let unresolved = shape.is_some_and(|shape| member(shape, "tail").is_some());
                if unresolved {
                    ("unproven", Some("unconfirmed"), None)
                } else {
                    ("reachable", None, None)
                }
            }
        },
    }
}

fn member<'v>(value: &'v Value, name: &str) -> Option<&'v Value> {
    if let Value::Object(members) = value {
        members
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value)
    } else {
        None
    }
}

fn text<'v>(value: &'v Value, name: &str) -> Option<&'v str> {
    if let Some(Value::String(text)) = member(value, name) {
        Some(text)
    } else {
        None
    }
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
