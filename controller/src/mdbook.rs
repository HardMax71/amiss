use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use amiss_controller_files::read_bounded_at;
use amiss_wire::digest::{Digest, hb, hj};
use amiss_wire::json::Value;
use amiss_wire::model::{ArtifactId, RepoPathText};
use cap_std::fs::Dir;
use html5gum::{Token, Tokenizer};
use url::Url;

pub const MDBOOK_RENDER_CONTEXT_BYTES: u64 = 16_777_216;
pub const MDBOOK_HTML_BYTES: u64 = 16_777_216;
const MDBOOK_VERSION: &str = "0.5.4";
const SITE_BUILD_VERSION: &str = "0.2.0";
const ROUTE_BYTES: usize = 16_384;
const ANCHOR_BYTES: usize = 4_096;
const INPUT_DOMAIN: &str = "amiss/controller-mdbook-site-input-v1";
const HTML_DOMAIN: &str = "amiss/controller-mdbook-html-v1";

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
    #[error("the completed mdBook HTML output cannot be read")]
    Output(#[source] std::io::Error),
    #[error("a published mdBook anchor is invalid or exceeds its ceiling")]
    Anchor,
    #[error("the completed mdBook navigation graph is invalid or exceeds its ceiling")]
    Navigation,
    #[error("the mdBook evidence exceeds the semantic wire contract")]
    Evidence,
}

struct Page {
    output: PathBuf,
    source: String,
}

struct BuildPages {
    rows: BTreeMap<String, Page>,
    source_root: Option<String>,
    manifest: String,
    entrypoint: String,
}

/// Produces one complete candidate-bound route, anchor, and navigation table
/// from a pinned mdBook renderer context and its completed HTML output.
///
/// `repository_book_root` locates the mdBook root inside the candidate tree;
/// `route_prefix` locates the HTML output inside the published site. The open
/// output directory remains the caller's capability and is never inferred
/// from repository-controlled context paths.
///
/// # Errors
///
/// The context, renderer version, HTML renderer selection, source mapping,
/// output path, page bytes, anchor set, link graph, or resulting evidence is
/// invalid, ambiguous, incomplete, or outside a fixed ceiling.
pub fn mdbook_site_evidence(
    candidate_identity_digest: Digest,
    repository_book_root: Option<&RepoPathText>,
    route_prefix: &str,
    context_bytes: &[u8],
    html_output: &Dir,
) -> Result<Value, MdBookEvidenceError> {
    if u64::try_from(context_bytes.len()).unwrap_or(u64::MAX) > MDBOOK_RENDER_CONTEXT_BYTES {
        return Err(MdBookEvidenceError::ContextBytes);
    }
    let context = amiss_wire::json::parse(context_bytes).map_err(MdBookEvidenceError::Context)?;
    let (source_directory, items) = render_context(&context)?;
    let base = route_base(route_prefix)?;
    let build = pages(items, &source_directory, repository_book_root, &base)?;
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
        let html_digest = hb(HTML_DOMAIN, &html);
        inputs.push(Value::object(vec![
            ("route".to_owned(), Value::string(route.clone())),
            ("source".to_owned(), Value::string(page.source.clone())),
            (
                "html_digest".to_owned(),
                Value::string(html_digest.to_string()),
            ),
        ]));
        observations.push(Value::object(vec![
            ("kind".to_owned(), Value::string("site-route".to_owned())),
            ("route".to_owned(), Value::string(route.clone())),
            ("source".to_owned(), Value::string(page.source.clone())),
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
                "route_prefix".to_owned(),
                Value::string(route_prefix.to_owned()),
            ),
            ("navigation".to_owned(), navigation),
            ("pages".to_owned(), Value::array(inputs)),
        ]),
    );
    let producer_kind =
        ArtifactId::new("site-build".to_owned()).ok_or(MdBookEvidenceError::Evidence)?;
    let producer_identity = ArtifactId::new("amiss-controller-mdbook-html".to_owned())
        .ok_or(MdBookEvidenceError::Evidence)?;
    amiss_wire::semantic::envelope(amiss_wire::semantic::SemanticEvidence {
        candidate_identity_digest,
        source_report_payload_digest: None,
        producer_kind,
        producer_identity,
        producer_version: SITE_BUILD_VERSION.to_owned(),
        input_digest,
        complete: true,
        observations,
    })
    .map_err(|_defect| MdBookEvidenceError::Evidence)
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

fn render_context(context: &Value) -> Result<(PathBuf, &[Value]), MdBookEvidenceError> {
    if context.text("version") != Some(MDBOOK_VERSION) {
        return Err(MdBookEvidenceError::UnsupportedBuild);
    }
    let config = context
        .member("config")
        .ok_or(MdBookEvidenceError::ContextShape)?;
    let book_config = config
        .member("book")
        .ok_or(MdBookEvidenceError::ContextShape)?;
    let source_directory = match book_config.member("src") {
        None => PathBuf::from("src"),
        Some(Value::String(path)) => relative_path(path, true)?,
        Some(
            Value::Null | Value::Bool(_) | Value::Integer(_) | Value::Array(_) | Value::Object(_),
        ) => return Err(MdBookEvidenceError::ContextShape),
    };
    if !matches!(
        config
            .member("output")
            .and_then(|output| output.member("html")),
        Some(Value::Object(_))
    ) {
        return Err(MdBookEvidenceError::UnsupportedBuild);
    }
    let Some(Value::Array(items)) = context.member("book").and_then(|book| book.member("items"))
    else {
        return Err(MdBookEvidenceError::ContextShape);
    };
    Ok((source_directory, items))
}

fn pages(
    items: &[Value],
    source_directory: &Path,
    repository_book_root: Option<&RepoPathText>,
    base: &Url,
) -> Result<BuildPages, MdBookEvidenceError> {
    let source_root = repository_path(
        repository_book_root.map(RepoPathText::as_str),
        source_directory,
    )?;
    let manifest = repository_path(source_root.as_deref(), Path::new("SUMMARY.md"))?
        .ok_or(MdBookEvidenceError::Path)?;
    let mut pending: Vec<&Value> = items.iter().rev().collect();
    let mut pages = BTreeMap::new();
    let mut entrypoint = None;
    while let Some(item) = pending.pop() {
        let chapter = match item {
            Value::String(separator) if separator.as_ref() == "Separator" => continue,
            Value::Object(members) => {
                let Some((kind, value)) = members.first() else {
                    return Err(MdBookEvidenceError::ContextShape);
                };
                if members.len() != 1 {
                    return Err(MdBookEvidenceError::ContextShape);
                }
                match (kind.as_str(), value) {
                    ("Chapter", Value::Object(_)) => value,
                    ("PartTitle", Value::String(_)) => continue,
                    _ => return Err(MdBookEvidenceError::ContextShape),
                }
            }
            Value::Null
            | Value::Bool(_)
            | Value::Integer(_)
            | Value::String(_)
            | Value::Array(_) => return Err(MdBookEvidenceError::ContextShape),
        };
        let Some(Value::Array(sub_items)) = chapter.member("sub_items") else {
            return Err(MdBookEvidenceError::ContextShape);
        };
        pending.extend(sub_items.iter().rev());
        let path = optional_text(chapter, "path")?;
        let source_path = optional_text(chapter, "source_path")?;
        let (Some(path), Some(source_path)) = (path, source_path) else {
            if path.is_none() && source_path.is_none() {
                continue;
            }
            return Err(MdBookEvidenceError::UnsupportedBuild);
        };
        let mut output = relative_path(path, false)?;
        output.set_extension("html");
        let source_path = relative_path(source_path, false)?;
        let source = repository_path(source_root.as_deref(), &source_path)?
            .ok_or(MdBookEvidenceError::Path)?;
        let route = output_route(base, &output)?;
        insert_page(
            &mut pages,
            route.clone(),
            Page {
                output: output.clone(),
                source: source.clone(),
            },
        )?;
        if entrypoint.is_none() {
            let index_output = PathBuf::from("index.html");
            let index_route = output_route(base, &index_output)?;
            entrypoint = Some(index_route.clone());
            if index_route != route {
                insert_page(
                    &mut pages,
                    index_route,
                    Page {
                        output: index_output,
                        source,
                    },
                )?;
            }
        }
    }
    Ok(BuildPages {
        rows: pages,
        source_root,
        manifest,
        entrypoint: entrypoint.ok_or(MdBookEvidenceError::UnsupportedBuild)?,
    })
}

fn optional_text<'a>(
    object: &'a Value,
    name: &str,
) -> Result<Option<&'a str>, MdBookEvidenceError> {
    match object.member(name) {
        Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value)),
        None | Some(Value::Bool(_) | Value::Integer(_) | Value::Array(_) | Value::Object(_)) => {
            Err(MdBookEvidenceError::ContextShape)
        }
    }
}

fn insert_page(
    pages: &mut BTreeMap<String, Page>,
    route: String,
    page: Page,
) -> Result<(), MdBookEvidenceError> {
    if pages.insert(route, page).is_some() {
        Err(MdBookEvidenceError::UnsupportedBuild)
    } else {
        Ok(())
    }
}

fn relative_path(raw: &str, empty_allowed: bool) -> Result<PathBuf, MdBookEvidenceError> {
    if raw == "." && empty_allowed {
        return Ok(PathBuf::new());
    }
    if raw.is_empty()
        || raw.len() > 4_096
        || raw
            .split(|character| character == '/' || (cfg!(windows) && character == '\\'))
            .any(|segment| segment.is_empty() || matches!(segment, "." | ".."))
    {
        return Err(MdBookEvidenceError::Path);
    }
    let mut path = PathBuf::new();
    for component in Path::new(raw).components() {
        match component {
            Component::Normal(segment) => {
                let segment = segment.to_str().ok_or(MdBookEvidenceError::Path)?;
                if segment.contains('\\') {
                    return Err(MdBookEvidenceError::Path);
                }
                path.push(segment);
            }
            Component::Prefix(_)
            | Component::RootDir
            | Component::CurDir
            | Component::ParentDir => {
                return Err(MdBookEvidenceError::Path);
            }
        }
    }
    if path.as_os_str().is_empty() {
        Err(MdBookEvidenceError::Path)
    } else {
        Ok(path)
    }
}

fn repository_path(
    prefix: Option<&str>,
    path: &Path,
) -> Result<Option<String>, MdBookEvidenceError> {
    let mut joined = prefix.map_or_else(String::new, str::to_owned);
    for component in path.components() {
        let Component::Normal(segment) = component else {
            return Err(MdBookEvidenceError::Path);
        };
        let segment = segment.to_str().ok_or(MdBookEvidenceError::Path)?;
        if !joined.is_empty() {
            joined.push('/');
        }
        joined.push_str(segment);
    }
    if joined.is_empty() {
        Ok(None)
    } else if RepoPathText::new(joined.clone()).is_some() {
        Ok(Some(joined))
    } else {
        Err(MdBookEvidenceError::Path)
    }
}

fn route_base(prefix: &str) -> Result<Url, MdBookEvidenceError> {
    if prefix.len() > ROUTE_BYTES
        || !prefix.ends_with('/')
        || !amiss_wire::uri::site_route_valid(prefix)
    {
        return Err(MdBookEvidenceError::Route);
    }
    let base = Url::parse(&format!("https://amiss.invalid{prefix}"))
        .map_err(|_defect| MdBookEvidenceError::Route)?;
    if base.path() == prefix {
        Ok(base)
    } else {
        Err(MdBookEvidenceError::Route)
    }
}

fn output_route(base: &Url, output: &Path) -> Result<String, MdBookEvidenceError> {
    let mut url = base.clone();
    let mut segments = url
        .path_segments_mut()
        .map_err(|()| MdBookEvidenceError::Route)?;
    segments.pop_if_empty();
    for component in output.components() {
        let Component::Normal(segment) = component else {
            return Err(MdBookEvidenceError::Path);
        };
        segments.push(segment.to_str().ok_or(MdBookEvidenceError::Path)?);
    }
    drop(segments);
    let route = url.path().to_owned();
    if route.len() <= ROUTE_BYTES && amiss_wire::uri::site_route_valid(&route) {
        Ok(route)
    } else {
        Err(MdBookEvidenceError::Route)
    }
}

fn page_facts(
    html: &[u8],
    route: &str,
    pages: &BTreeMap<String, Page>,
    anchor_count: &mut usize,
    href_count: &mut usize,
) -> Result<(Vec<String>, Vec<String>), MdBookEvidenceError> {
    let mut anchors = BTreeSet::new();
    let mut base_href = None;
    let mut hrefs = BTreeSet::new();
    for token in Tokenizer::new(html) {
        let token = match token {
            Ok(token) => token,
            Err(never) => match never {},
        };
        match token {
            Token::StartTag(tag) => {
                let name = tag.name.as_ref();
                let is_base = name == b"base".as_slice();
                let is_link = name == b"a".as_slice() || name == b"area".as_slice();
                for (name, value) in tag.attributes {
                    if name.as_ref() == b"id".as_slice() {
                        let anchor = String::from_utf8(value.value.0)
                            .map_err(|_defect| MdBookEvidenceError::Anchor)?;
                        if anchor.is_empty()
                            || anchor.len() > ANCHOR_BYTES
                            || anchor.chars().any(char::is_control)
                        {
                            return Err(MdBookEvidenceError::Anchor);
                        }
                        if anchors.insert(anchor) {
                            *anchor_count = anchor_count
                                .checked_add(1)
                                .filter(|count| {
                                    *count <= amiss_wire::semantic::SEMANTIC_OBSERVATIONS_LIMIT
                                })
                                .ok_or(MdBookEvidenceError::Anchor)?;
                        }
                    } else if name.as_ref() == b"href".as_slice()
                        && ((is_base && base_href.is_none()) || is_link)
                    {
                        let href = String::from_utf8(value.value.0)
                            .map_err(|_defect| MdBookEvidenceError::Navigation)?;
                        if is_base && base_href.is_none() {
                            base_href = Some(href);
                        } else if is_link && hrefs.insert(href) {
                            *href_count = href_count
                                .checked_add(1)
                                .filter(|count| {
                                    *count <= amiss_wire::semantic::SEMANTIC_OBSERVATIONS_LIMIT
                                })
                                .ok_or(MdBookEvidenceError::Navigation)?;
                        }
                    }
                }
            }
            Token::EndTag(_)
            | Token::String(_)
            | Token::Comment(_)
            | Token::Doctype(_)
            | Token::Error(_) => {}
        }
    }

    let document = Url::parse(&format!("https://amiss.invalid{route}"))
        .map_err(|_defect| MdBookEvidenceError::Navigation)?;
    let base = base_href
        .and_then(|href| document.join(&href).ok())
        .unwrap_or(document);
    let mut destinations = BTreeSet::new();
    for href in hrefs {
        let Ok(destination) = base.join(&href) else {
            continue;
        };
        if destination.scheme() == "https"
            && destination.host_str() == Some("amiss.invalid")
            && destination.port().is_none()
            && destination.username().is_empty()
            && destination.password().is_none()
            && pages.contains_key(destination.path())
        {
            destinations.insert(destination.path().to_owned());
        }
    }
    Ok((
        anchors.into_iter().collect(),
        destinations.into_iter().collect(),
    ))
}

fn reachable_sources(
    entrypoint: &str,
    links: &BTreeMap<String, Vec<String>>,
    pages: &BTreeMap<String, Page>,
) -> Result<Vec<String>, MdBookEvidenceError> {
    let mut pending = vec![entrypoint.to_owned()];
    let mut reached = BTreeSet::new();
    while let Some(route) = pending.pop() {
        if !reached.insert(route.clone()) {
            continue;
        }
        let destinations = links.get(&route).ok_or(MdBookEvidenceError::Navigation)?;
        pending.extend(destinations.iter().rev().cloned());
    }
    let mut sources = BTreeSet::new();
    for route in reached {
        let page = pages.get(&route).ok_or(MdBookEvidenceError::Navigation)?;
        sources.insert(page.source.clone());
    }
    Ok(sources.into_iter().collect())
}
