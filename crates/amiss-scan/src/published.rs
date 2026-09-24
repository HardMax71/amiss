use crate::discovery::Located;
use crate::discovery::SnapshotDiscovery;
use crate::discovery::declared_at;
use crate::discovery::declared_root;
use crate::discovery::published_routes;
use crate::discovery::regular_file;
use crate::discovery::site_root;
use crate::route::ANTORA;
use crate::route::ANTORA_FAMILIES;
use crate::route::BUNDLE_INDEX;
use crate::route::DEFAULT_SOURCE_SUFFIX;
use crate::route::DIRECTORY_PAGES;
use crate::route::DOCUSAURUS;
use crate::route::MDBOOK_PAGES;
use crate::route::MKDOCS;
use crate::route::PAGE_SUFFIXES;
use crate::route::ROUTERS;
use crate::route::SITE_ALIAS;
use crate::route::SPHINX;
use crate::route::Spelling;
use crate::route::UNROUTED_OPENING;
use crate::route::ZOLA;
use crate::route::ancestor_root;
use crate::route::beside_document;
use crate::route::candidates;
use crate::route::content_root;
use crate::route::directory;
use crate::route::docname_beside;
use crate::route::join;
use crate::route::normalized_path_under;
use crate::route::output_extension;
use crate::route::page_route;
use crate::route::rooted_docname;
use crate::route::under_base;
use crate::route::within;
use amiss_wire::controls::GitMode;
use amiss_wire::controls::SourceConstruct;
use amiss_wire::controls::TargetKind;
use amiss_wire::model::Adapter;
use amiss_wire::model::RepoPath;
use amiss_wire::uri::scheme;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
/// The path this reference is answered against. A destination the tree holds
/// is its own answer; otherwise the first router spelling that reaches an
/// ordinary file stands in for it, and last the document published at that
/// route. A promised directory is never re-spelled, and every spelling names
/// a file the tree already holds, so this can only turn an absent target into
/// a present one.
pub(crate) fn routed(
    snapshot: &SnapshotDiscovery,
    path: &RepoPath,
    target_kind: TargetKind,
) -> RepoPath {
    if target_kind == TargetKind::Tree || snapshot.locate(path).is_some() {
        return path.clone();
    }
    candidates(path)
        .into_iter()
        .find(|(_, candidate)| {
            matches!(
                snapshot.locate(candidate),
                Some(Located::Entry(
                    GitMode::RegularFile | GitMode::ExecutableFile,
                    _
                ))
            )
        })
        .map_or_else(
            || snapshot.published_routes.get(path).unwrap_or(path).clone(),
            |(_, candidate)| candidate,
        )
}

/// Where a generator declared in the tree anchors this destination: each
/// directory to look under, in the generator's own order, paired with the
/// part of the destination that is relative to it. Empty when no generator on
/// the document's ancestor chain claims the destination, which leaves it
/// beside the document. A rule that turns on the construct is not asked
/// without one.
#[must_use]
pub(crate) fn anchors(
    snapshot: &SnapshotDiscovery,
    adapter: Adapter,
    document: &RepoPath,
    construct: Option<SourceConstruct>,
    is_image: bool,
    path_part: &str,
) -> Vec<(Vec<u8>, String)> {
    if let Some(declared) = declared_site_anchor(snapshot, adapter, document, is_image, path_part) {
        return vec![declared];
    }
    match adapter {
        Adapter::AsciiDoc => antora_anchor(snapshot, document, construct, path_part),
        Adapter::Markdown | Adapter::Mdx => {
            markdown_anchors(snapshot, document, construct, is_image, path_part)
        }
        Adapter::Rst => sphinx_anchor(snapshot, document, construct, path_part),
        Adapter::PlainAdvisory => Vec::new(),
    }
}

/// The Markdown rules in the order they are asked. The first rule that claims
/// the destination answers, and a rule claims one only where its own
/// generator is declared above the document.
fn markdown_anchors(
    snapshot: &SnapshotDiscovery,
    document: &RepoPath,
    construct: Option<SourceConstruct>,
    is_image: bool,
    path_part: &str,
) -> Vec<(Vec<u8>, String)> {
    let published = published_anchors(snapshot, document, construct, path_part);
    if !published.is_empty() {
        return published;
    }
    if let Some(relative) = path_part.strip_prefix("@/")
        && let Some(content) = zola_content(snapshot, document)
    {
        return vec![(content, relative.to_owned())];
    }
    let site = docusaurus_anchors(snapshot, document, is_image, path_part);
    if !site.is_empty() {
        return site;
    }
    mdbook_anchors(snapshot, document, path_part)
}

/// A site route under the base a declaration gives the directory holding it.
/// The base opens the route, what follows names a page under that directory,
/// and a trailing slash is the page's own URL rather than a tree. The tree
/// has to hold the page, as written or under a router spelling, because a
/// site serves routes its own build makes up and a tree can enumerate only
/// the files it holds: a route reaching none keeps the boundary it had. The
/// generators a declaration names publish Markdown, and the other adapters
/// read a leading slash as a coordinate of their own.
fn declared_site_anchor(
    snapshot: &SnapshotDiscovery,
    adapter: Adapter,
    document: &RepoPath,
    is_image: bool,
    path_part: &str,
) -> Option<(Vec<u8>, String)> {
    let route = path_part
        .strip_prefix('/')
        .filter(|route| !route.starts_with('/'))?;
    if !matches!(adapter, Adapter::Markdown | Adapter::Mdx) {
        return None;
    }
    let raw = document.as_bytes();
    declared_base(snapshot, raw)
        .into_iter()
        .chain(published_bases(snapshot, raw))
        .find_map(|(root, base)| served_page(snapshot, root, &base, is_image, route))
}

/// The page one base serves a route from, where the tree holds it.
fn served_page(
    snapshot: &SnapshotDiscovery,
    root: Vec<u8>,
    base: &str,
    is_image: bool,
    route: &str,
) -> Option<(Vec<u8>, String)> {
    let page = under_base(base, route).map(|under| under.strip_suffix('/').unwrap_or(under))?;
    (!page.is_empty() && tracked_page(snapshot, &root, is_image, page))
        .then(|| (root, page.to_owned()))
}

/// Every content root the configuration of a site holding this document
/// names, with the path each is served under and the longest base first. The
/// configuration states where the pages are and what the site is rooted at,
/// which is what a declaration states by hand, so a repository that already
/// says it is not asked to say it twice. A root belongs to its whole project
/// rather than to the pages beneath it, since one page of a site routes to
/// another wherever either sits.
fn published_bases(snapshot: &SnapshotDiscovery, document: &[u8]) -> Vec<(Vec<u8>, String)> {
    let mut found: Vec<(Vec<u8>, String)> = snapshot
        .published_roots
        .iter()
        .filter(|(project, _)| within(document, project))
        .flat_map(|(_, roots)| roots.iter().cloned())
        .collect();
    found.sort_by_key(|(_, base)| std::cmp::Reverse(base.len()));
    found
}

/// Whether the tree holds what one anchoring names, which is the question the
/// resolver asks of it: the path as written, or the source a router spelling
/// serves it from.
fn tracked_page(
    snapshot: &SnapshotDiscovery,
    parent: &[u8],
    is_image: bool,
    relative: &str,
) -> bool {
    normalized_path_under(parent, is_image, relative)
        .ok()
        .is_some_and(|(path, kind)| snapshot.locate(&routed(snapshot, &path, kind)).is_some())
}

/// Whether the build answers this path instead of the tree. A generator on
/// the document's ancestor chain either serves every relative destination
/// from a page URL this engine does not model, or serves the built page a
/// path names, and neither is a file any tree holds. A destination naming a
/// page's own source is out of that reach whatever the generator: the build
/// reads such a file and publishes what it made under a name of its own, so
/// no route it invents carries this one and the tree's answer stands. An
/// Antora component an extension assembles answers the same way, for the path
/// the coordinate named rather than for the document that wrote it.
#[must_use]
pub(crate) fn unplaced(
    snapshot: &SnapshotDiscovery,
    document: &RepoPath,
    missing: &RepoPath,
) -> bool {
    let raw = missing.as_bytes();
    let page = output_extension(raw).is_some();
    (!PAGE_SUFFIXES.iter().any(|suffix| raw.ends_with(suffix))
        && ROUTERS.iter().any(|rule| {
            (rule.serves(Spelling::BuiltRoute) || (page && rule.serves(Spelling::BuiltPage)))
                && site_root(snapshot, document.as_bytes(), rule).is_some()
        }))
        || assembled(snapshot, missing)
}

/// Whether no router publishes this document as a page of its own. A
/// Docusaurus content path excludes every name opening with `_`, a directory
/// as well as a file, so such a document is rendered into the pages that
/// import it and a fragment written in it names an identity of whichever page
/// that is. One of them may be rendered into several, so the tree fixes
/// neither the page nor the identities it publishes. The site is the one
/// declared above the document alone: calling a file a partial withholds an
/// answer, so it is read from the declaration that covers the file rather
/// than from the tree around it.
#[must_use]
pub(crate) fn unrouted(
    snapshot: &SnapshotDiscovery,
    adapter: Adapter,
    document: &RepoPath,
) -> bool {
    if !matches!(adapter, Adapter::Markdown | Adapter::Mdx) {
        return false;
    }
    let raw = document.as_bytes();
    let Some(root) = declared_root(snapshot, raw, DOCUSAURUS.declared_by)
        .and_then(|site| content_root(&site, raw))
    else {
        return false;
    };
    raw.get(root.len()..)
        .unwrap_or_default()
        .split(|byte| *byte == b'/')
        .any(|segment| segment.starts_with(UNROUTED_OPENING.as_bytes()))
}

/// An Antora resource ID, `[module:][family$]relative`, anchored at the family
/// directory of the named module in every source root of the document's own
/// component, its own root first. A cross reference defaults to the page
/// family and an image to the image family, while an include without a family
/// coordinate stays relative to the file that includes it. A version or
/// component coordinate names a catalogue this tree does not hold, and a `./`
/// or `../` relative is relative to the page.
fn antora_anchor(
    snapshot: &SnapshotDiscovery,
    document: &RepoPath,
    construct: Option<SourceConstruct>,
    path_part: &str,
) -> Vec<(Vec<u8>, String)> {
    let Some(construct) = construct else {
        return Vec::new();
    };
    let default_family = if construct == SourceConstruct::AsciidocCrossReference {
        Some("page")
    } else if construct.is_image() {
        Some("image")
    } else if construct == SourceConstruct::AsciidocInclude {
        None
    } else {
        return Vec::new();
    };
    let Some((root, own_module)) = antora_module(snapshot, document.as_bytes()) else {
        return Vec::new();
    };
    if path_part.contains('@') {
        return Vec::new();
    }
    let (module, resource) = match path_part.split_once(':') {
        None => (own_module, path_part),
        Some((module, resource))
            if !module.is_empty() && !module.contains('/') && !resource.contains(':') =>
        {
            (module.as_bytes(), resource)
        }
        Some(_) => return Vec::new(),
    };
    let Some((family, relative)) = resource
        .split_once('$')
        .or_else(|| default_family.map(|family| (family, resource)))
    else {
        return Vec::new();
    };
    let Some((_, family_directory)) = ANTORA_FAMILIES.iter().find(|(name, _)| *name == family)
    else {
        return Vec::new();
    };
    if relative.is_empty()
        || relative.starts_with('/')
        || relative.starts_with("./")
        || relative.starts_with("../")
    {
        return Vec::new();
    }
    component_roots(snapshot, root)
        .into_iter()
        .map(|root| {
            let module_directory = join(&join(&root, b"modules"), module);
            (
                join(&module_directory, family_directory),
                relative.to_owned(),
            )
        })
        .collect()
}

/// Every source root this tree holds for the component a root contributes to,
/// that root first. Antora assembles one component from every root whose
/// descriptor spells the same name, so a module's resources are stored across
/// all of them and a coordinate naming one is answered by whichever holds it.
fn component_roots(snapshot: &SnapshotDiscovery, root: &[u8]) -> Vec<Vec<u8>> {
    let mut out = vec![root.to_vec()];
    let Some((name, _)) = descriptor(snapshot, root) else {
        return out;
    };
    for (path, (declared, _)) in &snapshot.antora_components {
        let sibling = directory(path.as_bytes());
        if declared == name && sibling != root {
            out.push(sibling.to_vec());
        }
    }
    out
}

/// What the descriptor of one Antora source root declares, when the tree holds
/// one there and this reader spelled it out.
fn descriptor<'a>(snapshot: &'a SnapshotDiscovery, root: &[u8]) -> Option<&'a (String, bool)> {
    ANTORA.declared_by.iter().find_map(|name| {
        RepoPath::from_bytes(join(root, name.as_bytes()))
            .and_then(|path| snapshot.antora_components.get(&path))
    })
}

/// The suffix a docname takes under one root: the first its `conf.py`
/// declares, or the default where it declares nothing this reader spells out.
/// A docname names one file, so a root declaring several reads the first.
fn docname_suffix<'a>(snapshot: &'a SnapshotDiscovery, root: &[u8]) -> &'a str {
    snapshot
        .source_suffixes
        .get(root)
        .and_then(BTreeSet::first)
        .map_or(DEFAULT_SOURCE_SUFFIX, String::as_str)
}

/// Whether the component a path belongs to is assembled by an extension rather
/// than by the tree. Antora's `ext` block is where a component descriptor
/// names the extensions that add resources to it while the site is built, and
/// this engine runs none of them, so the resources such a component serves are
/// not the ones a tree walk can count.
fn assembled(snapshot: &SnapshotDiscovery, path: &RepoPath) -> bool {
    let Some(root) = declared_root(snapshot, path.as_bytes(), ANTORA.declared_by) else {
        return false;
    };
    component_roots(snapshot, &root)
        .iter()
        .filter_map(|root| descriptor(snapshot, root))
        .any(|(_, extended)| *extended)
}

/// The component root and module a document belongs to: the nearest
/// `modules/<name>/` on its path whose parent directory holds `antora.yml`.
fn antora_module<'a>(
    snapshot: &SnapshotDiscovery,
    document: &'a [u8],
) -> Option<(&'a [u8], &'a [u8])> {
    let mut offset = 0_usize;
    let segments: Vec<(usize, &[u8])> = document
        .split(|byte| *byte == b'/')
        .map(|segment| {
            let start = offset;
            offset = offset.saturating_add(segment.len()).saturating_add(1);
            (start, segment)
        })
        .collect();
    segments
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, (start, segment))| {
            if *segment != b"modules" {
                return None;
            }
            let (_, module) = segments.get(index.saturating_add(1))?;
            segments.get(index.saturating_add(2))?;
            if module.is_empty() {
                return None;
            }
            let root = document.get(..start.saturating_sub(1))?;
            ANTORA
                .declared_by
                .iter()
                .any(|name| regular_file(snapshot, join(root, name.as_bytes())))
                .then_some((root, *module))
        })
}

/// A Docusaurus destination under its site directory: the `@site/` alias
/// names a path from that directory, and a bare Markdown path is tried beside
/// the document, then under the plugin content path the document sits in,
/// then under the site directory, which is the order `resolveMarkdownLink`
/// tries them. A `./` or `../` path is beside the document alone, and a URL
/// is not a local path whatever its last segment spells.
fn docusaurus_anchors(
    snapshot: &SnapshotDiscovery,
    document: &RepoPath,
    is_image: bool,
    path_part: &str,
) -> Vec<(Vec<u8>, String)> {
    if scheme(path_part).is_some() {
        return Vec::new();
    }
    let Some(site) = site_root(snapshot, document.as_bytes(), &DOCUSAURUS) else {
        return Vec::new();
    };
    if let Some(relative) = path_part.strip_prefix(SITE_ALIAS) {
        return vec![(site, relative.to_owned())];
    }
    let bare = !path_part.starts_with('/')
        && !path_part.starts_with("./")
        && !path_part.starts_with("../");
    let markdown = path_part.rsplit('/').next().is_some_and(|last| {
        let last = last.to_ascii_lowercase();
        last.as_bytes().ends_with(b".md") || last.as_bytes().ends_with(b".mdx")
    });
    if is_image || !bare || !markdown {
        return Vec::new();
    }
    let mut out = vec![(
        directory(document.as_bytes()).to_vec(),
        path_part.to_owned(),
    )];
    for root in content_root(&site, document.as_bytes())
        .into_iter()
        .chain([site])
    {
        if !out.iter().any(|(held, _)| *held == root) {
            out.push((root, path_part.to_owned()));
        }
    }
    out
}

/// Where the page's own URL puts a destination that climbs out of the book.
/// mdBook serves a page under a book's source directory at its path under the
/// book root, so a destination climbing past that root names a page of the
/// book that holds it and is read back as a source under that book's own
/// source directory, the one its `book.toml` names. The reading beside the document comes
/// first, so the finding still names the path the author wrote.
fn mdbook_anchors(
    snapshot: &SnapshotDiscovery,
    document: &RepoPath,
    path_part: &str,
) -> Vec<(Vec<u8>, String)> {
    let mut relative = path_part;
    let mut climbed = 0_usize;
    while let Some(rest) = relative.strip_prefix("../") {
        relative = rest;
        climbed = climbed.saturating_add(1);
    }
    if climbed == 0 || relative.is_empty() {
        return Vec::new();
    }
    let raw = document.as_bytes();
    let Some(root) = site_root(snapshot, raw, &MDBOOK_PAGES) else {
        return Vec::new();
    };
    let Some(page) = snapshot
        .book_sources
        .get(&root)
        .and_then(|source| raw.strip_prefix(source.as_slice()))
        .and_then(|rest| rest.strip_prefix(b"/"))
    else {
        return Vec::new();
    };
    let depth = page.split(|byte| *byte == b'/').count().saturating_sub(1);
    let Some(above) = climbed.checked_sub(depth).filter(|above| *above > 0) else {
        return Vec::new();
    };
    let mut ancestor = root.as_slice();
    for _ in 0..above {
        if ancestor.is_empty() {
            return Vec::new();
        }
        ancestor = directory(ancestor);
    }
    let Some(owner) = site_root(
        snapshot,
        &join(ancestor, relative.as_bytes()),
        &MDBOOK_PAGES,
    ) else {
        return Vec::new();
    };
    let Some(under) = ancestor
        .strip_prefix(owner.as_slice())
        .map(|rest| rest.strip_prefix(b"/").unwrap_or(rest))
    else {
        return Vec::new();
    };
    let Some(source) = snapshot.book_sources.get(&owner) else {
        return Vec::new();
    };
    let source = if under.is_empty() {
        source.clone()
    } else {
        join(source, under)
    };
    vec![
        (directory(raw).to_vec(), path_part.to_owned()),
        (source, relative.to_owned()),
    ]
}

/// Zola's content root: the `content` directory beside the `config.toml`
/// that declares the site, which is also what tells that file apart from the
/// Hugo configuration spelled the same way.
fn zola_content(snapshot: &SnapshotDiscovery, document: &RepoPath) -> Option<Vec<u8>> {
    let root = site_root(snapshot, document.as_bytes(), &ZOLA)?;
    let content = RepoPath::from_bytes(join(&root, b"content"))?;
    matches!(
        snapshot.locate(&content),
        Some(Located::ImpliedTree | Located::Entry(GitMode::Tree, _))
    )
    .then(|| content.as_bytes().to_vec())
}

/// Reads one snapshot under the routers another one declares and answers with
/// the declarations its own tree held. This is what puts both sides of a
/// comparison to the same question: a declaration is tree state, so a
/// candidate that writes one would otherwise be measured against a base
/// nothing declares, and every destination the declaration moves would answer
/// differently on the two sides and change the identity of whatever finding it
/// carried. A generator's own configuration is borrowed only where the snapshot
/// configures nothing of its own, since a site the candidate newly configures
/// is the same question, while one it reconfigures changes what its links
/// reach and is answered on each side as that side configures it. The routes a
/// document publishes are read from the same declarations, so they are read
/// again whenever these differ.
pub(crate) fn read_as_declared(
    snapshot: &mut SnapshotDiscovery,
    candidate: &SnapshotDiscovery,
) -> BTreeMap<RepoPath, (String, Option<String>)> {
    let held = std::mem::replace(
        &mut snapshot.declared_routers,
        candidate.declared_routers.clone(),
    );
    let configured = (snapshot.bound_configs.len(), snapshot.published_roots.len());
    for (path, rule) in &candidate.bound_configs {
        snapshot.bound_configs.entry(path.clone()).or_insert(*rule);
    }
    for (project, roots) in &candidate.published_roots {
        snapshot
            .published_roots
            .entry(project.clone())
            .or_insert_with(|| roots.clone());
    }
    if held != snapshot.declared_routers
        || configured != (snapshot.bound_configs.len(), snapshot.published_roots.len())
    {
        (snapshot.published_routes, snapshot.redirect_routes) = published_routes(snapshot);
    }
    held
}

/// The page a destination reaches through a URL its target moved away from.
/// The destination is read against the page's own URL and answered by the
/// page declaring that URL, and no other reading is asked: a redirect answers
/// a URL, never a path beside a source file, and for a page bundle the two
/// are the same directory anyway.
#[must_use]
pub(crate) fn redirected(
    snapshot: &SnapshotDiscovery,
    document: &RepoPath,
    anchors: &[(Vec<u8>, String)],
    is_image: bool,
) -> Option<RepoPath> {
    if snapshot.redirect_routes.is_empty() {
        return None;
    }
    let published = page_route(document.as_bytes(), true);
    anchors
        .iter()
        .filter(|(parent, _)| *parent == published)
        .find_map(|(parent, relative)| {
            let (candidate, _kind) = normalized_path_under(parent, is_image, relative).ok()?;
            snapshot.redirect_routes.get(&candidate).cloned()
        })
}

/// The nearest declaration above the document that says where its own
/// directory is published, with that directory. The nearest declaration
/// answers whatever it holds, so one naming no base withholds this reading
/// rather than passing the question further up.
fn declared_base(snapshot: &SnapshotDiscovery, document: &[u8]) -> Option<(Vec<u8>, String)> {
    let root = ancestor_root(document, &|directory| {
        declared_at(snapshot, directory).is_some()
    })?;
    let (_, base) = declared_at(snapshot, &root)?;
    base.clone().map(|base| (root, base))
}

/// A destination the browser resolves rather than the generator, under a
/// router that publishes every page at a directory of its own name: it is
/// relative to the page's own URL and its trailing slash names that page
/// rather than a tree. mkdocs rewrites a Markdown link and leaves raw HTML
/// alone, so only raw HTML is read this way there, while a declared
/// `directory-pages` rewrites nothing and every destination is read this
/// way. The document's own directory is tried first, so a destination that
/// reached a file beside the source still reaches that file, and the page URL
/// is the candidate added.
fn published_anchors(
    snapshot: &SnapshotDiscovery,
    document: &RepoPath,
    construct: Option<SourceConstruct>,
    path_part: &str,
) -> Vec<(Vec<u8>, String)> {
    let Some((beside, relative)) = beside_document(document, path_part) else {
        return Vec::new();
    };
    let raw = document.as_bytes();
    let mut out: Vec<(Vec<u8>, String)> = Vec::new();
    if site_root(snapshot, raw, &DIRECTORY_PAGES).is_some() {
        let published = page_route(raw, true);
        out.push((beside.clone(), relative.clone()));
        out.extend(BUNDLE_INDEX.map(|index| (published.clone(), format!("{relative}/{index}"))));
        if published != beside {
            out.push((published, relative));
        }
    } else if matches!(
        construct,
        Some(SourceConstruct::HtmlAnchor | SourceConstruct::HtmlImage)
    ) && site_root(snapshot, raw, &MKDOCS).is_some()
    {
        let published = page_route(raw, false);
        out.push((published.clone(), relative.clone()));
        if published != beside {
            out.push((beside, relative));
        }
    }
    out
}

/// Where a `:doc:` target is anchored: a leading slash at the directory
/// holding `conf.py`, and anything else beside the document, which is what
/// `docname_join` does. A trailing slash is normalized away first.
fn sphinx_anchor(
    snapshot: &SnapshotDiscovery,
    document: &RepoPath,
    construct: Option<SourceConstruct>,
    path_part: &str,
) -> Vec<(Vec<u8>, String)> {
    let docname = path_part.strip_suffix('/').unwrap_or(path_part);
    let Some((root, suffix)) = docname_root(snapshot, document, construct) else {
        return Vec::new();
    };
    match docname.strip_prefix('/') {
        Some(absolute) => rooted_docname(root, absolute, suffix),
        None => docname_beside(document, docname, suffix),
    }
}

/// The source root a `:doc:` target is read under and the suffix that root
/// reads, where the construct is the role that names a docname at all.
fn docname_root<'a>(
    snapshot: &'a SnapshotDiscovery,
    document: &RepoPath,
    construct: Option<SourceConstruct>,
) -> Option<(Vec<u8>, &'a str)> {
    if construct != Some(SourceConstruct::RstDocRole) {
        return None;
    }
    let root = site_root(snapshot, document.as_bytes(), &SPHINX)?;
    let suffix = docname_suffix(snapshot, &root);
    Some((root, suffix))
}
