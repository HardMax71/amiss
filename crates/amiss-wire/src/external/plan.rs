use std::collections::{BTreeMap, BTreeSet};

use crate::digest::hj;
use crate::json::Value;
use crate::model::ForgeDialect;
use crate::report::validate_envelope;

use super::{PLAN_ENVELOPE_SCHEMA, PLAN_PAYLOAD_SCHEMA, PlanDefect, object, string};

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
    let (payload, recorded, _verdict) = validate_envelope(envelope)?;
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
