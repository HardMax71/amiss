use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use amiss_wire::digest::{Digest, hj};
use amiss_wire::model::{ArtifactId, RepoPathText};
use serde::{Deserialize, Serialize};
use url::Url;

use super::{MDBOOK_VERSION, MdBookEvidenceError, SiteBuildContext};

const SITE_BUILD_VERSION: &str = "0.5.1";
const ROUTE_BYTES: usize = 16_384;
const CONTEXT_DOMAIN: &str = "amiss/controller-mdbook-site-context-v1";
const CONFIG_DOMAIN: &str = "amiss/controller-mdbook-config-v1";

pub(super) struct Page {
    pub(super) output: PathBuf,
    pub(super) source: Option<String>,
}

pub(super) struct BuildPages {
    pub(super) rows: BTreeMap<String, Page>,
    pub(super) source_root: Option<String>,
    pub(super) manifest: String,
    pub(super) entrypoint: String,
}

pub(super) fn site_build_context(
    site: &SiteBuildContext,
) -> Result<(crate::SemanticEvidenceExpectation, Url, Option<String>), MdBookEvidenceError> {
    let configuration = site.configuration.as_str();
    let repository_book_root = if configuration == "book.toml" {
        None
    } else {
        configuration
            .strip_suffix("/book.toml")
            .filter(|root| !root.is_empty())
            .map(str::to_owned)
            .map(Some)
            .ok_or(MdBookEvidenceError::ContextIdentity)?
    };
    if [site.locale.as_deref(), site.version.as_deref()]
        .into_iter()
        .flatten()
        .any(|identity| !amiss_wire::semantic::producer_version_valid(identity))
    {
        return Err(MdBookEvidenceError::ContextIdentity);
    }
    let base = route_base(&site.route_prefix)?;
    let context_digest = amiss_wire::codec::digest(
        CONTEXT_DOMAIN,
        &SiteIdentity {
            configuration,
            locale: site.locale.as_deref(),
            route_prefix: &site.route_prefix,
            version: site.version.as_deref(),
        },
    )
    .map_err(|_defect| MdBookEvidenceError::ContextIdentity)?;
    let producer_kind =
        ArtifactId::new("site-build".to_owned()).ok_or(MdBookEvidenceError::Evidence)?;
    let producer_identity = ArtifactId::new("amiss-controller-mdbook-html".to_owned())
        .ok_or(MdBookEvidenceError::Evidence)?;
    Ok((
        crate::SemanticEvidenceExpectation {
            acquisition_identity: producer_identity.clone(),
            producer_kind,
            producer_identity,
            producer_version: SITE_BUILD_VERSION.to_owned(),
            context_digest,
        },
        base,
        repository_book_root,
    ))
}

#[derive(Serialize)]
struct SiteIdentity<'a> {
    configuration: &'a str,
    locale: Option<&'a str>,
    route_prefix: &'a str,
    version: Option<&'a str>,
}

#[derive(Deserialize)]
struct RenderContext {
    version: String,
    config: serde_json::Value,
    book: Book,
}

#[derive(Deserialize)]
struct Book {
    items: Vec<BookItem>,
}

#[derive(Deserialize)]
pub(super) enum BookItem {
    Chapter(Chapter),
    PartTitle(String),
    Separator,
}

#[derive(Deserialize)]
pub(super) struct Chapter {
    #[serde(deserialize_with = "amiss_wire::codec::nullable")]
    path: Option<String>,
    #[serde(deserialize_with = "amiss_wire::codec::nullable")]
    source_path: Option<String>,
    sub_items: Vec<BookItem>,
}

#[derive(Deserialize)]
struct Config {
    book: BookConfig,
    #[serde(default)]
    output: Option<Output>,
}

#[derive(Deserialize)]
struct BookConfig {
    #[serde(default = "default_source")]
    src: String,
}

#[derive(Deserialize)]
struct Output {
    #[serde(default)]
    html: Option<BTreeMap<String, serde_json::Value>>,
}

fn default_source() -> String {
    "src".to_owned()
}

pub(super) fn render_context(
    bytes: &[u8],
) -> Result<(PathBuf, Vec<BookItem>, Digest), MdBookEvidenceError> {
    let value = amiss_wire::json::parse_upstream(bytes).map_err(MdBookEvidenceError::Context)?;
    let context: RenderContext = amiss_wire::codec::owned_value("$", value)
        .map_err(|_defect| MdBookEvidenceError::ContextShape)?;
    if context.version != MDBOOK_VERSION {
        return Err(MdBookEvidenceError::UnsupportedBuild);
    }
    let config_digest = hj(CONFIG_DOMAIN, &context.config);
    let config: Config = amiss_wire::codec::owned_value("$.config", context.config)
        .map_err(|_defect| MdBookEvidenceError::ContextShape)?;
    let source_directory = relative_path(&config.book.src, true)?;
    if config.output.and_then(|output| output.html).is_none() {
        return Err(MdBookEvidenceError::UnsupportedBuild);
    }
    Ok((source_directory, context.book.items, config_digest))
}

pub(super) fn pages(
    items: &[BookItem],
    source_directory: &Path,
    repository_book_root: Option<&str>,
    base: &Url,
) -> Result<BuildPages, MdBookEvidenceError> {
    let source_root = repository_path(repository_book_root, source_directory)?;
    let manifest = repository_path(source_root.as_deref(), Path::new("SUMMARY.md"))?
        .ok_or(MdBookEvidenceError::Path)?;
    let mut pending: Vec<&BookItem> = items.iter().rev().collect();
    let mut pages = BTreeMap::new();
    let mut entrypoint = None;
    while let Some(item) = pending.pop() {
        let chapter = match item {
            BookItem::Chapter(chapter) => chapter,
            BookItem::PartTitle(_title) => continue,
            BookItem::Separator => continue,
        };
        pending.extend(chapter.sub_items.iter().rev());
        let path = chapter.path.as_deref();
        let source_path = chapter.source_path.as_deref();
        let Some(path) = path else {
            if source_path.is_none() {
                continue;
            }
            return Err(MdBookEvidenceError::UnsupportedBuild);
        };
        let mut output = relative_path(path, false)?;
        output.set_extension("html");
        let source = source_path
            .map(|source_path| {
                let source_path = relative_path(source_path, false)?;
                repository_path(source_root.as_deref(), &source_path)?
                    .ok_or(MdBookEvidenceError::Path)
            })
            .transpose()?;
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
