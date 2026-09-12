use crate::codec;
use crate::de::{Error, ErrorKind};
use crate::json::Value;
use serde::Deserialize;

mod assessment;
mod document;
mod evidence;
mod plan;

pub use crate::report::ReportDefect as PlanDefect;
pub use assessment::{AssessDefect, assess};
pub use plan::plan;

pub const PLAN_ENVELOPE_SCHEMA: &str = "amiss/external-plan-envelope";
pub const PLAN_PAYLOAD_SCHEMA: &str = "amiss/external-plan-payload";
pub const EVIDENCE_SCHEMA: &str = "amiss/external-evidence";
pub const ASSESSMENT_ENVELOPE_SCHEMA: &str = "amiss/external-assessment-envelope";
pub const ASSESSMENT_PAYLOAD_SCHEMA: &str = "amiss/external-assessment-payload";

/// Whether the value is an external plan envelope whose payload matches its
/// recorded digest: the check a producer makes before spending calls on it.
#[must_use]
pub fn bound_plan(plan: &Value) -> bool {
    assessment::checked_plan(plan).is_ok()
}

/// Builds one producer's evidence under the digest recorded by its plan.
///
/// # Errors
///
/// The plan has no digest, or a producer or evidence row violates its contract.
pub fn evidence_file(
    plan: &Value,
    producer_name: &str,
    producer_version: &str,
    rows: Vec<Value>,
) -> Result<Value, Error> {
    #[derive(Deserialize)]
    struct PlanDigest {
        payload_digest: crate::digest::Digest,
    }
    let binding: PlanDigest = codec::from_value("$", plan)?;
    let rows = rows
        .into_iter()
        .enumerate()
        .map(|(index, value)| {
            let path = format!("$.rows[{index}]");
            let row: evidence::EvidenceRow = codec::from_value(&path, &value)?;
            row.check(&path)?;
            Ok(row)
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let evidence = evidence::Evidence {
        schema: EVIDENCE_SCHEMA.to_owned(),
        plan_payload_digest: binding.payload_digest,
        producer: evidence::Producer {
            name: producer_name.to_owned(),
            version: producer_version.to_owned(),
        },
        rows,
    };
    evidence.check_header()?;
    codec::to_value(&evidence)
}

/// Builds an HTTP observation, including redirect proof only when established.
///
/// # Errors
///
/// The method, outcome, destination, or observation instant is invalid.
pub fn probe_evidence_row(
    destination: &str,
    method: &str,
    status: Option<i64>,
    failure: Option<&str>,
    redirect: Option<(&str, bool)>,
    checked_at: &str,
) -> Result<Value, Error> {
    row_value(
        destination,
        checked_at,
        Observation::Probe {
            method,
            status,
            failure,
            redirect,
        },
    )
}

/// Builds forge visibility and optional resolution facts for a destination.
///
/// # Errors
///
/// The repository state, tail, destination, or observation instant is invalid.
pub fn forge_evidence_row(
    destination: &str,
    repository: &str,
    tail: Option<&str>,
    checked_at: &str,
) -> Result<Value, Error> {
    row_value(
        destination,
        checked_at,
        Observation::Forge { repository, tail },
    )
}

fn destination_valid(destination: &str) -> bool {
    !destination.is_empty() && destination.chars().count() <= 16_384
}

fn parse_enum<T: std::str::FromStr>(path: &str, value: &str) -> Result<T, Error> {
    value
        .parse()
        .map_err(|_defect| Error::new(path, ErrorKind::InvalidValue))
}

#[derive(Clone, Copy)]
enum Observation<'a> {
    Probe {
        method: &'a str,
        status: Option<i64>,
        failure: Option<&'a str>,
        redirect: Option<(&'a str, bool)>,
    },
    Forge {
        repository: &'a str,
        tail: Option<&'a str>,
    },
}

fn row_value(
    destination: &str,
    checked_at: &str,
    observation: Observation<'_>,
) -> Result<Value, Error> {
    let destination = destination.to_owned();
    let checked_at = checked_at.to_owned();
    let row = match observation {
        Observation::Probe {
            method,
            status,
            failure,
            redirect,
        } => {
            let (final_destination, redirect_chain_permanent) = match redirect {
                Some((destination, permanent)) => {
                    (Some(destination.to_owned()), permanent.then_some(true))
                }
                None => (None, None),
            };
            evidence::EvidenceRow::HttpProbe(evidence::Probe {
                destination,
                method: parse_enum("$.method", method)?,
                status,
                failure: failure
                    .map(|value| parse_enum("$.failure", value))
                    .transpose()?,
                final_destination,
                redirect_chain_permanent,
                checked_at,
            })
        }
        Observation::Forge { repository, tail } => {
            evidence::EvidenceRow::ForgeApi(evidence::Forge {
                destination,
                repository: parse_enum("$.repository", repository)?,
                tail: tail.map(|value| parse_enum("$.tail", value)).transpose()?,
                checked_at,
            })
        }
    };
    row.check("$")?;
    codec::to_value(&row)
}
