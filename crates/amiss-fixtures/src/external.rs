use amiss_wire::report::{
    PAYLOAD_SCHEMA,
    model::{ObservationComparison, ReportEnvelope},
};
use serde_json::Value;
use sha2::Digest as _;

const REPORT: &[u8] = include_bytes!("../../../spec/examples/scanner-report.canonical.json");

/// A minimal complete scanner report whose candidate side introduces the
/// given external destinations, digest-true, for producer and lane tests.
#[must_use]
pub fn external_report(destinations: &[&str]) -> Option<Vec<u8>> {
    let mut report: ReportEnvelope = serde_json::from_slice(REPORT).ok()?;
    let local = report
        .payload
        .observations
        .iter()
        .find(|row| {
            row.candidate
                .as_ref()
                .is_some_and(|candidate| candidate.external_destination.is_none())
        })?
        .clone();
    let external = report
        .payload
        .observations
        .iter()
        .find(|row| {
            row.candidate
                .as_ref()
                .is_some_and(|candidate| candidate.external_destination.is_some())
        })?
        .clone();
    let rows = destinations
        .iter()
        .enumerate()
        .map(|(index, destination)| {
            let (scheme, _tail) = destination.split_once(':')?;
            if scheme.is_empty() {
                return None;
            }
            let mut row: ObservationComparison = external.clone();
            let candidate = row.candidate.as_mut()?;
            candidate.external_destination = Some((*destination).to_owned());
            candidate.intent.external_scheme = Some(scheme.to_owned());
            candidate.intent.raw_destination_digest = amiss_wire::model::Digest::from(
                sha2::Sha256::new_with_prefix("amiss/fixture-destination")
                    .chain_update([0_u8])
                    .chain_update(destination.as_bytes())
                    .finalize()
                    .0,
            );
            candidate.observation_id_input.extracted_intent = candidate.intent.clone();
            candidate
                .observation_id_input
                .structural_address
                .construct_index = u64::try_from(index).ok()?.saturating_add(1);
            candidate.observation_id = amiss_wire::model::Digest::from(
                sha2::Sha256::new_with_prefix("amiss/fixture-observation")
                    .chain_update([0_u8])
                    .chain_update(destination.as_bytes())
                    .finalize()
                    .0,
            );
            Some(row)
        })
        .collect::<Option<Vec<_>>>()?;
    let count = u64::try_from(rows.len()).ok()?;
    report.payload.observations = std::iter::once(local).chain(rows).collect();
    report.payload.summary.references.explicit_local = 1;
    report.payload.summary.references.external_out_of_scope = count;
    report.payload.summary.references.extracted = count.saturating_add(1);
    report.payload.summary.references.resolved = 1;
    let payload = serde_json_canonicalizer::to_vec(&report.payload).ok()?;
    report.payload_digest = amiss_wire::model::Digest::from(
        sha2::Sha256::new_with_prefix(PAYLOAD_SCHEMA)
            .chain_update([0_u8])
            .chain_update(&payload)
            .finalize()
            .0,
    );
    serde_json_canonicalizer::to_vec(&report).ok()
}

/// Derives the external plan from the shared digest-true report fixture.
#[must_use]
pub fn external_plan(destinations: &[&str]) -> Option<Vec<u8>> {
    let report = external_report(destinations)?;
    let parsed = serde_json::from_slice::<Value>(&report).ok()?;
    let engine = parsed.get("payload")?.get("engine")?;
    amiss_wire::external::plan(
        &report,
        engine.get("engine_version").and_then(Value::as_str)?,
        amiss_wire::model::Digest::from_wire(engine.get("engine_digest").and_then(Value::as_str)?)?,
    )
    .ok()
}

/// Flattens forge evidence rows into the facts provider tests compare.
#[must_use]
pub fn external_facts(evidence: &[u8]) -> Option<Vec<String>> {
    let evidence = serde_json::from_slice::<Value>(evidence).ok()?;
    let Value::Array(rows) = evidence.get("rows")? else {
        return None;
    };
    rows.iter()
        .map(|row| {
            let destination = row.get("destination").and_then(Value::as_str)?;
            let repository = row.get("repository").and_then(Value::as_str)?;
            Some(match row.get("tail").and_then(Value::as_str) {
                Some(tail) => format!("{destination} {repository} {tail}"),
                None => format!("{destination} {repository}"),
            })
        })
        .collect()
}
