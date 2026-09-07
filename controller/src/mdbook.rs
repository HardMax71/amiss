use std::collections::BTreeMap;

use amiss_controller_files::read_bounded_at;
use amiss_wire::assessment::Nullable;
use amiss_wire::digest::{Digest, hb};
use amiss_wire::model::RepoPathText;
use amiss_wire::semantic::observation::{Observation, SiteBuildObservation};
use amiss_wire::semantic::{PayloadSchema, SemanticProducer, SemanticSubject};
use cap_std::fs::Dir;
use serde::Deserialize as _;

mod context;
mod html;
mod model;

use context::{BuildPages, pages, render_context, site_build_context};
use html::{page_facts, reachable_sources};

pub const MDBOOK_RENDER_CONTEXT_BYTES: u64 = 16_777_216;
pub const MDBOOK_HTML_BYTES: u64 = 16_777_216;
pub(super) const MDBOOK_VERSION: &str = "0.5.4";
const INPUT_DOMAIN: &str = "amiss/controller-mdbook-site-input-v1";
const HTML_DOMAIN: &str = "amiss/controller-mdbook-html-v1";

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct SiteBuildContext {
    pub configuration: RepoPathText,
    pub route_prefix: String,
    pub locale: Option<String>,
    pub version: Option<String>,
}

#[derive(serde::Serialize)]
struct SiteInput<'a> {
    mdbook_version: &'static str,
    context_digest: Digest,
    config_digest: Digest,
    navigation: &'a SiteBuildObservation,
    pages: &'a [SiteInputPage],
}

#[derive(serde::Serialize)]
struct SiteInputPage {
    route: String,
    source: Option<RepoPathText>,
    html_digest: Digest,
}

struct CollectedPages {
    observations: Vec<SiteBuildObservation>,
    inputs: Vec<SiteInputPage>,
    links: BTreeMap<String, Vec<String>>,
    anchor_count: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum MdBookEvidenceError {
    #[error("the mdBook renderer context exceeds its byte ceiling")]
    ContextBytes,
    #[error("the mdBook renderer context is not strict JSON")]
    Context(#[source] amiss_wire::json::Error),
    #[error("the mdBook renderer context has an invalid shape")]
    ContextShape,
    #[error("the mdBook build is not supported by this producer")]
    UnsupportedBuild,
    #[error("an mdBook source or output path is invalid")]
    Path,
    #[error("the mdBook route prefix or a resulting route is invalid")]
    Route,
    #[error("the mdBook configuration, locale, or version identity is invalid")]
    ContextIdentity,
    #[error("the completed mdBook HTML output cannot be read")]
    Output(#[source] std::io::Error),
    #[error("a published mdBook anchor is invalid or exceeds its ceiling")]
    Anchor,
    #[error("the completed mdBook navigation graph is invalid or exceeds its ceiling")]
    Navigation,
    #[error("the mdBook evidence exceeds the semantic wire contract")]
    Evidence,
}

/// Produces one complete candidate-bound route, anchor, and navigation table
/// from a pinned mdBook renderer context and its completed HTML output.
///
/// The site context locates the exact `book.toml` inside the candidate tree
/// and the HTML output inside the published site. The open output directory
/// remains the caller's capability and is never inferred from
/// repository-controlled context paths.
///
/// # Errors
///
/// The context, renderer version, HTML renderer selection, source mapping,
/// output path, page bytes, anchor set, link graph, or resulting evidence is
/// invalid, ambiguous, incomplete, or outside a fixed ceiling.
pub fn mdbook_site_evidence(
    candidate_identity_digest: Digest,
    site: &SiteBuildContext,
    context_bytes: &[u8],
    html_output: &Dir,
) -> Result<Vec<u8>, MdBookEvidenceError> {
    if u64::try_from(context_bytes.len()).unwrap_or(u64::MAX) > MDBOOK_RENDER_CONTEXT_BYTES {
        return Err(MdBookEvidenceError::ContextBytes);
    }
    amiss_wire::json::parse(context_bytes).map_err(MdBookEvidenceError::Context)?;
    let mut deserializer = serde_json::Deserializer::from_slice(context_bytes);
    // The strict JSON gate has already enforced the document depth ceiling.
    deserializer.disable_recursion_limit();
    let context = model::RenderContext::deserialize(&mut deserializer)
        .map_err(|_defect| MdBookEvidenceError::ContextShape)?;
    let (expectation, base, repository_book_root) = site_build_context(site)?;
    let (source_directory, items, config_digest) = render_context(&context)?;
    let build = pages(
        items,
        &source_directory,
        repository_book_root.as_deref(),
        &base,
    )?;
    if build.rows.len() >= amiss_wire::semantic::SEMANTIC_OBSERVATIONS_LIMIT {
        return Err(MdBookEvidenceError::UnsupportedBuild);
    }
    let CollectedPages {
        mut observations,
        inputs,
        links,
        anchor_count,
    } = collect_pages(&build, html_output)?;
    let reachable = reachable_sources(&build.entrypoint, &links, &build.rows)?;
    if anchor_count
        .checked_add(reachable.len())
        .and_then(|count| count.checked_add(1))
        .is_none_or(|count| count > amiss_wire::semantic::SEMANTIC_OBSERVATIONS_LIMIT)
    {
        return Err(MdBookEvidenceError::Navigation);
    }
    let navigation = navigation_observation(&build, reachable)?;
    observations.push(navigation.clone());

    let input_digest = serde_json_canonicalizer::to_vec(&SiteInput {
        mdbook_version: MDBOOK_VERSION,
        context_digest: expectation.context_digest,
        config_digest,
        navigation: &navigation,
        pages: &inputs,
    })
    .map(|canonical| hb(INPUT_DOMAIN, &canonical))
    .map_err(|_defect| MdBookEvidenceError::Evidence)?;
    let document = amiss_wire::semantic::envelope(amiss_wire::semantic::SemanticEvidence {
        schema: PayloadSchema::Current,
        subject: SemanticSubject {
            candidate_identity_digest,
            source_report_payload_digest: Nullable::Null,
        },
        producer: SemanticProducer {
            kind: expectation.producer_kind,
            identity: expectation.producer_identity,
            version: expectation.producer_version,
            context_digest: expectation.context_digest,
            input_digest,
        },
        complete: true,
        observations: observations
            .into_iter()
            .map(Observation::Site)
            .map(std::borrow::Cow::Owned)
            .collect(),
    })
    .map_err(|_defect| MdBookEvidenceError::Evidence)?;
    let mut bytes = Vec::new();
    amiss_wire::semantic::write(&document, &mut bytes)
        .map_err(|_defect| MdBookEvidenceError::Evidence)?;
    Ok(bytes)
}

fn collect_pages(
    build: &BuildPages,
    html_output: &Dir,
) -> Result<CollectedPages, MdBookEvidenceError> {
    let mut remaining = MDBOOK_HTML_BYTES;
    let mut href_count = 0_usize;
    let mut collected = CollectedPages {
        observations: Vec::with_capacity(build.rows.len().saturating_add(1)),
        inputs: Vec::with_capacity(build.rows.len()),
        links: BTreeMap::new(),
        anchor_count: 0,
    };
    for (route, page) in &build.rows {
        let html = read_bounded_at(html_output, &page.output, remaining)
            .map_err(MdBookEvidenceError::Output)?;
        remaining = remaining
            .checked_sub(u64::try_from(html.len()).unwrap_or(u64::MAX))
            .ok_or(MdBookEvidenceError::Output(std::io::Error::other(
                "aggregate HTML byte ceiling exceeded",
            )))?;
        let (anchors, destinations) = page_facts(
            &html,
            route,
            &build.rows,
            &mut collected.anchor_count,
            &mut href_count,
        )?;
        collected.links.insert(route.clone(), destinations);
        let source = page
            .source
            .as_ref()
            .map(|source| RepoPathText::new(source.clone()).ok_or(MdBookEvidenceError::Path))
            .transpose()?;
        collected.inputs.push(SiteInputPage {
            route: route.clone(),
            source: source.clone(),
            html_digest: hb(HTML_DOMAIN, &html),
        });
        collected.observations.push(match source {
            None => SiteBuildObservation::GeneratedRoute {
                route: route.clone(),
                source: Nullable::Null,
                anchors,
            },
            Some(source) => SiteBuildObservation::Route {
                route: route.clone(),
                source,
                anchors,
            },
        });
    }
    Ok(collected)
}

/// Freezes the operator-owned site identity that acquired evidence must match.
///
/// # Errors
///
/// The configuration is not a repository `book.toml`, the publication prefix
/// is not an absolute site route, or a locale/version identity is invalid.
pub fn mdbook_site_expectation(
    site: &SiteBuildContext,
) -> Result<crate::SemanticEvidenceExpectation, MdBookEvidenceError> {
    site_build_context(site).map(|(expectation, _base, _root)| expectation)
}

fn navigation_observation(
    build: &BuildPages,
    reachable: Vec<String>,
) -> Result<SiteBuildObservation, MdBookEvidenceError> {
    let root = build
        .source_root
        .as_ref()
        .map(|root| RepoPathText::new(root.clone()).ok_or(MdBookEvidenceError::Path))
        .transpose()?
        .map_or(Nullable::Null, Nullable::Value);
    let manifest = RepoPathText::new(build.manifest.clone()).ok_or(MdBookEvidenceError::Path)?;
    let reachable = reachable
        .into_iter()
        .map(|source| RepoPathText::new(source).ok_or(MdBookEvidenceError::Path))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(SiteBuildObservation::Navigation {
        root,
        manifest,
        entrypoints: vec![build.entrypoint.clone()],
        reachable,
    })
}
