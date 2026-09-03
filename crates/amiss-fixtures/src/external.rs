use amiss_wire::json::Value;
use amiss_wire::report::{
    PAYLOAD_SCHEMA,
    model::{ObservationComparison, ReportEnvelope},
};

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
            candidate.intent.raw_destination_digest =
                amiss_wire::digest::hb("amiss/fixture-destination", destination.as_bytes());
            candidate.observation_id_input.extracted_intent = candidate.intent.clone();
            candidate
                .observation_id_input
                .structural_address
                .construct_index = u64::try_from(index).ok()?.saturating_add(1);
            candidate.observation_id =
                amiss_wire::digest::hb("amiss/fixture-observation", destination.as_bytes());
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
    report.payload_digest = amiss_wire::digest::hb(PAYLOAD_SCHEMA, &payload);
    serde_json_canonicalizer::to_vec(&report).ok()
}

/// Derives the external plan from the shared digest-true report fixture.
#[must_use]
pub fn external_plan(destinations: &[&str]) -> Option<Value> {
    let report = external_report(destinations)?;
    let parsed = amiss_wire::json::parse(&report).ok()?;
    let engine = parsed.member("payload")?.member("engine")?;
    amiss_wire::external::plan(
        &parsed,
        engine.text("engine_version")?,
        amiss_wire::digest::Digest::from_wire(engine.text("engine_digest")?)?,
    )
    .ok()
}

/// Flattens forge evidence rows into the facts provider tests compare.
#[must_use]
pub fn external_facts(evidence: &Value) -> Option<Vec<String>> {
    let Value::Array(rows) = evidence.member("rows")? else {
        return None;
    };
    rows.iter()
        .map(|row| {
            let destination = row.text("destination")?;
            let repository = row.text("repository")?;
            Some(match row.text("tail") {
                Some(tail) => format!("{destination} {repository} {tail}"),
                None => format!("{destination} {repository}"),
            })
        })
        .collect()
}
