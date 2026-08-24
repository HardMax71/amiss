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
const SITE_BUILD_PRODUCER: &str = "site-build";
const SITE_BUILD_VERSION: &str = "0.1.0";
const SITE_ROUTE: &str = "site-route";
const SITE_REDIRECT: &str = "site-redirect";
const LABEL_BYTES: usize = 4_096;
const DESTINATION_BYTES: usize = 16_384;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Inputs {
    pub(crate) candidate_bindings: Vec<Digest>,
    pub(crate) labels: Arc<BTreeMap<String, InventoryLabel>>,
    pub(crate) routes: Arc<BTreeMap<String, SiteRoute>>,
    pub(crate) provenance: Vec<Provenance>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum InventoryLabel {
    Unique(String),
    Ambiguous,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SiteRoute {
    Page {
        source: amiss_wire::model::RepoPath,
        anchors: Vec<String>,
    },
    Redirect {
        destination: String,
    },
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
    pub(crate) routes: Arc<BTreeMap<String, SiteRoute>>,
    pub(crate) provenance: Vec<Provenance>,
}

#[derive(Clone, Copy)]
pub(crate) struct View<'a> {
    pub(crate) labels: &'a BTreeMap<String, InventoryLabel>,
    pub(crate) routes: Option<&'a BTreeMap<String, SiteRoute>>,
}

pub(crate) fn parse(values: &[Value]) -> Result<Inputs, Error> {
    let mut inputs = Inputs::default();
    let mut previous = None;
    let mut intersphinx = false;
    let mut site_build = false;
    let mut site_anchors = 0_usize;
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
        let amiss_wire::semantic::SemanticEvidence {
            candidate_identity_digest,
            source_report_payload_digest,
            producer_kind,
            producer_identity,
            producer_version,
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
                for (observation_index, observation) in observations.into_iter().enumerate() {
                    let observation_path =
                        format!("{path}.payload.observations[{observation_index}]");
                    insert_site(
                        Arc::make_mut(&mut inputs.routes),
                        &observation_path,
                        observation,
                        &mut site_anchors,
                    )?;
                }
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
        routes: inputs.routes.clone(),
        provenance: inputs.provenance.clone(),
    })
}

fn insert_site(
    routes: &mut BTreeMap<String, SiteRoute>,
    path: &str,
    observation: Value,
    anchor_count: &mut usize,
) -> Result<(), Error> {
    let kind = match observation.text("kind") {
        Some(SITE_ROUTE) => SITE_ROUTE,
        Some(SITE_REDIRECT) => SITE_REDIRECT,
        Some(_) | None => return Ok(()),
    };
    let mut row = Obj::new(path, observation)?;
    row.required("kind", |path, value| de::const_str(path, value, kind))?;
    let route = row.required("route", |path, value| {
        bounded_text(
            path,
            value,
            DESTINATION_BYTES,
            amiss_wire::uri::site_route_valid,
        )
    })?;
    let target = if kind == SITE_ROUTE {
        let source = row.required("source", |path, value| {
            amiss_wire::model::RepoPath::new(de::string(path, value)?)
                .ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
        })?;
        let anchors_path = row.field("anchors");
        let raw_anchors = de::array(&anchors_path, row.take("anchors")?)?;
        *anchor_count = anchor_count
            .checked_add(raw_anchors.len())
            .filter(|count| *count <= amiss_wire::semantic::SEMANTIC_OBSERVATIONS_LIMIT)
            .ok_or_else(|| Error::new(&anchors_path, ErrorKind::LimitExceeded))?;
        let mut anchors = Vec::with_capacity(raw_anchors.len());
        for (index, value) in raw_anchors.into_iter().enumerate() {
            let item_path = format!("{anchors_path}[{index}]");
            let anchor = bounded_text(&item_path, value, LABEL_BYTES, |value| {
                !value.is_empty() && value.chars().all(|character| !character.is_control())
            })?;
            match anchors.last().map(String::as_str) {
                Some(previous) if previous == anchor => {
                    return fail(&anchors_path, ErrorKind::DuplicateMember);
                }
                Some(previous) if previous > anchor.as_str() => {
                    return fail(&anchors_path, ErrorKind::UnsortedSet);
                }
                None | Some(_) => anchors.push(anchor),
            }
        }
        SiteRoute::Page { source, anchors }
    } else {
        let destination = row.required("destination", |path, value| {
            bounded_text(
                path,
                value,
                DESTINATION_BYTES,
                amiss_wire::uri::site_route_valid,
            )
        })?;
        if route == destination {
            return fail(&format!("{path}.destination"), ErrorKind::InvalidValue);
        }
        SiteRoute::Redirect { destination }
    };
    row.finish()?;
    routes
        .entry(route)
        .and_modify(|target| *target = SiteRoute::Ambiguous)
        .or_insert(target);
    Ok(())
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
