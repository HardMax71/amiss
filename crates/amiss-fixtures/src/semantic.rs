use amiss_wire::assessment::Nullable;
use amiss_wire::digest::{Digest, hb};
use amiss_wire::model::{ArtifactId, RepoPathText};
use amiss_wire::report::{
    PAYLOAD_SCHEMA,
    model::{Controls, ReportEnvelope, SemanticEvidenceProducer, SemanticEvidenceProvenance},
};
use amiss_wire::semantic::observation::{Observation, SiteBuildObservation};

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

/// A typed site-build observation fixture.
#[derive(Clone, Copy)]
pub enum SiteObservation<'a> {
    Page(&'a str, &'a [&'a str]),
    Generated(Option<&'a str>, &'a [&'a str]),
    Redirect(&'a str, &'a str),
}

/// Builds one site-build semantic observation.
///
/// # Errors
///
/// The source path is invalid.
pub fn site_observation(
    route: &str,
    observation: SiteObservation<'_>,
) -> Result<Observation, &'static str> {
    Ok(Observation::Site(match observation {
        SiteObservation::Page(source, anchors) => SiteBuildObservation::Route {
            route: route.to_owned(),
            source: source.parse()?,
            anchors: anchors.iter().map(|anchor| (*anchor).to_owned()).collect(),
        },
        SiteObservation::Generated(source, anchors) => SiteBuildObservation::GeneratedRoute {
            route: route.to_owned(),
            source: source
                .map(str::parse::<RepoPathText>)
                .transpose()?
                .map_or(Nullable::Null, Nullable::Value),
            anchors: anchors.iter().map(|anchor| (*anchor).to_owned()).collect(),
        },
        SiteObservation::Redirect(source, destination) => SiteBuildObservation::Redirect {
            route: route.to_owned(),
            source: source.parse()?,
            destination: destination.to_owned(),
        },
    }))
}
