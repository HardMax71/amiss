use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::Arc;

use amiss_wire::de::{self, Error, ErrorKind, Obj, fail};
use amiss_wire::digest::Digest;
use amiss_wire::json::{Value, canonical};
use amiss_wire::model::ArtifactId;
use amiss_wire::report::{AnalysisErrorCode, ErrorDetail};

const INTERSPHINX_PRODUCER: &str = "sphinx-inventory-set";
const INTERSPHINX_VERSION: &str = "1";
const SPHINX_LABEL: &str = "sphinx-label";
const LABEL_BYTES: usize = 4_096;
const DESTINATION_BYTES: usize = 16_384;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Inputs {
    pub(crate) candidate_bindings: Vec<Digest>,
    pub(crate) labels: Arc<BTreeMap<String, InventoryLabel>>,
    pub(crate) provenance: Vec<Provenance>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum InventoryLabel {
    Unique(String),
    Ambiguous,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Provenance {
    pub payload_digest: Digest,
    pub producer_kind: ArtifactId,
    pub producer_identity: ArtifactId,
    pub producer_version: String,
    pub input_digest: Digest,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Context {
    pub(crate) labels: Arc<BTreeMap<String, InventoryLabel>>,
    pub(crate) provenance: Vec<Provenance>,
}

pub(crate) fn parse(values: &[Value]) -> Result<Inputs, Error> {
    let mut inputs = Inputs::default();
    let mut previous = None;
    let mut intersphinx = false;
    for (index, value) in values.iter().enumerate() {
        let path = format!("$.semantic_evidence[{index}]");
        let bytes = canonical(value);
        let envelope = amiss_wire::semantic::parse(&bytes)?;
        match previous.map(|digest: Digest| digest.cmp(&envelope.payload_digest)) {
            Some(Ordering::Equal) => {
                return fail("$.semantic_evidence", ErrorKind::DuplicateMember);
            }
            Some(Ordering::Greater) => {
                return fail("$.semantic_evidence", ErrorKind::UnsortedSet);
            }
            None | Some(Ordering::Less) => previous = Some(envelope.payload_digest),
        }
        let evidence = envelope.payload;
        let known = evidence.producer_kind.as_str() == INTERSPHINX_PRODUCER;
        if known && evidence.producer_version != INTERSPHINX_VERSION {
            return fail(
                &format!("{path}.payload.producer.version"),
                ErrorKind::InvalidValue,
            );
        }
        if known {
            if intersphinx || !evidence.complete || evidence.source_report_payload_digest.is_some()
            {
                return fail(&path, ErrorKind::Inconsistent);
            }
            intersphinx = true;
        }
        inputs
            .candidate_bindings
            .push(evidence.candidate_identity_digest);
        inputs.provenance.push(Provenance {
            payload_digest: envelope.payload_digest,
            producer_kind: evidence.producer_kind,
            producer_identity: evidence.producer_identity,
            producer_version: evidence.producer_version,
            input_digest: evidence.input_digest,
        });
        if known {
            for (observation_index, observation) in evidence.observations.into_iter().enumerate() {
                let observation_path = format!("{path}.payload.observations[{observation_index}]");
                if observation.text("kind") == Some(SPHINX_LABEL) {
                    insert_label(
                        Arc::make_mut(&mut inputs.labels),
                        &observation_path,
                        observation,
                    )?;
                }
            }
        }
    }
    Ok(inputs)
}

pub(crate) fn bind(inputs: &Inputs, candidate: Digest) -> Result<Context, ErrorDetail> {
    if inputs
        .candidate_bindings
        .iter()
        .any(|binding| *binding != candidate)
    {
        return Err(ErrorDetail {
            code: AnalysisErrorCode::ControlBindingMismatch,
            path: None,
            path_bytes: None,
            resource: None,
        });
    }
    Ok(Context {
        labels: inputs.labels.clone(),
        provenance: inputs.provenance.clone(),
    })
}

fn insert_label(
    labels: &mut BTreeMap<String, InventoryLabel>,
    path: &str,
    observation: Value,
) -> Result<(), Error> {
    let mut row = Obj::new(path, observation)?;
    row.required("kind", |path, value| {
        de::const_str(path, value, SPHINX_LABEL)
    })?;
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
            crate::resolve::http_destination_valid,
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

fn decode_id(path: &str, value: Value) -> Result<ArtifactId, Error> {
    ArtifactId::new(de::string(path, value)?)
        .ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
}

fn bounded_text(
    path: &str,
    value: Value,
    limit: usize,
    valid: impl FnOnce(&str) -> bool,
) -> Result<String, Error> {
    let text = de::string(path, value)?;
    if text.len() <= limit && valid(&text) {
        Ok(text)
    } else {
        fail(path, ErrorKind::InvalidValue)
    }
}
