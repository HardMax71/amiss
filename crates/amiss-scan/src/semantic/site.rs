mod tests;

use amiss_wire::json::ValueExt as _;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use amiss_wire::de::{Error, ErrorKind, fail};
use amiss_wire::digest::{Digest, hj};
use amiss_wire::json::Value;
use amiss_wire::model::{RepoPath, RepoPathText};

use super::decode::{DESTINATION_BYTES, LABEL_BYTES, bounded_text, sorted_set};
use super::{
    SiteClaim, SiteDefect, SiteEvaluation, SiteNavigation, SitePageBacking, SiteRoute, SiteTarget,
};

const SITE_ROUTE: &str = "site-route";
const SITE_GENERATED_ROUTE: &str = "site-generated-route";
const SITE_REDIRECT: &str = "site-redirect";
const SITE_NAVIGATION: &str = "site-navigation";
const SITE_CLAIM_DOMAIN: &str = "amiss/scanner-site-claim";
const SITE_DEFECT_DOMAIN: &str = "amiss/scanner-site-defect";

#[derive(serde::Deserialize)]
#[serde(tag = "kind", deny_unknown_fields, remote = "Self")]
enum SiteObservation {
    #[serde(rename = "site-route")]
    Route {
        route: String,
        source: RepoPathText,
        anchors: Vec<String>,
    },
    #[serde(rename = "site-generated-route")]
    GeneratedRoute {
        route: String,
        #[serde(deserialize_with = "amiss_wire::codec::nullable")]
        source: Option<RepoPathText>,
        anchors: Vec<String>,
    },
    #[serde(rename = "site-redirect")]
    Redirect {
        route: String,
        source: RepoPathText,
        destination: String,
    },
    #[serde(rename = "site-navigation")]
    Navigation {
        #[serde(deserialize_with = "amiss_wire::codec::nullable")]
        root: Option<RepoPathText>,
        manifest: RepoPathText,
        entrypoints: Vec<String>,
        reachable: Vec<RepoPathText>,
    },
}

impl<'de> serde::Deserialize<'de> for SiteObservation {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(amiss_wire::codec::object(deserializer))
    }
}

pub(super) fn site_build_inputs(
    routes: &mut Arc<BTreeMap<String, SiteRoute>>,
    path: &str,
    observations: Vec<Value>,
    item_count: &mut usize,
) -> Result<SiteEvaluation, Error> {
    let mut navigation = None;
    for (index, observation) in observations.into_iter().enumerate() {
        if !matches!(
            observation.text("kind"),
            Some(SITE_ROUTE | SITE_GENERATED_ROUTE | SITE_REDIRECT | SITE_NAVIGATION)
        ) {
            continue;
        }
        let observation_path = format!("{path}.payload.observations[{index}]");
        let decoded: SiteObservation =
            amiss_wire::codec::from_value(&observation_path, &observation)?;
        match decoded {
            SiteObservation::Navigation {
                root,
                manifest,
                entrypoints,
                reachable,
            } => {
                if navigation.is_some() {
                    return fail(&observation_path, ErrorKind::Inconsistent);
                }
                let checked = navigation_input(
                    &observation_path,
                    root.as_ref(),
                    &manifest,
                    entrypoints,
                    &reachable,
                    item_count,
                )?;
                navigation = Some((observation_path, checked));
            }
            SiteObservation::Route {
                route,
                source,
                anchors,
            } => {
                let digest = hj(SITE_CLAIM_DOMAIN, &observation);
                let claim = page_claim(
                    &observation_path,
                    &route,
                    Some(RepoPath::from(&source)),
                    SitePageBacking::Repository,
                    anchors,
                    digest,
                    item_count,
                )?;
                merge_site_claim(Arc::make_mut(routes), route, claim);
            }
            SiteObservation::GeneratedRoute {
                route,
                source,
                anchors,
            } => {
                let digest = hj(SITE_CLAIM_DOMAIN, &observation);
                let claim = page_claim(
                    &observation_path,
                    &route,
                    source.as_ref().map(RepoPath::from),
                    SitePageBacking::Generated,
                    anchors,
                    digest,
                    item_count,
                )?;
                merge_site_claim(Arc::make_mut(routes), route, claim);
            }
            SiteObservation::Redirect {
                route,
                source,
                destination,
            } => {
                route_text(&format!("{observation_path}.route"), &route)?;
                let target = redirect_target(
                    &format!("{observation_path}.destination"),
                    &route,
                    destination,
                )?;
                let claim = SiteClaim {
                    source: Some(RepoPath::from(&source)),
                    digest: hj(SITE_CLAIM_DOMAIN, &observation),
                    target,
                };
                merge_site_claim(Arc::make_mut(routes), route, claim);
            }
        }
    }
    let Some((navigation_path, navigation)) = navigation else {
        return Ok(SiteEvaluation {
            navigation: None,
            defects: site_defects(routes)?.into(),
        });
    };
    validate_navigation(routes, &navigation_path, &navigation)?;
    Ok(SiteEvaluation {
        navigation: Some(Arc::new(navigation)),
        defects: site_defects(routes)?.into(),
    })
}

fn navigation_input(
    path: &str,
    root: Option<&RepoPathText>,
    manifest: &RepoPathText,
    entrypoints: Vec<String>,
    reachable: &[RepoPathText],
    item_count: &mut usize,
) -> Result<SiteNavigation, Error> {
    let root = root.map(RepoPath::from);
    let manifest = RepoPath::from(manifest);
    sorted_set(&format!("{path}.entrypoints"), &entrypoints, item_count)?;
    sorted_set(&format!("{path}.reachable"), reachable, item_count)?;
    for (index, route) in entrypoints.iter().enumerate() {
        route_text(&format!("{path}.entrypoints[{index}]"), route)?;
    }
    let reachable: Vec<_> = reachable.iter().map(RepoPath::from).collect();
    if entrypoints.is_empty()
        || !navigation_contains(root.as_ref(), &manifest)
        || reachable
            .iter()
            .any(|source| !navigation_contains(root.as_ref(), source))
        || reachable.binary_search(&manifest).is_ok()
    {
        return fail(path, ErrorKind::Inconsistent);
    }
    Ok(SiteNavigation {
        root,
        manifest,
        entrypoints,
        reachable,
    })
}

fn route_text(path: &str, route: &str) -> Result<(), Error> {
    bounded_text(
        path,
        route,
        DESTINATION_BYTES,
        amiss_wire::uri::site_route_valid,
    )
}

fn page_claim(
    path: &str,
    route: &str,
    source: Option<RepoPath>,
    backing: SitePageBacking,
    anchors: Vec<String>,
    digest: Digest,
    item_count: &mut usize,
) -> Result<SiteClaim, Error> {
    route_text(&format!("{path}.route"), route)?;
    let anchors_path = format!("{path}.anchors");
    sorted_set(&anchors_path, &anchors, item_count)?;
    for (index, anchor) in anchors.iter().enumerate() {
        bounded_text(
            &format!("{anchors_path}[{index}]"),
            anchor,
            LABEL_BYTES,
            |value| !value.is_empty() && value.chars().all(|character| !character.is_control()),
        )?;
    }
    Ok(SiteClaim {
        source,
        digest,
        target: SiteTarget::Page { backing, anchors },
    })
}

fn redirect_target(path: &str, route: &str, mut destination: String) -> Result<SiteTarget, Error> {
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
            .is_some_and(|value| value.chars().any(char::is_control))
        || route == destination
    {
        return fail(path, ErrorKind::InvalidValue);
    }
    Ok(SiteTarget::Redirect {
        destination,
        fragment,
    })
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

fn site_defects(routes: &BTreeMap<String, SiteRoute>) -> Result<Vec<SiteDefect>, Error> {
    let mut defects = Vec::new();
    for (route, target) in routes {
        let defect = match target {
            SiteRoute::Ambiguous { sources, claims } => {
                Some(duplicate_route_defect(route, sources, claims)?)
            }
            SiteRoute::Unique(claim) => broken_redirect_defect(routes, route, claim)?,
        };
        if let Some(defect) = defect {
            defects.push(defect);
        }
    }
    Ok(defects)
}

fn duplicate_route_defect(
    route: &str,
    sources: &[RepoPath],
    claims: &[Digest],
) -> Result<SiteDefect, Error> {
    #[derive(serde::Serialize)]
    struct Evidence<'a> {
        claim_digests: &'a [Digest],
        kind: &'static str,
        route: &'a str,
        sources: &'a [RepoPath],
    }
    let evidence = amiss_wire::codec::to_value(&Evidence {
        claim_digests: claims,
        kind: "duplicate-route",
        route,
        sources,
    })?;
    Ok(SiteDefect {
        id: site_defect_id("duplicate-route", route)?,
        evidence,
        source: sources.first().cloned(),
        member_count: u64::try_from(claims.len()).unwrap_or(u64::MAX),
    })
}

fn broken_redirect_defect(
    routes: &BTreeMap<String, SiteRoute>,
    route: &str,
    claim: &SiteClaim,
) -> Result<Option<SiteDefect>, Error> {
    #[derive(serde::Serialize)]
    struct Evidence<'a> {
        claim_digest: Digest,
        destination: &'a str,
        kind: &'static str,
        reason: &'a str,
        route: &'a str,
        source: &'a RepoPath,
    }
    let SiteTarget::Redirect {
        destination,
        fragment,
    } = &claim.target
    else {
        return Ok(None);
    };
    let Some(source) = claim.source.as_ref() else {
        return Ok(None);
    };
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
            let Some(fragment) = fragment.as_deref().filter(|fragment| !fragment.is_empty()) else {
                return Ok(None);
            };
            if fragment_target(anchors, fragment) {
                return Ok(None);
            }
            "missing-anchor"
        }
    };
    let mut published = destination.clone();
    if let Some(fragment) = fragment {
        published.push('#');
        published.push_str(fragment);
    }
    let evidence = amiss_wire::codec::to_value(&Evidence {
        claim_digest: claim.digest,
        destination: &published,
        kind: "broken-redirect",
        reason,
        route,
        source,
    })?;
    Ok(Some(SiteDefect {
        id: site_defect_id("broken-redirect", route)?,
        evidence,
        source: Some(source.clone()),
        member_count: 1,
    }))
}

pub(crate) fn fragment_target(anchors: &[String], fragment: &str) -> bool {
    let published = |candidate: &str| {
        candidate.eq_ignore_ascii_case("top")
            || anchors
                .binary_search_by(|anchor| anchor.as_str().cmp(candidate))
                .is_ok()
    };
    published(fragment)
        || (fragment.as_bytes().contains(&b'%')
            && percent_encoding::percent_decode_str(fragment)
                .decode_utf8()
                .ok()
                .as_deref()
                .is_some_and(published))
}

fn site_defect_id(kind: &str, route: &str) -> Result<Digest, Error> {
    #[derive(serde::Serialize)]
    struct Identity<'a> {
        kind: &'a str,
        route: &'a str,
    }
    amiss_wire::codec::digest(SITE_DEFECT_DOMAIN, &Identity { kind, route })
}

fn validate_navigation(
    routes: &BTreeMap<String, SiteRoute>,
    path: &str,
    navigation: &SiteNavigation,
) -> Result<(), Error> {
    let page_sources: BTreeSet<&RepoPath> = routes
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

pub(crate) fn navigation_contains(root: Option<&RepoPath>, path: &RepoPath) -> bool {
    root.is_none_or(|root| {
        path.as_bytes()
            .strip_prefix(root.as_bytes())
            .is_some_and(|tail| tail.first() == Some(&b'/'))
    })
}
