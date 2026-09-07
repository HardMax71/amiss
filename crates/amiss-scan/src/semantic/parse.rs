use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::Arc;

use amiss_wire::de::{Error, ErrorKind, fail};
use amiss_wire::digest::Digest;
use amiss_wire::report::model::SemanticEvidenceProducer;
use amiss_wire::requests::SuppliedSemanticEvidence;
use amiss_wire::semantic::observation::{
    Observation, SITE_BUILD_VERSION, SPHINX_INVENTORY_VERSION, SphinxLabelObservation,
};
use amiss_wire::semantic::{SemanticEvidenceEnvelope, SemanticProducerKind};

use super::record::insert_record_set;
use super::site::site_build_inputs;
use super::{Inputs, InventoryLabel, Provenance};

mod tests;

const LABEL_BYTES: usize = 4_096;
const DESTINATION_BYTES: usize = 16_384;

pub(crate) fn parse<'a>(
    values: impl IntoIterator<Item = Result<SemanticEvidenceEnvelope<'a>, Error>>,
) -> Result<Inputs, Error> {
    let mut inputs = Inputs::default();
    let mut previous: Option<Digest> = None;
    let mut intersphinx = false;
    let mut site_build = false;
    let mut site_items = 0_usize;
    for (index, envelope) in values.into_iter().enumerate() {
        let path = format!("$.semantic_evidence[{index}]");
        let envelope = envelope?;
        match previous.map(|digest| digest.cmp(&envelope.payload_digest)) {
            Some(Ordering::Equal) => {
                return fail("$.semantic_evidence", ErrorKind::DuplicateMember);
            }
            Some(Ordering::Greater) => return fail("$.semantic_evidence", ErrorKind::UnsortedSet),
            None | Some(Ordering::Less) => previous = Some(envelope.payload_digest),
        }
        let amiss_wire::semantic::SemanticEvidence {
            schema: _schema,
            subject,
            producer,
            complete,
            observations,
        } = envelope.payload;
        let amiss_wire::semantic::SemanticSubject {
            candidate_identity_digest,
            source_report_payload_digest,
        } = subject;
        match producer.kind {
            SemanticProducerKind::SphinxInventorySet => {
                if producer.version != SPHINX_INVENTORY_VERSION {
                    return fail(
                        &format!("{path}.payload.producer.version"),
                        ErrorKind::InvalidValue,
                    );
                }
                if intersphinx
                    || !complete
                    || source_report_payload_digest != amiss_wire::assessment::Nullable::Null
                {
                    return fail(&path, ErrorKind::Inconsistent);
                }
                intersphinx = true;
                insert_labels(Arc::make_mut(&mut inputs.labels), &path, observations)?;
            }
            SemanticProducerKind::SiteBuild => {
                if producer.version != SITE_BUILD_VERSION {
                    return fail(
                        &format!("{path}.payload.producer.version"),
                        ErrorKind::InvalidValue,
                    );
                }
                if site_build || !complete {
                    return fail(&path, ErrorKind::Inconsistent);
                }
                site_build = true;
                inputs.site =
                    site_build_inputs(&mut inputs.routes, &path, observations, &mut site_items)?;
            }
            SemanticProducerKind::RecordSet => {
                if producer.version != amiss_wire::semantic::record::PRODUCER_VERSION {
                    return fail(
                        &format!("{path}.payload.producer.version"),
                        ErrorKind::InvalidValue,
                    );
                }
                insert_record_set(
                    Arc::make_mut(&mut inputs.record_sets),
                    &path,
                    source_report_payload_digest,
                    complete,
                    observations,
                )?;
            }
        }
        inputs.candidate_bindings.push(candidate_identity_digest);
        inputs.provenance.push(Provenance {
            payload_digest: envelope.payload_digest,
            producer: SemanticEvidenceProducer {
                kind: producer.kind,
                identity: producer.identity,
                version: producer.version,
                input_digest: producer.input_digest,
            },
        });
    }
    Ok(inputs)
}

pub(crate) fn validated_envelope(
    supplied: SuppliedSemanticEvidence,
    path: &str,
) -> Result<SemanticEvidenceEnvelope<'static>, Error> {
    let mut counter = countio::Counter::new(std::io::sink());
    serde_json::to_writer(&mut counter, &supplied.value)
        .map_err(|_defect| Error::new(path, ErrorKind::InvalidValue))?;
    if u64::try_from(counter.writer_bytes()).unwrap_or(u64::MAX)
        > amiss_wire::semantic::SEMANTIC_EVIDENCE_BYTES
    {
        return fail("$", ErrorKind::LimitExceeded);
    }
    let envelope = supplied.value;
    amiss_wire::semantic::validate(&envelope)?;
    if envelope.payload.producer.context_digest != supplied.expected_context_digest {
        return fail(
            &format!("{path}.expected_context_digest"),
            ErrorKind::DigestMismatch,
        );
    }
    Ok(envelope)
}

fn insert_labels(
    labels: &mut BTreeMap<String, InventoryLabel>,
    path: &str,
    observations: Vec<Cow<'_, Observation>>,
) -> Result<(), Error> {
    for (index, observation) in observations.into_iter().enumerate() {
        let path = format!("{path}.payload.observations[{index}]");
        let Observation::Sphinx(SphinxLabelObservation {
            kind: _kind,
            inventory: _inventory,
            name,
            destination,
        }) = observation.into_owned()
        else {
            return fail(&path, ErrorKind::Inconsistent);
        };
        let normalized = amiss_rst::normalized_label(&name);
        if name.is_empty()
            || name.len() > LABEL_BYTES
            || name.chars().any(char::is_control)
            || normalized.is_empty()
        {
            return fail(&format!("{path}.name"), ErrorKind::InvalidValue);
        }
        if destination.len() > DESTINATION_BYTES
            || !amiss_wire::uri::http_destination_valid(&destination)
        {
            return fail(&format!("{path}.destination"), ErrorKind::InvalidValue);
        }
        labels
            .entry(normalized)
            .and_modify(|label| *label = InventoryLabel::Ambiguous)
            .or_insert(InventoryLabel::Unique(destination));
    }
    Ok(())
}
