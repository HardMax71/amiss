mod tests;

use amiss_wire::external::{bound_plan, evidence_file, forge_evidence_row};
use amiss_wire::json::Value;
use serde::Deserialize;

use crate::ProviderError;

/// What a forge API established about a foreign repository.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForgeVisibility {
    Readable,
    Missing,
    Denied,
}

/// Whether a forge route established that its subject exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForgePresence {
    Present,
    Absent,
    Unknown,
}

/// The two namespaces in which a forge stores named refs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForgeRefFamily {
    Heads,
    Tags,
}

impl ForgeRefFamily {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Heads => "heads",
            Self::Tags => "tags",
        }
    }
}

/// A tail fact an evidence producer can state after reading a repository.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForgeTail {
    Resolved,
    RevisionMissing,
    PathMissing,
}

/// One introduced destination whose repository shape belongs to a producer.
#[derive(Clone, Copy)]
pub struct ForgeTarget<'a> {
    pub repository: &'a Value,
    pub owner: &'a str,
    pub name: &'a str,
}

/// The fixed identity and clock value attached to one evidence file.
#[derive(Clone, Copy)]
pub struct ForgeProducer<'a> {
    pub dialect: &'a str,
    pub host: &'a str,
    pub name: &'a str,
    pub version: &'a str,
    pub checked_at: &'a str,
}

/// The evidence learned for one repository, including a readable repository
/// whose tail lookup exhausted the provider before the next row could begin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForgeEvidence {
    Missing,
    Denied,
    Readable(Option<ForgeTail>),
    ReadableThenUnavailable,
}

/// Turns repository visibility and a lazy tail lookup into one evidence fact.
///
/// # Errors
///
/// Returns a provider defect other than standing unavailability. An unavailable
/// tail lookup preserves the readable fact and asks the outer walk to stop.
pub fn forge_repository_evidence(
    visibility: ForgeVisibility,
    resolve_tail: impl FnOnce() -> Result<Option<ForgeTail>, ProviderError>,
) -> Result<ForgeEvidence, ProviderError> {
    match visibility {
        ForgeVisibility::Missing => Ok(ForgeEvidence::Missing),
        ForgeVisibility::Denied => Ok(ForgeEvidence::Denied),
        ForgeVisibility::Readable => match resolve_tail() {
            Ok(tail) => Ok(ForgeEvidence::Readable(tail)),
            Err(ProviderError::Unavailable) => Ok(ForgeEvidence::ReadableThenUnavailable),
            Err(defect) => Err(defect),
        },
    }
}

/// Verifies the introduced forge destinations belonging to one dialect and
/// host, preserving partial evidence when the provider becomes unavailable.
///
/// # Errors
///
/// Returns [`ProviderError::InvalidResponse`] when the plan is not bound or an
/// evidence file cannot be formed, and propagates non-availability provider
/// defects returned while preparing or inspecting the provider.
pub fn forge_evidence<S>(
    plan: &Value,
    producer: ForgeProducer<'_>,
    prepare: impl FnOnce() -> Result<S, ProviderError>,
    mut inspect: impl FnMut(&mut S, ForgeTarget<'_>) -> Result<ForgeEvidence, ProviderError>,
) -> Result<Value, ProviderError> {
    if !bound_plan(plan) {
        return Err(ProviderError::InvalidResponse);
    }
    let plan_projection: Plan = amiss_wire::codec::from_value("$", plan)
        .map_err(|_defect| ProviderError::InvalidResponse)?;
    let mut state = prepare()?;
    let mut rows = Vec::new();
    for row in plan_projection.payload.introduced {
        let (Some(destination), Some(repository)) = (row.destination, row.repository) else {
            continue;
        };
        let Ok(shape) = amiss_wire::codec::from_value::<Repository>("$.repository", &repository)
        else {
            continue;
        };
        if shape.dialect != producer.dialect || shape.host != producer.host {
            continue;
        }
        let evidence = match inspect(
            &mut state,
            ForgeTarget {
                repository: &repository,
                owner: &shape.owner,
                name: &shape.name,
            },
        ) {
            Ok(evidence) => evidence,
            Err(ProviderError::Unavailable) => break,
            Err(defect) => return Err(defect),
        };
        let (repository_fact, tail, stop) = match evidence {
            ForgeEvidence::Missing => ("missing", None, false),
            ForgeEvidence::Denied => ("denied", None, false),
            ForgeEvidence::Readable(tail) => ("readable", tail.map(forge_tail_name), false),
            ForgeEvidence::ReadableThenUnavailable => ("readable", None, true),
        };
        rows.push(
            forge_evidence_row(&destination, repository_fact, tail, producer.checked_at)
                .map_err(|_defect| ProviderError::InvalidResponse)?,
        );
        if stop {
            break;
        }
    }
    evidence_file(plan, producer.name, producer.version, rows)
        .map_err(|_defect| ProviderError::InvalidResponse)
}

#[derive(Deserialize)]
struct Plan {
    payload: PlanPayload,
}

#[derive(Deserialize)]
struct PlanPayload {
    introduced: Vec<Introduced>,
}

#[derive(Deserialize)]
struct Introduced {
    destination: Option<String>,
    repository: Option<Value>,
}

#[derive(Deserialize)]
struct Repository {
    dialect: String,
    host: String,
    owner: String,
    name: String,
}

const fn forge_tail_name(tail: ForgeTail) -> &'static str {
    match tail {
        ForgeTail::Resolved => "resolved",
        ForgeTail::RevisionMissing => "revision-missing",
        ForgeTail::PathMissing => "path-missing",
    }
}
