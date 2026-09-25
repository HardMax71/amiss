use amiss_wire::extraction::SourceConstruct;
use amiss_wire::model::{Adapter, RepoPath};

use crate::discovery::{SnapshotDiscovery, site_root};
use crate::route::{DEFAULT_SOURCE_SUFFIX, SPHINX, docname_beside, rooted_docname};

/// Where a `:doc:` target is anchored: a leading slash at the directory
/// holding `conf.py`, and anything else beside the document, which is what
/// `docname_join` does. A trailing slash is normalized away first. A file
/// Sphinx reads keeps its name and takes the same root for a leading slash.
/// With no root above it, a docname takes its own format's suffix.
pub(super) fn anchors(
    snapshot: &SnapshotDiscovery,
    adapter: Adapter,
    document: &RepoPath,
    construct: Option<SourceConstruct>,
    path_part: &str,
) -> Vec<(Vec<u8>, String)> {
    let unrooted = if adapter == Adapter::Markdown {
        ".md"
    } else {
        DEFAULT_SOURCE_SUFFIX
    };
    let docname = path_part.strip_suffix('/').unwrap_or(path_part);
    match docname_root(snapshot, document, construct) {
        DocnameRoot::Rooted(root, suffixes) => match docname.strip_prefix('/') {
            Some(absolute) => rooted_docname(&root, absolute, &suffixes),
            None => docname_beside(document, docname, &suffixes),
        },
        DocnameRoot::Unrooted => docname_beside(document, docname, &[unrooted]),
        DocnameRoot::NotDocname => Vec::new(),
    }
}

/// Where a construct's docname is read: nowhere when the construct names no
/// docname, beside its document under the default suffix when no `conf.py`
/// sits above it, and otherwise under the directory holding the nearest
/// `conf.py`, with the suffix that file declares. A file Sphinx reads keeps
/// its name, so it takes no suffix and has no reading without that root.
enum DocnameRoot<'a> {
    NotDocname,
    Unrooted,
    Rooted(Vec<u8>, Vec<&'a str>),
}

fn docname_root<'a>(
    snapshot: &'a SnapshotDiscovery,
    document: &RepoPath,
    construct: Option<SourceConstruct>,
) -> DocnameRoot<'a> {
    if matches!(
        construct,
        Some(
            SourceConstruct::RstDownloadRole
                | SourceConstruct::RstImageDirective
                | SourceConstruct::RstIncludeDirective
        )
    ) {
        return site_root(snapshot, document.as_bytes(), &SPHINX)
            .map_or(DocnameRoot::NotDocname, |root| {
                DocnameRoot::Rooted(root, vec![""])
            });
    }
    if !matches!(
        construct,
        Some(SourceConstruct::RstDocRole | SourceConstruct::RstTocTreeEntry)
    ) {
        return DocnameRoot::NotDocname;
    }
    site_root(snapshot, document.as_bytes(), &SPHINX).map_or(DocnameRoot::Unrooted, |root| {
        let suffixes = docname_suffixes(snapshot, &root);
        DocnameRoot::Rooted(root, suffixes)
    })
}

/// The suffixes a docname takes under one root: every one its `conf.py`
/// declares, or the default where it declares nothing this reader spells out,
/// and `.md` wherever it loads `MyST`, which adds that suffix when it loads.
fn docname_suffixes<'a>(snapshot: &'a SnapshotDiscovery, root: &[u8]) -> Vec<&'a str> {
    let mut suffixes: Vec<&str> = snapshot
        .source_suffixes
        .get(root)
        .filter(|declared| !declared.is_empty())
        .map_or_else(
            || vec![DEFAULT_SOURCE_SUFFIX],
            |declared| declared.iter().map(String::as_str).collect(),
        );
    let myst = snapshot.sphinx_configs.get(root).is_some_and(|config| {
        MYST_EXTENSIONS
            .iter()
            .any(|extension| config.extensions.contains(*extension))
    });
    if myst && !suffixes.contains(&".md") {
        suffixes.push(".md");
    }
    suffixes
}

const MYST_EXTENSIONS: [&str; 2] = ["myst_parser", "myst_nb"];

/// The sources a Sphinx root builds the page a `.html` path names from: the
/// stem under each suffix the root reads, since the build writes each page
/// beside where its source sits.
pub(super) fn page_sources(snapshot: &SnapshotDiscovery, path: &RepoPath) -> Vec<RepoPath> {
    let raw = path.as_bytes();
    let (Some(stem), Some(root)) = (
        raw.strip_suffix(b".html"),
        site_root(snapshot, raw, &SPHINX),
    ) else {
        return Vec::new();
    };
    docname_suffixes(snapshot, &root)
        .into_iter()
        .filter_map(|suffix| RepoPath::from_bytes([stem, suffix.as_bytes()].concat()))
        .collect()
}
