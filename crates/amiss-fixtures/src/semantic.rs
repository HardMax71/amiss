use amiss_wire::digest::{Digest, hb};
use amiss_wire::model::ArtifactId;
use amiss_wire::report::{
    PAYLOAD_SCHEMA,
    model::{Controls, ReportEnvelope, SemanticEvidenceProducer, SemanticEvidenceProvenance},
};

const REPORT: &[u8] = include_bytes!("../../../spec/examples/scanner-report.canonical.json");

/// Builds a digest-true passing report over the supplied semantic payloads.
#[must_use]
pub fn semantic_report(payload_digests: &[Digest]) -> Option<Vec<u8>> {
    let mut report: ReportEnvelope = serde_json::from_slice(REPORT).ok()?;
    let Controls::Resolved(controls) = &mut report.payload.controls else {
        return None;
    };
    let identity = ArtifactId::new("fixture".to_owned())?;
    controls.semantic_evidence = Some(
        payload_digests
            .iter()
            .copied()
            .map(|payload_digest| SemanticEvidenceProvenance {
                payload_digest,
                producer: SemanticEvidenceProducer {
                    identity: identity.clone(),
                    input_digest: payload_digest,
                    kind: amiss_wire::semantic::SemanticProducerKind::RecordSet,
                    version: "1".to_owned(),
                },
            })
            .collect(),
    );
    let payload = serde_json_canonicalizer::to_vec(&report.payload).ok()?;
    report.payload_digest = hb(PAYLOAD_SCHEMA, &payload);
    serde_json_canonicalizer::to_vec(&report).ok()
}

/// Builds one record-set semantic observation.
#[must_use]
pub fn record_set(name: &str, records: &[(&str, &str)]) -> serde_json::Value {
    serde_json::json!({
        "kind": "record-set",
        "name": name,
        "records": records
            .iter()
            .map(|(key, value)| serde_json::json!({"key": key, "value": value}))
            .collect::<Vec<_>>()
    })
}

/// A typed site-build observation fixture.
#[derive(Clone, Copy)]
pub enum SiteObservation<'a> {
    Page(&'a str, &'a [&'a str]),
    Generated(Option<&'a str>, &'a [&'a str]),
    Redirect(&'a str, &'a str),
}

/// Builds one site-build semantic observation.
#[must_use]
pub fn site_observation(route: &str, observation: SiteObservation<'_>) -> serde_json::Value {
    match observation {
        SiteObservation::Page(source, anchors) => serde_json::json!({
            "kind": "site-route",
            "route": route,
            "source": source,
            "anchors": anchors,
        }),
        SiteObservation::Generated(source, anchors) => serde_json::json!({
            "kind": "site-generated-route",
            "route": route,
            "source": source,
            "anchors": anchors,
        }),
        SiteObservation::Redirect(source, destination) => serde_json::json!({
            "kind": "site-redirect",
            "route": route,
            "source": source,
            "destination": destination,
        }),
    }
}

/// Builds one canonical site navigation observation.
#[must_use]
pub fn site_navigation(
    root: Option<&str>,
    manifest: &str,
    entrypoints: &[&str],
    reachable: &[&str],
) -> serde_json::Value {
    serde_json::json!({
        "kind": "site-navigation",
        "root": root,
        "manifest": manifest,
        "entrypoints": entrypoints,
        "reachable": reachable,
    })
}
