use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use amiss_wire::assessment::Nullable;
use amiss_wire::de::{Error, ErrorKind, fail};
use amiss_wire::digest::{Digest, hj_serde};
use amiss_wire::model::RepoPath;
use amiss_wire::report::model::{
    BrokenRedirectFactEvidenceKind, BrokenRedirectReason, DuplicateRouteFactEvidenceKind,
    FindingFactEvidence,
};
use amiss_wire::semantic::observation::{Observation, SiteBuildObservation};

use super::{
    SiteClaim, SiteDefect, SiteEvaluation, SiteNavigation, SitePageBacking, SiteRoute, SiteTarget,
};

const LABEL_BYTES: usize = 4_096;
const DESTINATION_BYTES: usize = 16_384;
const SITE_CLAIM_DOMAIN: &str = "amiss/scanner-site-claim";
const SITE_DEFECT_DOMAIN: &str = "amiss/scanner-site-defect";

#[derive(serde::Serialize)]
struct SiteDefectIdentity<'a, K> {
    kind: K,
    route: &'a str,
}

pub(super) fn site_build_inputs(
    routes: &mut Arc<BTreeMap<String, SiteRoute>>,
    path: &str,
    observations: Vec<Cow<'_, Observation>>,
    item_count: &mut usize,
) -> Result<SiteEvaluation, Error> {
    let mut navigation = None;
    for (index, observation) in observations.into_iter().enumerate() {
        let observation_path = format!("{path}.payload.observations[{index}]");
        let Observation::Site(observation) = observation.into_owned() else {
            return fail(&observation_path, ErrorKind::Inconsistent);
        };
        let digest = hj_serde(SITE_CLAIM_DOMAIN, |mut writer| {
            serde_json_canonicalizer::to_writer(&observation, &mut writer)
        })
        .map_err(|_defect| Error::new(&observation_path, ErrorKind::InvalidValue))?;
        match observation {
            SiteBuildObservation::Navigation {
                root,
                manifest,
                entrypoints,
                reachable,
            } => {
                if navigation.is_some() {
                    return fail(&observation_path, ErrorKind::Inconsistent);
                }
                validate_routes(
                    &format!("{observation_path}.entrypoints"),
                    &entrypoints,
                    item_count,
                )?;
                validate_sorted(
                    &format!("{observation_path}.reachable"),
                    &reachable,
                    item_count,
                )?;
                let root = match root {
                    Nullable::Value(root) => Some(RepoPath::from(&root)),
                    Nullable::Null => None,
                };
                let manifest = RepoPath::from(&manifest);
                let reachable = reachable.iter().map(RepoPath::from).collect::<Vec<_>>();
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
            }
            claim @ (SiteBuildObservation::Route { .. }
            | SiteBuildObservation::GeneratedRoute { .. }
            | SiteBuildObservation::Redirect { .. }) => {
                let (route, claim) = site_claim(&observation_path, digest, claim, item_count)?;
                merge_site_claim(Arc::make_mut(routes), route, claim);
            }
        }
    }
    let navigation = navigation
        .map(|(navigation_path, navigation)| {
            validate_navigation(routes, &navigation_path, &navigation)
                .map(|()| Arc::new(navigation))
        })
        .transpose()?;
    let defects =
        site_defects(routes).map_err(|_defect| Error::new(path, ErrorKind::InvalidValue))?;
    Ok(SiteEvaluation {
        navigation,
        defects: defects.into(),
    })
}

fn site_claim(
    path: &str,
    digest: Digest,
    observation: SiteBuildObservation,
    anchor_count: &mut usize,
) -> Result<(String, SiteClaim), Error> {
    let (route, source, target) = match observation {
        SiteBuildObservation::Route {
            route,
            source,
            anchors,
        } => {
            validate_route(&format!("{path}.route"), &route)?;
            validate_anchors(&format!("{path}.anchors"), &anchors, anchor_count)?;
            (
                route,
                Some(RepoPath::from(&source)),
                SiteTarget::Page {
                    backing: SitePageBacking::Repository,
                    anchors,
                },
            )
        }
        SiteBuildObservation::GeneratedRoute {
            route,
            source,
            anchors,
        } => {
            validate_route(&format!("{path}.route"), &route)?;
            validate_anchors(&format!("{path}.anchors"), &anchors, anchor_count)?;
            (
                route,
                match source {
                    Nullable::Value(source) => Some(RepoPath::from(&source)),
                    Nullable::Null => None,
                },
                SiteTarget::Page {
                    backing: SitePageBacking::Generated,
                    anchors,
                },
            )
        }
        SiteBuildObservation::Redirect {
            route,
            source,
            mut destination,
        } => {
            validate_route(&format!("{path}.route"), &route)?;
            if destination.len() > DESTINATION_BYTES {
                return fail(&format!("{path}.destination"), ErrorKind::InvalidValue);
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
                return fail(&format!("{path}.destination"), ErrorKind::InvalidValue);
            }
            (
                route,
                Some(RepoPath::from(&source)),
                SiteTarget::Redirect {
                    destination,
                    fragment,
                },
            )
        }
        SiteBuildObservation::Navigation { .. } => {
            return fail(path, ErrorKind::Inconsistent);
        }
    };
    Ok((
        route,
        SiteClaim {
            source,
            digest,
            target,
        },
    ))
}

fn validate_route(path: &str, route: &str) -> Result<(), Error> {
    (route.len() <= DESTINATION_BYTES && amiss_wire::uri::site_route_valid(route))
        .then_some(())
        .ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
}

fn validate_routes(path: &str, routes: &[String], item_count: &mut usize) -> Result<(), Error> {
    validate_sorted(path, routes, item_count)?;
    for (index, route) in routes.iter().enumerate() {
        validate_route(&format!("{path}[{index}]"), route)?;
    }
    Ok(())
}

fn validate_anchors(path: &str, anchors: &[String], item_count: &mut usize) -> Result<(), Error> {
    validate_sorted(path, anchors, item_count)?;
    for (index, anchor) in anchors.iter().enumerate() {
        if anchor.is_empty() || anchor.len() > LABEL_BYTES || anchor.chars().any(char::is_control) {
            return fail(&format!("{path}[{index}]"), ErrorKind::InvalidValue);
        }
    }
    Ok(())
}

fn validate_sorted<T: Ord>(path: &str, values: &[T], item_count: &mut usize) -> Result<(), Error> {
    *item_count = item_count
        .checked_add(values.len())
        .filter(|count| *count <= amiss_wire::semantic::SEMANTIC_OBSERVATIONS_LIMIT)
        .ok_or_else(|| Error::new(path, ErrorKind::LimitExceeded))?;
    values.windows(2).try_for_each(|pair| match pair {
        [left, right] if left < right => Ok(()),
        [left, right] if left == right => fail(path, ErrorKind::DuplicateMember),
        [_left, _right] => fail(path, ErrorKind::UnsortedSet),
        _ => Ok(()),
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

fn site_defects(routes: &BTreeMap<String, SiteRoute>) -> serde_json::Result<Vec<SiteDefect>> {
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
    sources: &[RepoPath],
    claims: &[Digest],
) -> serde_json::Result<SiteDefect> {
    let evidence = FindingFactEvidence::DuplicateRoute {
        claim_digests: claims.to_vec(),
        kind: DuplicateRouteFactEvidenceKind::DuplicateRoute,
        route: route.to_owned(),
        sources: sources.to_vec(),
    };
    Ok(SiteDefect {
        id: site_defect_id(DuplicateRouteFactEvidenceKind::DuplicateRoute, route)?,
        evidence,
        source: sources.first().cloned(),
        member_count: u64::try_from(claims.len()).unwrap_or(u64::MAX),
    })
}

fn broken_redirect_defect(
    routes: &BTreeMap<String, SiteRoute>,
    route: &str,
    claim: &SiteClaim,
) -> Option<serde_json::Result<SiteDefect>> {
    let SiteTarget::Redirect {
        destination,
        fragment,
    } = &claim.target
    else {
        return None;
    };
    let source = claim.source.as_ref()?;
    let reason = match routes.get(destination) {
        None => BrokenRedirectReason::MissingRoute,
        Some(SiteRoute::Ambiguous { .. }) => BrokenRedirectReason::AmbiguousRoute,
        Some(SiteRoute::Unique(SiteClaim {
            target: SiteTarget::Redirect { .. },
            ..
        })) => BrokenRedirectReason::NonterminalRedirect,
        Some(SiteRoute::Unique(SiteClaim {
            target: SiteTarget::Page { anchors, .. },
            ..
        })) => {
            let fragment = fragment
                .as_deref()
                .filter(|fragment| !fragment.is_empty())?;
            if fragment_target(anchors, fragment) {
                return None;
            }
            BrokenRedirectReason::MissingAnchor
        }
    };
    let mut published = destination.clone();
    if let Some(fragment) = fragment {
        published.push('#');
        published.push_str(fragment);
    }
    let evidence = FindingFactEvidence::BrokenRedirect {
        claim_digest: claim.digest,
        destination: published,
        kind: BrokenRedirectFactEvidenceKind::BrokenRedirect,
        reason,
        route: route.to_owned(),
        source: source.clone(),
    };
    Some(
        site_defect_id(BrokenRedirectFactEvidenceKind::BrokenRedirect, route).map(|id| {
            SiteDefect {
                id,
                evidence,
                source: Some(source.clone()),
                member_count: 1,
            }
        }),
    )
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

fn site_defect_id(kind: impl serde::Serialize, route: &str) -> serde_json::Result<Digest> {
    hj_serde(SITE_DEFECT_DOMAIN, |mut writer| {
        serde_json_canonicalizer::to_writer(&SiteDefectIdentity { kind, route }, &mut writer)
    })
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
