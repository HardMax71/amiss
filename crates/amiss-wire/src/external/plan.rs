use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use serde_json::Map;

use crate::codec::{self, Schema};
use crate::digest::Digest;
use crate::json::Value;
use crate::model::ForgeDialect;
use crate::report::validate_envelope;

use super::PlanDefect;
use super::document::{DestinationRow, Engine, Plan, ReportBinding, RepositoryShape};

struct Entry {
    scheme: String,
    documents: BTreeSet<String>,
}

#[derive(Deserialize)]
struct Evaluation {
    base: Map<String, Value>,
    candidate: Map<String, Value>,
    mode: String,
    #[serde(default)]
    repository: Option<DeclaredRepository>,
    #[serde(default)]
    forge: Option<ForgeDialect>,
}

#[derive(Deserialize)]
struct DeclaredRepository {
    host: String,
}

#[derive(Deserialize)]
struct Report {
    evaluation: Evaluation,
}

/// Derives sorted introduced and removed external destinations from a complete report.
///
/// # Errors
///
/// The report is invalid or incomplete, or a delegated occurrence lacks its required facts.
pub fn plan(
    envelope: &Value,
    engine_version: &str,
    engine_digest: &str,
) -> Result<Value, PlanDefect> {
    let (payload, recorded, verdict) = validate_envelope(envelope)?;
    if verdict == crate::ExitClass::Failure {
        return Err(PlanDefect::Incomplete);
    }
    let Report { evaluation } =
        codec::from_value("$.payload", payload).map_err(|_defect| PlanDefect::NotAReport)?;
    let observations = payload
        .get("observations")
        .and_then(Value::as_array)
        .ok_or(PlanDefect::NotAReport)?;
    let base = collect(observations, "base")?;
    let candidate = collect(observations, "candidate")?;
    let retained = candidate
        .keys()
        .filter(|destination| base.contains_key(*destination))
        .count();
    let recognition = Recognition {
        declared: evaluation
            .repository
            .as_ref()
            .zip(evaluation.forge.as_ref())
            .map(|(repository, dialect)| (repository.host.as_str(), dialect.as_ref())),
    };
    let introduced = rows(&candidate, &base, &recognition);
    let removed = rows(&base, &candidate, &recognition);
    let plan = Plan {
        schema: Schema::default(),
        engine: Engine::new(engine_version, engine_digest)
            .map_err(|_defect| PlanDefect::NotAReport)?,
        report: ReportBinding {
            payload_digest: Digest::from_wire(recorded).ok_or(PlanDefect::NotAReport)?,
            base: evaluation.base,
            candidate: evaluation.candidate,
            mode: evaluation.mode,
        },
        introduced,
        removed,
        retained_count: u64::try_from(retained).map_err(|_defect| PlanDefect::MalformedExternal)?,
    };
    codec::seal_value(&plan).map_err(|_defect| PlanDefect::MalformedExternal)
}

#[derive(Deserialize)]
struct Kind<'a> {
    kind: &'a str,
}

#[derive(Deserialize)]
struct Resolution<'a> {
    #[serde(default)]
    reason: Option<&'a str>,
}

#[derive(Deserialize)]
struct Historical<'a> {
    #[serde(borrow)]
    scope: Kind<'a>,
}

#[derive(Deserialize)]
struct Delegated<'a> {
    document: &'a str,
    external_destination: &'a str,
}

#[derive(Deserialize)]
struct Intent<'a> {
    external_scheme: &'a str,
}

#[derive(Deserialize)]
struct Request<'a> {
    #[serde(borrow)]
    intent: Intent<'a>,
}

fn collect(observations: &[Value], side: &str) -> Result<BTreeMap<String, Entry>, PlanDefect> {
    let mut entries: BTreeMap<String, Entry> = BTreeMap::new();
    for row in observations {
        let Some(occurrence) = row.get(side) else {
            continue;
        };
        let Some(resolution) = occurrence.get("resolution") else {
            continue;
        };
        let Ok(kind) = codec::borrow_value::<Kind<'_>>("$.resolution", resolution) else {
            continue;
        };
        let historical = match kind.kind {
            "external" => false,
            "unsupported-version" => {
                let historical: Historical<'_> = codec::borrow_value("$.resolution", resolution)
                    .map_err(|_defect| PlanDefect::MalformedExternal)?;
                if historical.scope.kind != "known-commit" {
                    continue;
                }
                true
            }
            _ => continue,
        };
        let selected: Resolution<'_> = codec::borrow_value("$.resolution", resolution)
            .map_err(|_defect| PlanDefect::MalformedExternal)?;
        if matches!(
            selected.reason,
            Some("intersphinx-inventory" | "site-build")
        ) {
            continue;
        }
        let delegated: Delegated<'_> = codec::borrow_value("$.occurrence", occurrence)
            .map_err(|_defect| PlanDefect::MalformedExternal)?;
        let scheme = if historical {
            "https"
        } else {
            codec::borrow_value::<Request<'_>>("$.occurrence", occurrence)
                .map_err(|_defect| PlanDefect::MalformedExternal)?
                .intent
                .external_scheme
        };
        if delegated.document.is_empty()
            || delegated.external_destination.is_empty()
            || scheme.is_empty()
        {
            return Err(PlanDefect::MalformedExternal);
        }
        let entry = entries
            .entry(delegated.external_destination.to_owned())
            .or_insert_with(|| Entry {
                scheme: scheme.to_owned(),
                documents: BTreeSet::new(),
            });
        if entry.scheme != scheme {
            return Err(PlanDefect::MalformedExternal);
        }
        entry.documents.insert(delegated.document.to_owned());
    }
    Ok(entries)
}

fn rows(
    entries: &BTreeMap<String, Entry>,
    other: &BTreeMap<String, Entry>,
    recognition: &Recognition<'_>,
) -> Vec<DestinationRow> {
    entries
        .iter()
        .filter(|(destination, _)| !other.contains_key(*destination))
        .map(|(destination, entry)| DestinationRow {
            destination: destination.clone(),
            scheme: entry.scheme.clone(),
            documents: entry.documents.iter().cloned().collect(),
            repository: repository_shape(destination, recognition),
        })
        .collect()
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
fn repository_shape(destination: &str, recognition: &Recognition<'_>) -> Option<RepositoryShape> {
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
    let (owner, name, form, tail) = if dialect == "bitbucket-data-center" {
        let marker = segments
            .iter()
            .position(|segment| matches!(*segment, "projects" | "users"))?;
        let [route, owner, "repos", name, rest @ ..] = segments.get(marker..)? else {
            return None;
        };
        let owner = if *route == "projects" {
            owner.strip_prefix('~').unwrap_or(owner)
        } else {
            owner
        };
        let (form, tail) = rest
            .split_first()
            .map_or((None, [].as_slice()), |(form, tail)| (Some(*form), tail));
        (Cow::Borrowed(owner), *name, form, tail)
    } else {
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
        (Cow::Owned(owner.join("/")), *name, form, tail)
    };
    let tail = if form.is_some() && !tail.is_empty() {
        let mut joined = tail.join("/");
        if directory {
            joined.push('/');
        }
        Some(joined)
    } else {
        None
    };
    Some(RepositoryShape {
        dialect: dialect.parse().ok()?,
        host: host.to_owned(),
        owner: owner.into_owned(),
        name: name.to_owned(),
        form: form.map(str::to_owned),
        tail,
    })
}
