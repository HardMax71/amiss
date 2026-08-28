use std::collections::BTreeMap;

use amiss_controller_files::read_bounded_at;
use amiss_wire::digest::{Digest, hb, hj};
use amiss_wire::json::Value;
use amiss_wire::model::RepoPathText;
use cap_std::fs::Dir;

mod context;
mod html;

use context::{BuildPages, pages, render_context, site_build_context};
use html::{page_facts, reachable_sources};

pub const MDBOOK_RENDER_CONTEXT_BYTES: u64 = 16_777_216;
pub const MDBOOK_HTML_BYTES: u64 = 16_777_216;
pub(super) const MDBOOK_VERSION: &str = "0.5.4";
const INPUT_DOMAIN: &str = "amiss/controller-mdbook-site-input-v1";
const HTML_DOMAIN: &str = "amiss/controller-mdbook-html-v1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SiteBuildContext {
    pub configuration: RepoPathText,
    pub route_prefix: String,
    pub locale: Option<String>,
    pub version: Option<String>,
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
) -> Result<Value, MdBookEvidenceError> {
    if u64::try_from(context_bytes.len()).unwrap_or(u64::MAX) > MDBOOK_RENDER_CONTEXT_BYTES {
        return Err(MdBookEvidenceError::ContextBytes);
    }
    let context = amiss_wire::json::parse(context_bytes).map_err(MdBookEvidenceError::Context)?;
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

    let mut remaining = MDBOOK_HTML_BYTES;
    let mut anchor_count = 0_usize;
    let mut href_count = 0_usize;
    let mut links = BTreeMap::new();
    let mut observations = Vec::with_capacity(build.rows.len().saturating_add(1));
    let mut inputs = Vec::with_capacity(build.rows.len());
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
            &mut anchor_count,
            &mut href_count,
        )?;
        links.insert(route.clone(), destinations);
        let html_digest = Value::string(hb(HTML_DOMAIN, &html).to_string());
        let (kind, source) = page.source.as_ref().map_or_else(
            || ("site-generated-route", Value::Null),
            |source| ("site-route", Value::string(source.clone())),
        );
        inputs.push(Value::object(vec![
            ("route".to_owned(), Value::string(route.clone())),
            ("source".to_owned(), source.clone()),
            ("html_digest".to_owned(), html_digest),
        ]));
        observations.push(Value::object(vec![
            ("kind".to_owned(), Value::string(kind.to_owned())),
            ("route".to_owned(), Value::string(route.clone())),
            ("source".to_owned(), source),
            (
                "anchors".to_owned(),
                Value::array(anchors.into_iter().map(Value::string).collect()),
            ),
        ]));
    }
    let reachable = reachable_sources(&build.entrypoint, &links, &build.rows)?;
    if anchor_count
        .checked_add(reachable.len())
        .and_then(|count| count.checked_add(1))
        .is_none_or(|count| count > amiss_wire::semantic::SEMANTIC_OBSERVATIONS_LIMIT)
    {
        return Err(MdBookEvidenceError::Navigation);
    }
    let navigation = navigation_observation(&build, reachable);
    observations.push(navigation.clone());

    let input_digest = hj(
        INPUT_DOMAIN,
        &Value::object(vec![
            (
                "mdbook_version".to_owned(),
                Value::string(MDBOOK_VERSION.to_owned()),
            ),
            (
                "context_digest".to_owned(),
                Value::string(expectation.context_digest.to_string()),
            ),
            (
                "config_digest".to_owned(),
                Value::string(config_digest.to_string()),
            ),
            ("navigation".to_owned(), navigation),
            ("pages".to_owned(), Value::array(inputs)),
        ]),
    );
    amiss_wire::semantic::envelope(amiss_wire::semantic::SemanticEvidence {
        candidate_identity_digest,
        source_report_payload_digest: None,
        producer_kind: expectation.producer_kind,
        producer_identity: expectation.producer_identity,
        producer_version: expectation.producer_version,
        context_digest: expectation.context_digest,
        input_digest,
        complete: true,
        observations,
    })
    .map_err(|_defect| MdBookEvidenceError::Evidence)
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

fn navigation_observation(build: &BuildPages, reachable: Vec<String>) -> Value {
    Value::object(vec![
        (
            "entrypoints".to_owned(),
            Value::array(vec![Value::string(build.entrypoint.clone())]),
        ),
        (
            "kind".to_owned(),
            Value::string("site-navigation".to_owned()),
        ),
        ("manifest".to_owned(), Value::string(build.manifest.clone())),
        (
            "reachable".to_owned(),
            Value::array(reachable.into_iter().map(Value::string).collect()),
        ),
        (
            "root".to_owned(),
            build
                .source_root
                .as_ref()
                .map_or(Value::Null, |root| Value::string(root.clone())),
        ),
    ])
}
