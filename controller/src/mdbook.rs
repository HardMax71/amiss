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
const SITE_BUILD_VERSION: &str = "0.1.0";
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
    #[error("the mdBook evidence exceeds the semantic wire contract")]
    Evidence,
}

struct Page {
    route: String,
    output: PathBuf,
    source: String,
}

/// Produces one complete candidate-bound route and anchor table from a pinned
/// mdBook renderer context and its completed HTML output.
///
/// `repository_book_root` locates the mdBook root inside the candidate tree;
/// `route_prefix` locates the HTML output inside the published site. The open
/// output directory remains the caller's capability and is never inferred
/// from repository-controlled context paths.
///
/// # Errors
///
/// The context, renderer version, HTML renderer selection, source mapping,
/// output path, page bytes, anchor set, or resulting evidence is invalid,
/// ambiguous, incomplete, or outside a fixed ceiling.
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
    let pages = pages(items, &source_directory, repository_book_root, &base)?;
    if pages.is_empty() || pages.len() > amiss_wire::semantic::SEMANTIC_OBSERVATIONS_LIMIT {
        return Err(MdBookEvidenceError::UnsupportedBuild);
    }

    let mut remaining = MDBOOK_HTML_BYTES;
    let mut anchor_count = 0_usize;
    let mut observations = Vec::with_capacity(pages.len());
    let mut inputs = Vec::with_capacity(pages.len());
    for page in pages.into_values() {
        let html = read_bounded_at(html_output, &page.output, remaining)
            .map_err(MdBookEvidenceError::Output)?;
        remaining = remaining
            .checked_sub(u64::try_from(html.len()).unwrap_or(u64::MAX))
            .ok_or(MdBookEvidenceError::Output(std::io::Error::other(
                "aggregate HTML byte ceiling exceeded",
            )))?;
        let anchors = anchors(&html, &mut anchor_count)?;
        let html_digest = hb(HTML_DOMAIN, &html);
        inputs.push(Value::object(vec![
            ("route".to_owned(), Value::string(page.route.clone())),
            ("source".to_owned(), Value::string(page.source.clone())),
            (
                "html_digest".to_owned(),
                Value::string(html_digest.to_string()),
            ),
        ]));
        observations.push(Value::object(vec![
            ("kind".to_owned(), Value::string("site-route".to_owned())),
            ("route".to_owned(), Value::string(page.route)),
            ("source".to_owned(), Value::string(page.source)),
            (
                "anchors".to_owned(),
                Value::array(anchors.into_iter().map(Value::string).collect()),
            ),
        ]));
    }

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
) -> Result<BTreeMap<String, Page>, MdBookEvidenceError> {
    let mut pending: Vec<&Value> = items.iter().rev().collect();
    let mut pages = BTreeMap::new();
    let mut first = true;
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
        let source = repository_source(repository_book_root, source_directory, source_path)?;
        let route = output_route(base, &output)?;
        insert_page(
            &mut pages,
            Page {
                route: route.clone(),
                output: output.clone(),
                source: source.clone(),
            },
        )?;
        if first {
            first = false;
            let index_output = PathBuf::from("index.html");
            let index_route = output_route(base, &index_output)?;
            if index_route != route {
                insert_page(
                    &mut pages,
                    Page {
                        route: index_route,
                        output: index_output,
                        source,
                    },
                )?;
            }
        }
    }
    Ok(pages)
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

fn insert_page(pages: &mut BTreeMap<String, Page>, page: Page) -> Result<(), MdBookEvidenceError> {
    if pages.insert(page.route.clone(), page).is_some() {
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

fn repository_source(
    repository_book_root: Option<&RepoPathText>,
    source_directory: &Path,
    source_path: &str,
) -> Result<String, MdBookEvidenceError> {
    let source_path = relative_path(source_path, false)?;
    let mut source = repository_book_root.map_or_else(String::new, |root| root.as_str().to_owned());
    for path in [source_directory, &source_path] {
        for component in path.components() {
            let Component::Normal(segment) = component else {
                return Err(MdBookEvidenceError::Path);
            };
            let segment = segment.to_str().ok_or(MdBookEvidenceError::Path)?;
            if !source.is_empty() {
                source.push('/');
            }
            source.push_str(segment);
        }
    }
    RepoPathText::new(source.clone())
        .map(|_validated| source)
        .ok_or(MdBookEvidenceError::Path)
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

fn anchors(html: &[u8], total: &mut usize) -> Result<Vec<String>, MdBookEvidenceError> {
    let mut anchors = BTreeSet::new();
    for token in Tokenizer::new(html) {
        let token = match token {
            Ok(token) => token,
            Err(never) => match never {},
        };
        match token {
            Token::StartTag(tag) => {
                let Some((_name, value)) = tag
                    .attributes
                    .into_iter()
                    .find(|(name, _value)| name.as_ref() == b"id".as_slice())
                else {
                    continue;
                };
                let anchor = String::from_utf8(value.value.0)
                    .map_err(|_defect| MdBookEvidenceError::Anchor)?;
                if anchor.is_empty()
                    || anchor.len() > ANCHOR_BYTES
                    || anchor.chars().any(char::is_control)
                {
                    return Err(MdBookEvidenceError::Anchor);
                }
                if anchors.insert(anchor) {
                    *total = total
                        .checked_add(1)
                        .filter(|count| *count <= amiss_wire::semantic::SEMANTIC_OBSERVATIONS_LIMIT)
                        .ok_or(MdBookEvidenceError::Anchor)?;
                }
            }
            Token::EndTag(_)
            | Token::String(_)
            | Token::Comment(_)
            | Token::Doctype(_)
            | Token::Error(_) => {}
        }
    }
    Ok(anchors.into_iter().collect())
}
