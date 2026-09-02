use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::Arc;

use amiss_wire::de::{Error, ErrorKind, fail};
use amiss_wire::digest::Digest;
use amiss_wire::json::canonical;
use amiss_wire::requests::SuppliedSemanticEvidence;
use amiss_wire::semantic::SemanticEvidenceEnvelope;
use amiss_wire::semantic::observation::{
    SITE_BUILD_PRODUCER, SITE_BUILD_VERSION, SPHINX_INVENTORY_PRODUCER, SPHINX_INVENTORY_VERSION,
    SPHINX_LABEL, SphinxLabelObservation,
};

use super::record::insert_record_set;
use super::site::site_build_inputs;
use super::{Inputs, InventoryLabel, Provenance};

const LABEL_BYTES: usize = 4_096;
const DESTINATION_BYTES: usize = 16_384;

pub(crate) fn parse(values: &[SuppliedSemanticEvidence]) -> Result<Inputs, Error> {
    let mut inputs = Inputs::default();
    let mut previous = None;
    let mut intersphinx = false;
    let mut site_build = false;
    let mut site_items = 0_usize;
    for (index, supplied) in values.iter().enumerate() {
        let path = format!("$.semantic_evidence[{index}]");
        let envelope = validated_envelope(supplied, &path, &mut previous)?;
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
        let amiss_wire::semantic::SemanticProducer {
            kind: producer_kind,
            identity: producer_identity,
            version: producer_version,
            context_digest: _context_digest,
            input_digest,
        } = producer;
        match producer_kind.as_str() {
            SPHINX_INVENTORY_PRODUCER => {
                if producer_version != SPHINX_INVENTORY_VERSION {
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
                for (observation_index, observation) in observations.into_iter().enumerate() {
                    let observation_path =
                        format!("{path}.payload.observations[{observation_index}]");
                    if observation.get("kind").and_then(serde_json::Value::as_str)
                        == Some(SPHINX_LABEL)
                    {
                        insert_label(
                            Arc::make_mut(&mut inputs.labels),
                            &observation_path,
                            observation,
                        )?;
                    }
                }
            }
            SITE_BUILD_PRODUCER => {
                if producer_version != SITE_BUILD_VERSION {
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
            amiss_wire::semantic::record::PRODUCER_KIND => {
                if producer_version != amiss_wire::semantic::record::PRODUCER_VERSION {
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
            _ => {}
        }
        inputs.candidate_bindings.push(candidate_identity_digest);
        inputs.provenance.push(Provenance {
            payload_digest: envelope.payload_digest,
            producer_kind,
            producer_identity,
            producer_version,
            input_digest,
        });
    }
    Ok(inputs)
}

fn validated_envelope(
    supplied: &SuppliedSemanticEvidence,
    path: &str,
    previous: &mut Option<Digest>,
) -> Result<SemanticEvidenceEnvelope, Error> {
    let envelope = amiss_wire::semantic::parse(&canonical(&supplied.value))?;
    if envelope.payload.producer.context_digest != supplied.expected_context_digest {
        return fail(
            &format!("{path}.expected_context_digest"),
            ErrorKind::DigestMismatch,
        );
    }
    match previous.map(|digest| digest.cmp(&envelope.payload_digest)) {
        Some(Ordering::Equal) => fail("$.semantic_evidence", ErrorKind::DuplicateMember),
        Some(Ordering::Greater) => fail("$.semantic_evidence", ErrorKind::UnsortedSet),
        None | Some(Ordering::Less) => {
            *previous = Some(envelope.payload_digest);
            Ok(envelope)
        }
    }
}

fn insert_label(
    labels: &mut BTreeMap<String, InventoryLabel>,
    path: &str,
    observation: serde_json::Value,
) -> Result<(), Error> {
    let SphinxLabelObservation {
        kind: _kind,
        inventory: _inventory,
        name,
        destination,
    } = amiss_wire::de::deserialize_value(path, observation)?;
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
    Ok(())
}
