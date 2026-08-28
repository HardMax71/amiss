use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use amiss_wire::de::{self, Error, ErrorKind, fail};
use amiss_wire::digest::{Digest, hj};
use amiss_wire::json::Value;

use super::decode::{
    DESTINATION_BYTES, LABEL_BYTES, bounded_text, observation_row, repo_path, sorted_set,
};
use super::{
    SiteClaim, SiteDefect, SiteEvaluation, SiteNavigation, SitePageBacking, SiteRoute, SiteTarget,
};

const SITE_ROUTE: &str = "site-route";
const SITE_GENERATED_ROUTE: &str = "site-generated-route";
const SITE_REDIRECT: &str = "site-redirect";
const SITE_NAVIGATION: &str = "site-navigation";
const SITE_CLAIM_DOMAIN: &str = "amiss/scanner-site-claim";
const SITE_DEFECT_DOMAIN: &str = "amiss/scanner-site-defect";

pub(super) fn site_build_inputs(
    routes: &mut Arc<BTreeMap<String, SiteRoute>>,
    path: &str,
    observations: Vec<Value>,
    item_count: &mut usize,
) -> Result<SiteEvaluation, Error> {
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
        } else if let Some((route, claim)) = site_claim(&observation_path, observation, item_count)?
        {
            merge_site_claim(Arc::make_mut(routes), route, claim);
        }
    }
    let Some((navigation_path, navigation)) = navigation else {
        return Ok(SiteEvaluation {
            navigation: None,
            defects: site_defects(routes).into(),
        });
    };
    validate_navigation(routes, &navigation_path, &navigation)?;
    Ok(SiteEvaluation {
        navigation: Some(Arc::new(navigation)),
        defects: site_defects(routes).into(),
    })
}

fn site_claim(
    path: &str,
    observation: Value,
    anchor_count: &mut usize,
) -> Result<Option<(String, SiteClaim)>, Error> {
    let (kind, backing) = match observation.text("kind") {
        Some(SITE_ROUTE) => (SITE_ROUTE, Some(SitePageBacking::Repository)),
        Some(SITE_GENERATED_ROUTE) => (SITE_GENERATED_ROUTE, Some(SitePageBacking::Generated)),
        Some(SITE_REDIRECT) => (SITE_REDIRECT, None),
        Some(_) | None => return Ok(None),
    };
    let digest = hj(SITE_CLAIM_DOMAIN, &observation);
    let mut row = observation_row(path, observation, kind)?;
    let route = row.required("route", |path, value| {
        bounded_text(
            path,
            value,
            DESTINATION_BYTES,
            amiss_wire::uri::site_route_valid,
        )
    })?;
    let source = row.required("source", |path, value| match backing {
        Some(SitePageBacking::Generated) => de::nullable(value)
            .map(|value| repo_path(path, value))
            .transpose(),
        Some(SitePageBacking::Repository) | None => repo_path(path, value).map(Some),
    })?;
    let target = if let Some(backing) = backing {
        SiteTarget::Page {
            backing,
            anchors: row.required("anchors", |path, value| {
                sorted_set(path, value, anchor_count, |path, value| {
                    bounded_text(path, value, LABEL_BYTES, |value| {
                        !value.is_empty() && value.chars().all(|character| !character.is_control())
                    })
                })
            })?,
        }
    } else {
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
        SiteTarget::Redirect {
            destination,
            fragment,
        }
    };
    row.finish()?;
    Ok(Some((
        route,
        SiteClaim {
            source,
            digest,
            target,
        },
    )))
}

fn merge_site_claim(routes: &mut BTreeMap<String, SiteRoute>, route: String, claim: SiteClaim) {
    match routes.entry(route) {
        std::collections::btree_map::Entry::Vacant(entry) => {
            entry.insert(SiteRoute::Unique(claim));
        }
        std::collections::btree_map::Entry::Occupied(mut entry) => {
            let source = claim.source.clone();
            let claim = claim.digest;
            let existing = entry.get_mut();
            let (mut sources, mut claims) = match existing {
                SiteRoute::Ambiguous { sources, claims } => {
                    if let Some(source) = source
                        && let Err(index) = sources.binary_search(&source)
                    {
                        sources.insert(index, source);
                    }
                    if let Err(index) = claims.binary_search(&claim) {
                        claims.insert(index, claim);
                    }
                    return;
                }
                SiteRoute::Unique(existing) => (
                    [existing.source.clone(), source]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>(),
                    vec![existing.digest, claim],
                ),
            };
            sources.sort();
            sources.dedup();
            claims.sort();
            claims.dedup();
            *existing = SiteRoute::Ambiguous { sources, claims };
        }
    }
}

fn site_defects(routes: &BTreeMap<String, SiteRoute>) -> Vec<SiteDefect> {
    routes
        .iter()
        .filter_map(|(route, target)| match target {
            SiteRoute::Ambiguous { sources, claims } => {
                Some(duplicate_route_defect(route, sources, claims))
            }
            SiteRoute::Unique(claim) => broken_redirect_defect(routes, route, claim),
        })
        .collect()
}

fn duplicate_route_defect(
    route: &str,
    sources: &[amiss_wire::model::RepoPath],
    claims: &[Digest],
) -> SiteDefect {
    let evidence = Value::object(vec![
        (
            "claim_digests".to_owned(),
            Value::array(
                claims
                    .iter()
                    .map(|claim| Value::string(claim.to_string()))
                    .collect(),
            ),
        ),
        (
            "kind".to_owned(),
            Value::string("duplicate-route".to_owned()),
        ),
        ("route".to_owned(), Value::string(route.to_owned())),
        (
            "sources".to_owned(),
            Value::array(
                sources
                    .iter()
                    .map(amiss_wire::model::RepoPath::to_value)
                    .collect(),
            ),
        ),
    ]);
    SiteDefect {
        id: site_defect_id("duplicate-route", route),
        evidence,
        source: sources.first().cloned(),
        member_count: u64::try_from(claims.len()).unwrap_or(u64::MAX),
    }
}

fn broken_redirect_defect(
    routes: &BTreeMap<String, SiteRoute>,
    route: &str,
    claim: &SiteClaim,
) -> Option<SiteDefect> {
    let SiteTarget::Redirect {
        destination,
        fragment,
    } = &claim.target
    else {
        return None;
    };
    let source = claim.source.as_ref()?;
    let reason = match routes.get(destination) {
        None => "missing-route",
        Some(SiteRoute::Ambiguous { .. }) => "ambiguous-route",
        Some(SiteRoute::Unique(SiteClaim {
            target: SiteTarget::Redirect { .. },
            ..
        })) => "nonterminal-redirect",
        Some(SiteRoute::Unique(SiteClaim {
            target: SiteTarget::Page { anchors, .. },
            ..
        })) => {
            let fragment = fragment
                .as_deref()
                .filter(|fragment| !fragment.is_empty())?;
            let decoded = amiss_wire::uri::decode_fragment(fragment)?;
            if anchors.binary_search(&decoded).is_ok() {
                return None;
            }
            "missing-anchor"
        }
    };
    let mut published = destination.clone();
    if let Some(fragment) = fragment {
        published.push('#');
        published.push_str(fragment);
    }
    let evidence = Value::object(vec![
        (
            "claim_digest".to_owned(),
            Value::string(claim.digest.to_string()),
        ),
        ("destination".to_owned(), Value::string(published)),
        (
            "kind".to_owned(),
            Value::string("broken-redirect".to_owned()),
        ),
        ("reason".to_owned(), Value::string(reason.to_owned())),
        ("route".to_owned(), Value::string(route.to_owned())),
        ("source".to_owned(), source.to_value()),
    ]);
    Some(SiteDefect {
        id: site_defect_id("broken-redirect", route),
        evidence,
        source: Some(source.clone()),
        member_count: 1,
    })
}

fn site_defect_id(kind: &str, route: &str) -> Digest {
    hj(
        SITE_DEFECT_DOMAIN,
        &Value::object(vec![
            ("kind".to_owned(), Value::string(kind.to_owned())),
            ("route".to_owned(), Value::string(route.to_owned())),
        ]),
    )
}

fn validate_navigation(
    routes: &BTreeMap<String, SiteRoute>,
    path: &str,
    navigation: &SiteNavigation,
) -> Result<(), Error> {
    let page_sources: BTreeSet<&amiss_wire::model::RepoPath> = routes
        .values()
        .filter_map(|route| match route {
            SiteRoute::Unique(SiteClaim {
                source,
                target:
                    SiteTarget::Page {
                        backing: SitePageBacking::Repository,
                        ..
                    },
                ..
            }) => source.as_ref(),
            SiteRoute::Unique(SiteClaim {
                target:
                    SiteTarget::Page {
                        backing: SitePageBacking::Generated,
                        ..
                    }
                    | SiteTarget::Redirect { .. },
                ..
            })
            | SiteRoute::Ambiguous { .. } => None,
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
        let Some(SiteRoute::Unique(SiteClaim {
            source,
            target: SiteTarget::Page { backing, .. },
            ..
        })) = routes.get(entrypoint)
        else {
            return fail(path, ErrorKind::Inconsistent);
        };
        if *backing == SitePageBacking::Repository {
            let Some(source) = source else {
                return fail(path, ErrorKind::Inconsistent);
            };
            if navigation.reachable.binary_search(source).is_err() {
                return fail(path, ErrorKind::Inconsistent);
            }
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
