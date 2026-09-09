use amiss_wire::external::{ExternalEvidence, ExternalEvidenceRow};
use amiss_wire::report::{
    PAYLOAD_SCHEMA,
    model::{ObservationComparison, ReportEnvelope},
};

/// A minimal complete scanner report whose candidate side introduces the
/// given external destinations, digest-true, for producer and lane tests.
#[must_use]
pub fn external_report(destinations: &[&str]) -> Option<ReportEnvelope> {
    let mut report: ReportEnvelope = serde_json::from_slice(crate::SCANNER_REPORT).ok()?;
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
    report.payload_digest = amiss_wire::digest::hj_serde(PAYLOAD_SCHEMA, |mut writer| {
        serde_json_canonicalizer::to_writer(&report.payload, &mut writer)
    })
    .ok()?;
    Some(report)
}

/// Derives the external plan from the shared digest-true report fixture.
#[must_use]
pub fn external_plan(destinations: &[&str]) -> Option<amiss_wire::external::ExternalPlanEnvelope> {
    let report = external_report(destinations)?;
    amiss_wire::external::plan(
        &report,
        &report.payload.engine.engine_version,
        report.payload.engine.engine_digest,
    )
    .ok()
}

/// Flattens forge evidence rows into the facts provider tests compare.
#[must_use]
pub fn external_facts(evidence: &ExternalEvidence) -> Option<Vec<String>> {
    evidence
        .rows
        .iter()
        .map(|row| {
            let ExternalEvidenceRow::ForgeApi {
                destination,
                repository,
                tail,
                ..
            } = row
            else {
                return None;
            };
            Some(match tail {
                Some(tail) => format!("{destination} {repository} {tail}"),
                None => format!("{destination} {repository}"),
            })
        })
        .collect()
}
