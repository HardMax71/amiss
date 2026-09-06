use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

use crate::de::{self, Error, ErrorKind, fail};
use crate::digest::{Digest, hb, hj_serde};
use crate::json;
use crate::model::ForgeDialect;
use crate::report::model::{
    Evaluation, ExternalResolutionReason, ObservationComparison, Occurrence, RepoPath, Resolution,
    VersionScope,
};
use crate::report::validate_envelope;

use super::{EXTERNAL_DOCUMENT_BYTES, PLAN_PAYLOAD_SCHEMA, PlanDefect};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalPlanEnvelope<P = ExternalPlan> {
    pub schema: ExternalPlanEnvelopeSchema,
    pub payload: P,
    pub payload_digest: Digest,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum ExternalPlanEnvelopeSchema {
    #[strum(serialize = "amiss/external-plan-envelope")]
    Current,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalPlan<B = BTreeMap<String, serde_json::Value>, C = B> {
    pub schema: ExternalPlanPayloadSchema,
    pub engine: ExternalEngine,
    pub report: ExternalPlanReport<B, C>,
    pub introduced: Vec<ExternalDestination>,
    pub removed: Vec<ExternalDestination>,
    pub retained_count: u64,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum ExternalPlanPayloadSchema {
    #[strum(serialize = "amiss/external-plan-payload")]
    Current,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, wary::Wary)]
pub struct ExternalEngine {
    #[validate(length(chars, 1..))]
    pub engine_version: String,
    pub engine_digest: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalPlanReport<B = BTreeMap<String, serde_json::Value>, C = B> {
    pub payload_digest: Digest,
    pub base: B,
    pub candidate: C,
    pub mode: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalDestination {
    pub destination: String,
    pub scheme: String,
    pub documents: Vec<String>,
    #[serde(
        default,
        deserialize_with = "json_serde::deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub repository: Option<ExternalRepository>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalRepository {
    pub host: String,
    pub dialect: ForgeDialect,
    pub owner: String,
    pub name: String,
    #[serde(
        default,
        deserialize_with = "json_serde::deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub form: Option<String>,
    #[serde(
        default,
        deserialize_with = "json_serde::deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub tail: Option<String>,
}

/// One side's view of a destination: its scheme and every document naming it.
struct Entry {
    scheme: String,
    documents: BTreeSet<String>,
}

/// Derives the external plan from one complete scanner report: the distinct
/// destinations delegated to evidence that the candidate introduced and the
/// base lost, each with its documents, bound to the payload digest the
/// derivation verified.
/// The engine never fetches a destination; the plan only names the work an
/// evidence producer may do.
///
/// # Errors
///
/// Returns the first [`PlanDefect`] when the bytes are not a report envelope,
/// its digest does not hold, it is incomplete, or a delegated occurrence
/// lacks a field the exactly-when contract promises.
pub fn plan(
    envelope: &[u8],
    engine_version: &str,
    engine_digest: Digest,
) -> Result<Vec<u8>, PlanDefect> {
    let (payload, recorded, _verdict) = validate_envelope(envelope)?;
    if !payload.result.complete {
        return Err(PlanDefect::Incomplete);
    }
    let Evaluation::Resolved(evaluation) = &payload.evaluation else {
        return Err(PlanDefect::NotAReport);
    };

    let base = collect(&payload.observations, |comparison| comparison.base.as_ref())?;
    let candidate = collect(&payload.observations, |comparison| {
        comparison.candidate.as_ref()
    })?;
    let retained = candidate
        .keys()
        .filter(|destination| base.contains_key(*destination))
        .count();
    let declared = evaluation
        .repository
        .as_ref()
        .zip(evaluation.forge)
        .map(|(repository, dialect)| (repository.host(), dialect));
    let payload = ExternalPlan {
        schema: ExternalPlanPayloadSchema::Current,
        engine: ExternalEngine {
            engine_version: engine_version.to_owned(),
            engine_digest,
        },
        report: ExternalPlanReport {
            payload_digest: recorded,
            base: &evaluation.base,
            candidate: &evaluation.candidate,
            mode: evaluation.mode.as_ref().to_owned(),
        },
        introduced: rows(&candidate, &base, declared),
        removed: rows(&base, &candidate, declared),
        retained_count: u64::try_from(retained).unwrap_or(u64::MAX),
    };
    let payload_digest =
        plan_payload_digest(&payload).map_err(|_defect| PlanDefect::MalformedExternal)?;
    let document = ExternalPlanEnvelope {
        schema: ExternalPlanEnvelopeSchema::Current,
        payload,
        payload_digest,
    };
    let canonical = serde_json_canonicalizer::to_vec(&document)
        .map_err(|_defect| PlanDefect::MalformedExternal)?;
    if u64::try_from(canonical.len()).unwrap_or(u64::MAX) > EXTERNAL_DOCUMENT_BYTES {
        return Err(PlanDefect::MalformedExternal);
    }
    json::parse(&canonical).map_err(|_defect| PlanDefect::MalformedExternal)?;
    Ok(canonical)
}

/// Parses one strict, digest-bound external plan while ignoring additive fields.
///
/// # Errors
///
/// Fails on oversized or malformed strict JSON, a malformed known field, a
/// violated plan law, or a payload digest mismatch.
pub fn parse_plan(bytes: &[u8]) -> Result<ExternalPlanEnvelope, Error> {
    let envelope: ExternalPlanEnvelope<serde_json::Value> = super::read(bytes)?;
    let payload_digest = hj_serde(PLAN_PAYLOAD_SCHEMA, |mut writer| {
        serde_json_canonicalizer::to_writer(&envelope.payload, &mut writer)
    })
    .map_err(|_defect| Error::new("$.payload", ErrorKind::InvalidValue))?;
    let document = ExternalPlanEnvelope {
        schema: envelope.schema,
        payload: de::deserialize_value("$.payload", envelope.payload)?,
        payload_digest: envelope.payload_digest,
    };
    if payload_digest != document.payload_digest {
        return fail("$.payload_digest", ErrorKind::DigestMismatch);
    }
    validate_plan(&document.payload)?;
    Ok(document)
}

fn plan_payload_digest<B: Serialize, C: Serialize>(
    plan: &ExternalPlan<B, C>,
) -> Result<Digest, Error> {
    validate_plan(plan)?;
    serde_json_canonicalizer::to_vec(plan)
        .map(|canonical| hb(PLAN_PAYLOAD_SCHEMA, &canonical))
        .map_err(|_defect| Error::new("$.payload", ErrorKind::InvalidValue))
}

fn validate_plan<B, C>(plan: &ExternalPlan<B, C>) -> Result<(), Error> {
    if plan.engine.engine_version.is_empty() {
        return fail("$.payload.engine.engine_version", ErrorKind::InvalidValue);
    }
    if plan.report.mode.is_empty() {
        return fail("$.payload.report.mode", ErrorKind::InvalidValue);
    }
    if plan.retained_count > json::MAX_SAFE_INTEGER.unsigned_abs() {
        return fail("$.payload.retained_count", ErrorKind::LimitExceeded);
    }
    validate_rows("$.payload.introduced", &plan.introduced)?;
    validate_rows("$.payload.removed", &plan.removed)?;
    let introduced: BTreeSet<&str> = plan
        .introduced
        .iter()
        .map(|row| row.destination.as_str())
        .collect();
    if plan
        .removed
        .iter()
        .any(|row| introduced.contains(row.destination.as_str()))
    {
        return fail("$.payload", ErrorKind::Inconsistent);
    }
    Ok(())
}

fn validate_rows(path: &str, rows: &[ExternalDestination]) -> Result<(), Error> {
    let mut destinations = BTreeSet::new();
    for (index, row) in rows.iter().enumerate() {
        let row_path = format!("{path}[{index}]");
        let destination_length = row.destination.chars().count();
        if !(1..=16_384).contains(&destination_length) {
            return fail(&format!("{row_path}.destination"), ErrorKind::InvalidValue);
        }
        if !destinations.insert(row.destination.as_str()) {
            return fail(path, ErrorKind::DuplicateMember);
        }
        let mut scheme = row.scheme.bytes();
        if !scheme.next().is_some_and(|byte| byte.is_ascii_lowercase())
            || !scheme.all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'+' | b'.' | b'-')
            })
        {
            return fail(&format!("{row_path}.scheme"), ErrorKind::InvalidValue);
        }
        if row.documents.is_empty() || row.documents.iter().any(String::is_empty) {
            return fail(&format!("{row_path}.documents"), ErrorKind::InvalidValue);
        }
        crate::controls::sorted_set(&format!("{row_path}.documents"), &row.documents, Ord::cmp)?;
        if let Some(repository) = &row.repository {
            validate_repository(&format!("{row_path}.repository"), repository)?;
        }
    }
    Ok(())
}

fn validate_repository(path: &str, repository: &ExternalRepository) -> Result<(), Error> {
    for (field, value) in [
        ("host", repository.host.as_str()),
        ("owner", repository.owner.as_str()),
        ("name", repository.name.as_str()),
    ] {
        if value.is_empty() {
            return fail(&format!("{path}.{field}"), ErrorKind::InvalidValue);
        }
    }
    if repository.form.as_ref().is_some_and(String::is_empty) {
        return fail(&format!("{path}.form"), ErrorKind::InvalidValue);
    }
    if repository.tail.as_ref().is_some_and(String::is_empty) {
        return fail(&format!("{path}.tail"), ErrorKind::InvalidValue);
    }
    if repository.tail.is_some() && repository.form.is_none() {
        return fail(path, ErrorKind::Inconsistent);
    }
    Ok(())
}

/// One side's destinations delegated to another evidence layer.
fn collect<'report>(
    observations: &'report [ObservationComparison],
    side: impl Fn(&'report ObservationComparison) -> Option<&'report Occurrence>,
) -> Result<BTreeMap<String, Entry>, PlanDefect> {
    let mut entries: BTreeMap<String, Entry> = BTreeMap::new();
    for row in observations {
        let Some(occurrence) = side(row) else {
            continue;
        };
        let external = matches!(
            &occurrence.resolution,
            Resolution::External {
                reason: ExternalResolutionReason::Url | ExternalResolutionReason::ForeignRepository,
                ..
            }
        );
        let historical = matches!(
            &occurrence.resolution,
            Resolution::UnsupportedVersion {
                scope: VersionScope::KnownCommit { .. },
                ..
            }
        );
        if (!external && !historical)
            || matches!(
                &occurrence.resolution,
                Resolution::External {
                    reason: ExternalResolutionReason::IntersphinxInventory
                        | ExternalResolutionReason::SiteBuild,
                    ..
                }
            )
        {
            continue;
        }
        let destination = occurrence
            .external_destination
            .as_deref()
            .filter(|value| !value.is_empty());
        let document = match &occurrence.document {
            RepoPath::Text(path) => Some(path.as_str()),
            RepoPath::Bytes(_) => None,
        };
        let scheme = if historical {
            Some("https")
        } else {
            occurrence
                .intent
                .external_scheme
                .as_deref()
                .filter(|value| !value.is_empty())
        };
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
    declared: Option<(&str, ForgeDialect)>,
) -> Vec<ExternalDestination> {
    entries
        .iter()
        .filter(|(destination, _)| !other.contains_key(*destination))
        .map(|(destination, entry)| ExternalDestination {
            destination: destination.clone(),
            scheme: entry.scheme.clone(),
            documents: entry.documents.iter().cloned().collect(),
            repository: repository(destination, declared),
        })
        .collect()
}

/// The forge shape of one destination, structure only: owner and name split
/// by the dialect's grammar, the segment after them verbatim as the form,
/// and everything later as one opaque tail, since splitting revision from
/// path needs the other repository's refs, which branch slashes hide.
fn repository(
    destination: &str,
    declared: Option<(&str, ForgeDialect)>,
) -> Option<ExternalRepository> {
    let rest = destination.strip_prefix("https://")?;
    let (host, path) = rest.split_at(rest.find(['/', '?', '#']).unwrap_or(rest.len()));
    let dialect = declared
        .filter(|(declared_host, _dialect)| *declared_host == host)
        .map(|(_declared_host, dialect)| dialect)
        .or_else(|| ForgeDialect::default_for_host(host))?;
    let path = path.strip_prefix('/')?.split(['?', '#']).next()?;
    let directory = path.ends_with('/');
    let mut segments: Vec<&str> = path.split('/').collect();
    if segments.last() == Some(&"") {
        segments.pop();
    }
    if segments.len() < 2 || segments.iter().any(|segment| segment.is_empty()) {
        return None;
    }
    let (owner, name, form, tail) = if dialect == ForgeDialect::BitbucketDataCenter {
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
        let (project, form, tail) = if dialect == ForgeDialect::Gitlab {
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
    let tail = (!tail.is_empty()).then(|| {
        let mut tail = tail.join("/");
        if directory {
            tail.push('/');
        }
        tail
    });
    Some(ExternalRepository {
        host: host.to_owned(),
        dialect,
        owner: owner.into_owned(),
        name: name.to_owned(),
        form: form.map(str::to_owned),
        tail,
    })
}
