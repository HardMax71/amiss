use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::Arc;

use amiss_wire::de::{Error, ErrorKind, fail};
use amiss_wire::digest::Digest;
use amiss_wire::json::{Value, canonical};
use amiss_wire::requests::SuppliedSemanticEvidence;

use super::decode::{DESTINATION_BYTES, LABEL_BYTES, bounded_text, decode_id, observation_row};
use super::record::insert_record_set;
use super::site::site_build_inputs;
use super::{Inputs, InventoryLabel, Provenance};

const INTERSPHINX_PRODUCER: &str = "sphinx-inventory-set";
const INTERSPHINX_VERSION: &str = "1";
const SPHINX_LABEL: &str = "sphinx-label";
const SITE_BUILD_PRODUCER: &str = "site-build";
const SITE_BUILD_VERSION: &str = "0.5.1";
const RECORD_SET_PRODUCER: &str = "record-set";
const RECORD_SET_VERSION: &str = "1";

pub(crate) fn parse(values: &[SuppliedSemanticEvidence]) -> Result<Inputs, Error> {
    let mut inputs = Inputs::default();
    let mut previous = None;
    let mut intersphinx = false;
    let mut site_build = false;
    let mut site_items = 0_usize;
    for (index, supplied) in values.iter().enumerate() {
        let path = format!("$.semantic_evidence[{index}]");
        let bytes = canonical(&supplied.value);
        let envelope = amiss_wire::semantic::parse(&bytes)?;
        if envelope.payload.context_digest != supplied.expected_context_digest {
            return fail(
                &format!("{path}.expected_context_digest"),
                ErrorKind::DigestMismatch,
            );
        }
        match previous.map(|digest: Digest| digest.cmp(&envelope.payload_digest)) {
            Some(Ordering::Equal) => {
                return fail("$.semantic_evidence", ErrorKind::DuplicateMember);
            }
            Some(Ordering::Greater) => {
                return fail("$.semantic_evidence", ErrorKind::UnsortedSet);
            }
            None | Some(Ordering::Less) => previous = Some(envelope.payload_digest),
        }
        let amiss_wire::semantic::SemanticEvidence {
            candidate_identity_digest,
            source_report_payload_digest,
            producer_kind,
            producer_identity,
            producer_version,
            context_digest: _context_digest,
            input_digest,
            complete,
            observations,
        } = envelope.payload;
        match producer_kind.as_str() {
            INTERSPHINX_PRODUCER => {
                if producer_version != INTERSPHINX_VERSION {
                    return fail(
                        &format!("{path}.payload.producer.version"),
                        ErrorKind::InvalidValue,
                    );
                }
                if intersphinx || !complete || source_report_payload_digest.is_some() {
                    return fail(&path, ErrorKind::Inconsistent);
                }
                intersphinx = true;
                for (observation_index, observation) in observations.into_iter().enumerate() {
                    let observation_path =
                        format!("{path}.payload.observations[{observation_index}]");
                    if observation.text("kind") == Some(SPHINX_LABEL) {
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
            RECORD_SET_PRODUCER => {
                if producer_version != RECORD_SET_VERSION {
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

fn insert_label(
    labels: &mut BTreeMap<String, InventoryLabel>,
    path: &str,
    observation: Value,
) -> Result<(), Error> {
    let mut row = observation_row(path, observation, SPHINX_LABEL)?;
    let _inventory = row.required("inventory", decode_id)?;
    let name = row.required("name", |path, value| {
        bounded_text(path, value, LABEL_BYTES, |label| {
            !label.is_empty() && label.chars().all(|character| !character.is_control())
        })
    })?;
    let destination = row.required("destination", |path, value| {
        bounded_text(
            path,
            value,
            DESTINATION_BYTES,
            amiss_wire::uri::http_destination_valid,
        )
    })?;
    row.finish()?;
    let normalized = amiss_rst::normalized_label(&name);
    if normalized.is_empty() {
        return fail(&format!("{path}.name"), ErrorKind::InvalidValue);
    }
    labels
        .entry(normalized)
        .and_modify(|label| *label = InventoryLabel::Ambiguous)
        .or_insert(InventoryLabel::Unique(destination));
    Ok(())
}
