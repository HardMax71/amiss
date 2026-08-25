use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
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
const SITE_NAVIGATION: &str = "site-navigation";
const LABEL_BYTES: usize = 4_096;
const DESTINATION_BYTES: usize = 16_384;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Inputs {
    pub(crate) candidate_bindings: Vec<Digest>,
    pub(crate) labels: Arc<BTreeMap<String, InventoryLabel>>,
    pub(crate) routes: Arc<BTreeMap<String, SiteRoute>>,
    pub(crate) navigation: Option<Arc<SiteNavigation>>,
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
        fragment: Option<String>,
    },
    Ambiguous,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SiteNavigation {
    pub(crate) root: Option<amiss_wire::model::RepoPath>,
    pub(crate) manifest: amiss_wire::model::RepoPath,
    pub(crate) entrypoints: Vec<String>,
    pub(crate) reachable: Vec<amiss_wire::model::RepoPath>,
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
    pub(crate) navigation: Option<Arc<SiteNavigation>>,
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
    let mut site_items = 0_usize;
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
                inputs.navigation =
                    site_build_inputs(&mut inputs.routes, &path, observations, &mut site_items)?;
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

fn site_build_inputs(
    routes: &mut Arc<BTreeMap<String, SiteRoute>>,
    path: &str,
    observations: Vec<Value>,
    item_count: &mut usize,
) -> Result<Option<Arc<SiteNavigation>>, Error> {
    let mut navigation = None;
    for (index, observation) in observations.into_iter().enumerate() {
        let observation_path = format!("{path}.payload.observations[{index}]");
        if observation.text("kind") == Some(SITE_NAVIGATION) {
            if navigation.is_some() {
                return fail(&observation_path, ErrorKind::Inconsistent);
            }
            let mut row = observation_row(&observation_path, observation, SITE_NAVIGATION)?;
            let root = row.required("root", |path, value| match de::nullable(value) {
                None => Ok(None),
                Some(value) => repo_path(path, value).map(Some),
            })?;
            let manifest = row.required("manifest", repo_path)?;
            let entrypoints = row.required("entrypoints", |path, value| {
                sorted_set(path, value, item_count, |path, value| {
                    bounded_text(
                        path,
                        value,
                        DESTINATION_BYTES,
                        amiss_wire::uri::site_route_valid,
                    )
                })
            })?;
            let reachable = row.required("reachable", |path, value| {
                sorted_set(path, value, item_count, repo_path)
            })?;
            row.finish()?;
            if entrypoints.is_empty()
                || reachable.is_empty()
                || !navigation_contains(root.as_ref(), &manifest)
                || reachable
                    .iter()
                    .any(|source| !navigation_contains(root.as_ref(), source))
                || reachable.binary_search(&manifest).is_ok()
            {
                return fail(&observation_path, ErrorKind::Inconsistent);
            }
            navigation = Some((
                observation_path,
                SiteNavigation {
                    root,
                    manifest,
                    entrypoints,
                    reachable,
                },
            ));
        } else {
            insert_site(
                Arc::make_mut(routes),
                &observation_path,
                observation,
                item_count,
            )?;
        }
    }
    let Some((navigation_path, navigation)) = navigation else {
        return Ok(None);
    };
    validate_navigation(routes, &navigation_path, &navigation)?;
    Ok(Some(Arc::new(navigation)))
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
        navigation: inputs.navigation.clone(),
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
    let mut row = observation_row(path, observation, kind)?;
    let route = row.required("route", |path, value| {
        bounded_text(
            path,
            value,
            DESTINATION_BYTES,
            amiss_wire::uri::site_route_valid,
        )
    })?;
    let target = if kind == SITE_ROUTE {
        site_page(&mut row, anchor_count)?
    } else {
        site_redirect(path, &mut row, &route)?
    };
    row.finish()?;
    insert_or_ambiguous(routes, route, target, SiteRoute::Ambiguous);
    Ok(())
}

fn site_page(row: &mut Obj, anchor_count: &mut usize) -> Result<SiteRoute, Error> {
    let source = row.required("source", repo_path)?;
    let anchors = row.required("anchors", |path, value| {
        sorted_set(path, value, anchor_count, |path, value| {
            bounded_text(path, value, LABEL_BYTES, |value| {
                !value.is_empty() && value.chars().all(|character| !character.is_control())
            })
        })
    })?;
    Ok(SiteRoute::Page { source, anchors })
}

fn site_redirect(path: &str, row: &mut Obj, route: &str) -> Result<SiteRoute, Error> {
    let (destination, fragment) = row.required("destination", |path, value| {
        let mut destination = de::string(path, value)?;
        if destination.len() > DESTINATION_BYTES {
            return fail(path, ErrorKind::InvalidValue);
        }
        let fragment = destination.find('#').and_then(|separator| {
            let fragment = destination.get(separator.saturating_add(1)..)?.to_owned();
            destination.truncate(separator);
            Some(fragment)
        });
        if !amiss_wire::uri::site_route_valid(&destination)
            || fragment
                .as_deref()
                .is_some_and(|value| amiss_wire::uri::decode_fragment(value).is_none())
        {
            return fail(path, ErrorKind::InvalidValue);
        }
        Ok((destination, fragment))
    })?;
    if route == destination {
        return fail(&format!("{path}.destination"), ErrorKind::InvalidValue);
    }
    Ok(SiteRoute::Redirect {
        destination,
        fragment,
    })
}

fn validate_navigation(
    routes: &BTreeMap<String, SiteRoute>,
    path: &str,
    navigation: &SiteNavigation,
) -> Result<(), Error> {
    let page_sources: BTreeSet<&amiss_wire::model::RepoPath> = routes
        .values()
        .filter_map(|route| match route {
            SiteRoute::Page { source, .. } => Some(source),
            SiteRoute::Redirect { .. } | SiteRoute::Ambiguous => None,
        })
        .collect();
    if navigation
        .reachable
        .iter()
        .any(|source| !page_sources.contains(source))
    {
        return fail(path, ErrorKind::Inconsistent);
    }
    for entrypoint in &navigation.entrypoints {
        let Some(SiteRoute::Page { source, .. }) = routes.get(entrypoint) else {
            return fail(path, ErrorKind::Inconsistent);
        };
        if navigation.reachable.binary_search(source).is_err() {
            return fail(path, ErrorKind::Inconsistent);
        }
    }
    Ok(())
}

pub(crate) fn navigation_contains(
    root: Option<&amiss_wire::model::RepoPath>,
    path: &amiss_wire::model::RepoPath,
) -> bool {
    root.is_none_or(|root| {
        path.as_bytes()
            .strip_prefix(root.as_bytes())
            .is_some_and(|tail| tail.first() == Some(&b'/'))
    })
}

fn repo_path(path: &str, value: Value) -> Result<amiss_wire::model::RepoPath, Error> {
    amiss_wire::model::RepoPath::new(de::string(path, value)?)
        .ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
}

fn sorted_set<T: Ord>(
    path: &str,
    value: Value,
    item_count: &mut usize,
    mut decode: impl FnMut(&str, Value) -> Result<T, Error>,
) -> Result<Vec<T>, Error> {
    let values = de::array(path, value)?;
    *item_count = item_count
        .checked_add(values.len())
        .filter(|count| *count <= amiss_wire::semantic::SEMANTIC_OBSERVATIONS_LIMIT)
        .ok_or_else(|| Error::new(path, ErrorKind::LimitExceeded))?;
    let mut items: Vec<T> = Vec::with_capacity(values.len());
    for (index, value) in values.into_iter().enumerate() {
        let item = decode(&format!("{path}[{index}]"), value)?;
        match items.last().map(|previous| previous.cmp(&item)) {
            Some(Ordering::Equal) => return fail(path, ErrorKind::DuplicateMember),
            Some(Ordering::Greater) => return fail(path, ErrorKind::UnsortedSet),
            None | Some(Ordering::Less) => items.push(item),
        }
    }
    Ok(items)
}

fn observation_row(path: &str, observation: Value, kind: &str) -> Result<Obj, Error> {
    let mut row = Obj::new(path, observation)?;
    row.required("kind", |path, value| de::const_str(path, value, kind))?;
    Ok(row)
}

fn insert_or_ambiguous<K: Ord, V>(values: &mut BTreeMap<K, V>, key: K, unique: V, ambiguous: V) {
    values
        .entry(key)
        .and_modify(|value| *value = ambiguous)
        .or_insert(unique);
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
    insert_or_ambiguous(
        labels,
        normalized,
        InventoryLabel::Unique(destination),
        InventoryLabel::Ambiguous,
    );
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
