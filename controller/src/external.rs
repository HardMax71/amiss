mod tests;

use amiss_wire::external::{
    ExternalEvidence, ExternalEvidenceProducer, ExternalEvidenceRow, ExternalEvidenceSchema,
    ExternalRepository, evidence, parse_plan,
};
pub use amiss_wire::external::{ForgeRepository as ForgeVisibility, ForgeTail};
use amiss_wire::model::ForgeDialect;
use strum::AsRefStr;

use crate::ProviderError;

/// Whether a forge route established that its subject exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForgePresence {
    Present,
    Absent,
    Unknown,
}

/// The two namespaces in which a forge stores named refs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, AsRefStr)]
#[strum(serialize_all = "lowercase")]
pub enum ForgeRefFamily {
    Heads,
    Tags,
}

/// The fixed identity and clock value attached to one evidence file.
#[derive(Clone, Copy)]
pub struct ForgeProducer<'a> {
    pub dialect: ForgeDialect,
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
    plan: &[u8],
    producer: ForgeProducer<'_>,
    prepare: impl FnOnce() -> Result<S, ProviderError>,
    mut inspect: impl FnMut(&mut S, &ExternalRepository) -> Result<ForgeEvidence, ProviderError>,
) -> Result<Vec<u8>, ProviderError> {
    let plan = parse_plan(plan).map_err(|_defect| ProviderError::InvalidResponse)?;
    let mut state = prepare()?;
    let mut rows = Vec::new();
    for row in &plan.payload.introduced {
        let Some(repository) = row.repository.as_ref() else {
            continue;
        };
        if repository.dialect != producer.dialect || repository.host != producer.host {
            continue;
        }
        let evidence = match inspect(&mut state, repository) {
            Ok(evidence) => evidence,
            Err(ProviderError::Unavailable) => break,
            Err(defect) => return Err(defect),
        };
        let (repository, tail, stop) = match evidence {
            ForgeEvidence::Missing => (ForgeVisibility::Missing, None, false),
            ForgeEvidence::Denied => (ForgeVisibility::Denied, None, false),
            ForgeEvidence::Readable(tail) => (ForgeVisibility::Readable, tail, false),
            ForgeEvidence::ReadableThenUnavailable => (ForgeVisibility::Readable, None, true),
        };
        rows.push(ExternalEvidenceRow::ForgeApi {
            destination: row.destination.clone(),
            repository,
            tail,
            checked_at: producer.checked_at.to_owned(),
        });
        if stop {
            break;
        }
    }
    evidence(&ExternalEvidence {
        schema: ExternalEvidenceSchema::Current,
        plan_payload_digest: plan.payload_digest,
        producer: ExternalEvidenceProducer {
            name: producer.name.to_owned(),
            version: producer.version.to_owned(),
        },
        rows,
    })
    .map_err(|_defect| ProviderError::InvalidResponse)
}
