use std::collections::{BTreeMap, BTreeSet};

use amiss_wire::controls::{GitMode, SourceConstruct};
use amiss_wire::model::{Adapter, RepoPath};
use amiss_wire::uri::scheme;

use crate::discovery::{DocumentStatus, Located, SnapshotDiscovery};

/// A spelling a router serves for a page whose source file is named
/// otherwise. The first three were harvested from the router itself and hold
/// in every tree; the rest were read from a generator's own resolver and hold
/// only under the file that declares that generator. The last two name what a
/// build serves instead of the tree, so they reach no file and declare a
/// boundary where the tree holds none.
#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::AsRefStr)]
#[strum(serialize_all = "kebab-case")]
pub enum Spelling {
    Extensionless,
    OutputExtension,
    ReadmeIndex,
    AntoraResource,
    SiteAlias,
    ContentRoot,
    DocumentId,
    SourceRoot,
    DirectoryUrl,
    BookRoute,
    BuiltPage,
    BuiltRoute,
}

/// One router's route rule: the spellings it serves for a source file beyond
/// the source path itself, and the file whose presence on a document's
/// ancestor chain selects it. A router declared by nothing serves every tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RouteRule {
    pub name: &'static str,
    pub declared_by: &'static [&'static str],
    pub serves: &'static [Spelling],
}

impl RouteRule {
    #[must_use]
    pub fn serves(&self, spelling: Spelling) -> bool {
        self.serves.contains(&spelling)
    }
}

const ANTORA: RouteRule = RouteRule {
    name: "antora",
    declared_by: &["antora.yml"],
    serves: &[Spelling::AntoraResource],
};

pub const DOCUSAURUS: RouteRule = RouteRule {
    name: "docusaurus",
    declared_by: &[
        "docusaurus.config.ts",
        "docusaurus.config.mts",
        "docusaurus.config.cts",
        "docusaurus.config.js",
        "docusaurus.config.mjs",
        "docusaurus.config.cjs",
    ],
    serves: &[
        Spelling::SiteAlias,
        Spelling::ContentRoot,
        Spelling::DocumentId,
    ],
};

pub(crate) const MKDOCS: RouteRule = RouteRule {
    name: "mkdocs",
    declared_by: &["mkdocs.yml", "mkdocs.yaml"],
    serves: &[Spelling::DirectoryUrl],
};

pub(crate) const SPHINX: RouteRule = RouteRule {
    name: "sphinx",
    declared_by: &["conf.py"],
    serves: &[Spelling::SourceRoot],
};

pub(crate) const MDBOOK_PAGES: RouteRule = RouteRule {
    name: "mdbook-pages",
    declared_by: &["book.toml"],
    serves: &[Spelling::BookRoute, Spelling::BuiltPage],
};

pub(crate) const ZOLA: RouteRule = RouteRule {
    name: "zola",
    declared_by: &["config.toml"],
    serves: &[Spelling::ContentRoot],
};

const ASTRO: RouteRule = RouteRule {
    name: "astro",
    declared_by: &[
        "astro.config.ts",
        "astro.config.mts",
        "astro.config.js",
        "astro.config.mjs",
        "astro.config.cjs",
    ],
    serves: &[Spelling::BuiltRoute],
};

const ELEVENTY: RouteRule = RouteRule {
    name: "eleventy",
    declared_by: &[
        "eleventy.config.ts",
        "eleventy.config.js",
        "eleventy.config.mjs",
        "eleventy.config.cjs",
        ".eleventy.js",
    ],
    serves: &[Spelling::BuiltRoute],
};

const HUGO: RouteRule = RouteRule {
    name: "hugo",
    declared_by: &["hugo.toml", "hugo.yaml"],
    serves: &[Spelling::BuiltRoute],
};

const JEKYLL: RouteRule = RouteRule {
    name: "jekyll",
    declared_by: &["_config.yml"],
    serves: &[Spelling::BuiltRoute],
};

/// Every router rule the resolver knows. A spelling reaches a source file only
/// when that file is in the tree, so a rule can widen what resolves and can
/// never invent a target.
pub const ROUTERS: [RouteRule; 13] = [
    RouteRule {
        name: "mdbook",
        declared_by: &[],
        serves: &[Spelling::OutputExtension, Spelling::ReadmeIndex],
    },
    RouteRule {
        name: "vitepress",
        declared_by: &[],
        serves: &[Spelling::Extensionless, Spelling::OutputExtension],
    },
    RouteRule {
        name: "vitepress-readme",
        declared_by: &[],
        serves: &[
            Spelling::Extensionless,
            Spelling::OutputExtension,
            Spelling::ReadmeIndex,
        ],
    },
    ANTORA,
    DOCUSAURUS,
    MKDOCS,
    SPHINX,
    MDBOOK_PAGES,
    ZOLA,
    ASTRO,
    ELEVENTY,
    HUGO,
    JEKYLL,
];

/// Antora's resource families: the coordinate an author writes before `$`,
/// and the module directory the family is stored under.
const ANTORA_FAMILIES: [(&str, &[u8]); 5] = [
    ("page", b"pages"),
    ("partial", b"partials"),
    ("example", b"examples"),
    ("attachment", b"attachments"),
    ("image", b"images"),
];

/// The plugin content paths Docusaurus reads by default, relative to the site
/// directory, with `*` standing for one segment.
pub const DOCUSAURUS_CONTENT_ROOTS: [&[&str]; 4] = [
    &["docs"],
    &["blog"],
    &["src", "pages"],
    &["versioned_docs", "*"],
];

/// The opening Docusaurus's own default exclusion covers under every content
/// path it reads, for a file and for a directory alike. A document named that
/// way is served at no URL of its own, so it is rendered into whichever pages
/// import it rather than published as one.
pub const UNROUTED_OPENING: &str = "_";

/// The opening Docusaurus expands to its own site directory, which is the
/// `site-alias` spelling written out.
const SITE_ALIAS: &str = "@site/";

/// Whether a generator rule owns this destination's opening. Only the opening
/// is read, so an ordinary directory named with an at sign is still a path.
#[must_use]
pub fn generator_alias(path_part: &str) -> bool {
    path_part.starts_with(SITE_ALIAS)
}

/// The openings a bundler's own inline request syntax reserves, which no tree
/// answers: webpack reads a leading `!` as the loaders it disables and the
/// rest of the string as a loader chain ending in the resource.
pub const BUNDLER_REQUESTS: [(&str, &[&str]); 1] = [("webpack", &["!", "-!"])];

/// Whether a bundler owns this destination outright. Only the opening is read,
/// so a file whose name carries the character anywhere else is still a path.
#[must_use]
pub fn bundler_request(path_part: &str) -> bool {
    BUNDLER_REQUESTS
        .iter()
        .flat_map(|(_, openings)| openings.iter())
        .any(|opening| path_part.starts_with(opening))
}

/// The delimiters a template engine substitutes before a page is served.
/// Jinja, Liquid, Nunjucks and Handlebars all spell an expression this way.
pub const TEMPLATE_EXPRESSIONS: [(&str, &str); 2] = [("{{", "}}"), ("{%", "%}")];

/// Whether a destination is an expression the build fills in rather than a
/// path. Both delimiters must be there in order, so a file whose name merely
/// carries a brace is a path like any other.
#[must_use]
pub fn template_expression(semantic: &str) -> bool {
    TEMPLATE_EXPRESSIONS.iter().any(|(open, close)| {
        semantic
            .split_once(open)
            .is_some_and(|(_, rest)| rest.contains(close))
    })
}

/// Every source path a modelled router would serve for this destination, in a
/// fixed order and without the destination itself.
#[must_use]
pub fn candidates(destination: &RepoPath) -> Vec<(Spelling, RepoPath)> {
    let mut out: Vec<(Spelling, RepoPath)> = Vec::new();
    for rule in &ROUTERS {
        for candidate in spellings(rule, destination) {
            if !out.iter().any(|(_, path)| *path == candidate.1) {
                out.push(candidate);
            }
        }
    }
    out
}

/// The source paths one rule would serve, in the order the resolver tries
/// them: the output name before the elided extension, and a directory's
/// README last.
#[must_use]
pub fn spellings(rule: &RouteRule, destination: &RepoPath) -> Vec<(Spelling, RepoPath)> {
    let raw = destination.as_bytes();
    let source = output_extension(raw);
    let mut out: Vec<(Spelling, RepoPath)> = Vec::new();
    let mut push = |spelling: Spelling, bytes: Option<Vec<u8>>| {
        let Some(path) = bytes.and_then(RepoPath::from_bytes) else {
            return;
        };
        if path.as_bytes() != raw && !out.iter().any(|(_, held)| *held == path) {
            out.push((spelling, path));
        }
    };
    if rule.serves(Spelling::OutputExtension) {
        push(Spelling::OutputExtension, source.clone());
    }
    if rule.serves(Spelling::Extensionless) {
        push(Spelling::Extensionless, extensionless(raw));
    }
    if rule.serves(Spelling::ReadmeIndex) {
        push(Spelling::ReadmeIndex, readme_index(raw));
        if rule.serves(Spelling::OutputExtension) {
            push(
                Spelling::ReadmeIndex,
                source.as_deref().and_then(readme_index),
            );
        }
    }
    out
}

/// Where a generator declared in the tree anchors this destination: each
/// directory to look under, in the generator's own order, paired with the
/// part of the destination that is relative to it. Empty when no generator on
/// the document's ancestor chain claims the destination, which leaves it
/// beside the document. A rule that turns on the construct is not asked
/// without one.
#[must_use]
pub fn anchors(
    snapshot: &SnapshotDiscovery,
    adapter: Adapter,
    document: &RepoPath,
    construct: Option<SourceConstruct>,
    is_image: bool,
    path_part: &str,
) -> Vec<(Vec<u8>, String)> {
    match adapter {
        Adapter::AsciiDoc => antora_anchor(snapshot, document, construct, path_part)
            .into_iter()
            .collect(),
        Adapter::Markdown | Adapter::Mdx => {
            markdown_anchors(snapshot, document, construct, is_image, path_part)
        }
        Adapter::Rst => sphinx_anchor(snapshot, document, construct, path_part)
            .into_iter()
            .collect(),
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
    let published = mkdocs_anchors(snapshot, document, construct, path_part);
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

/// Whether the build answers this path instead of the tree. A generator on
/// the document's ancestor chain either serves every relative destination
/// from a page URL this engine does not model, or serves the built page a
/// path names, and neither is a file any tree holds.
#[must_use]
pub fn unplaced(snapshot: &SnapshotDiscovery, document: &RepoPath, missing: &RepoPath) -> bool {
    let page = output_extension(missing.as_bytes()).is_some();
    ROUTERS.iter().any(|rule| {
        (rule.serves(Spelling::BuiltRoute) || (page && rule.serves(Spelling::BuiltPage)))
            && declared_root(snapshot, document.as_bytes(), rule).is_some()
    })
}

/// Whether no router publishes this document as a page of its own. A
/// Docusaurus content path excludes every name opening with `_`, a directory
/// as well as a file, so such a document is rendered into the pages that
/// import it and a fragment written in it names an identity of whichever page
/// that is. One of them may be rendered into several, so the tree fixes
/// neither the page nor the identities it publishes.
#[must_use]
pub fn unrouted(snapshot: &SnapshotDiscovery, adapter: Adapter, document: &RepoPath) -> bool {
    if !matches!(adapter, Adapter::Markdown | Adapter::Mdx) {
        return false;
    }
    let raw = document.as_bytes();
    let Some(root) =
        declared_root(snapshot, raw, &DOCUSAURUS).and_then(|site| content_root(&site, raw))
    else {
        return false;
    };
    raw.get(root.len()..)
        .unwrap_or_default()
        .split(|byte| *byte == b'/')
        .any(|segment| segment.starts_with(UNROUTED_OPENING.as_bytes()))
}

/// The directory a document sits in, without its trailing separator; empty
/// at the repository root.
#[must_use]
pub fn directory(document: &[u8]) -> &[u8] {
    document
        .iter()
        .rposition(|byte| *byte == b'/')
        .and_then(|split| document.get(..split))
        .unwrap_or_default()
}

/// An Antora resource ID, `[module:][family$]relative`, anchored at the family
/// directory of the named module in the document's own component. A cross
/// reference defaults to the page family and an image to the image family,
/// while an include without a family coordinate stays relative to the file
/// that includes it. A version or component coordinate names a catalogue this
/// tree does not hold, and a `./` or `../` relative is relative to the page.
fn antora_anchor(
    snapshot: &SnapshotDiscovery,
    document: &RepoPath,
    construct: Option<SourceConstruct>,
    path_part: &str,
) -> Option<(Vec<u8>, String)> {
    let construct = construct?;
    let default_family = if construct == SourceConstruct::AsciidocCrossReference {
        Some("page")
    } else if construct.is_image() {
        Some("image")
    } else if construct == SourceConstruct::AsciidocInclude {
        None
    } else {
        return None;
    };
    let (root, own_module) = antora_module(snapshot, document.as_bytes())?;
    if path_part.contains('@') {
        return None;
    }
    let (module, resource) = match path_part.split_once(':') {
        None => (own_module, path_part),
        Some((module, resource))
            if !module.is_empty() && !module.contains('/') && !resource.contains(':') =>
        {
            (module.as_bytes(), resource)
        }
        Some(_) => return None,
    };
    let (family, relative) = match resource.split_once('$') {
        Some((family, relative)) => (family, relative),
        None => (default_family?, resource),
    };
    let (_, family_directory) = ANTORA_FAMILIES.iter().find(|(name, _)| *name == family)?;
    if relative.is_empty()
        || relative.starts_with('/')
        || relative.starts_with("./")
        || relative.starts_with("../")
    {
        return None;
    }
    let module_directory = join(&join(root, b"modules"), module);
    Some((
        join(&module_directory, family_directory),
        relative.to_owned(),
    ))
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
    let Some(site) = declared_root(snapshot, document.as_bytes(), &DOCUSAURUS) else {
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
/// mdBook serves a page under a book's `src` at its path under the book root,
/// one directory shallower than the source, so a destination climbing past
/// that root names a page of the book that holds it and is read back as a
/// source under that book's own `src`. The reading beside the document comes
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
    let Some(root) = declared_root(snapshot, raw, &MDBOOK_PAGES) else {
        return Vec::new();
    };
    let Some(page) = raw
        .strip_prefix(join(&root, b"src").as_slice())
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
    let Some(owner) = declared_root(
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
    let source = join(&owner, b"src");
    let source = if under.is_empty() {
        source
    } else {
        join(&source, under)
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
    let root = declared_root(snapshot, document.as_bytes(), &ZOLA)?;
    let content = RepoPath::from_bytes(join(&root, b"content"))?;
    matches!(
        snapshot.locate(&content),
        Some(Located::ImpliedTree | Located::Entry(GitMode::Tree, _))
    )
    .then(|| content.as_bytes().to_vec())
}

/// Every route the snapshot's documents publish, and the document publishing
/// each. A route two documents claim is left out rather than decided between
/// them.
#[must_use]
pub(crate) fn published_routes(snapshot: &SnapshotDiscovery) -> BTreeMap<RepoPath, RepoPath> {
    let mut routes: BTreeMap<RepoPath, RepoPath> = BTreeMap::new();
    if !declares_docusaurus(snapshot) {
        return routes;
    }
    let mut claimed_twice: BTreeSet<RepoPath> = BTreeSet::new();
    for record in &snapshot.documents {
        let DocumentStatus::Scanned(scanned) = &record.status else {
            continue;
        };
        if !matches!(record.adapter, Some(Adapter::Markdown | Adapter::Mdx)) {
            continue;
        }
        let Some(route) = published_route(snapshot, &record.path, scanned.declared_name.as_deref())
        else {
            continue;
        };
        if routes.insert(route.clone(), record.path.clone()).is_some() {
            claimed_twice.insert(route);
        }
    }
    for route in claimed_twice {
        routes.remove(&route);
    }
    routes
}

/// Where one document is published: the name it declares in place of its own
/// file name, that name under the content root it sits in when it opens with
/// a slash, and the route its own path spells when it declares nothing.
fn published_route(
    snapshot: &SnapshotDiscovery,
    document: &RepoPath,
    declared: Option<&str>,
) -> Option<RepoPath> {
    let raw = document.as_bytes();
    let Some(name) = declared else {
        return RepoPath::from_bytes(page_route(raw));
    };
    let Some(absolute) = name.strip_prefix('/') else {
        return RepoPath::from_bytes(join(directory(raw), name.as_bytes()));
    };
    let site = declared_root(snapshot, raw, &DOCUSAURUS)?;
    let root = content_root(&site, raw).unwrap_or(site);
    RepoPath::from_bytes(join(&root, absolute.as_bytes()))
}

/// Whether this snapshot is published by Docusaurus at all. The site
/// directory names the directories it reads in a configuration this engine
/// does not read, and a site under `website/` commonly reads `../docs`, so
/// the declaration is read from the whole tree rather than from the
/// document's own ancestor chain.
fn declares_docusaurus(snapshot: &SnapshotDiscovery) -> bool {
    snapshot.entries.iter().any(|(path, (mode, _))| {
        matches!(mode, GitMode::RegularFile | GitMode::ExecutableFile)
            && DOCUSAURUS.declared_by.iter().any(|name| {
                path.as_bytes().rsplit(|byte| *byte == b'/').next() == Some(name.as_bytes())
            })
    })
}

/// The name a document declares for its own page in frontmatter, which is
/// `slug` before `id`, each a plain scalar on a line of its own. Nothing else
/// in the region is read, and the region stays opaque to the grammar.
#[must_use]
pub(crate) fn declared_name(adapter: Adapter, source: &[u8]) -> Option<String> {
    if !matches!(adapter, Adapter::Markdown | Adapter::Mdx) {
        return None;
    }
    let region = amiss_md::frontmatter::recognize(source)?;
    let body = source.get(region.bom_bytes..region.suffix_offset)?;
    let mut id = None;
    let mut slug = None;
    for line in amiss_md::lines::scan(body) {
        let content = line.content(body);
        if let Some(value) = scalar(content, b"slug:") {
            slug = slug.or(Some(value));
        } else if let Some(value) = scalar(content, b"id:") {
            id = id.or(Some(value));
        }
    }
    slug.or(id).map(str::to_owned)
}

/// One key's value where the key opens the line: a scalar closed by the quote
/// it opened with, or plain text up to an inline comment.
fn scalar<'a>(line: &'a [u8], key: &[u8]) -> Option<&'a str> {
    let text = std::str::from_utf8(line.strip_prefix(key)?)
        .ok()?
        .trim_matches([' ', '\t']);
    for quote in ['"', '\''] {
        if let Some(inner) = text
            .strip_prefix(quote)
            .and_then(|rest| rest.strip_suffix(quote))
        {
            return spelled(inner);
        }
    }
    spelled(
        text.split_once(" #")
            .map_or(text, |(value, _comment)| value)
            .trim_end(),
    )
}

/// A value this reader spells out rather than guesses at: one line of
/// ordinary text. A block or flow opening, an alias, and an empty value are
/// declarations it declines.
fn spelled(value: &str) -> Option<&str> {
    let declined = value.is_empty()
        || value.starts_with(['>', '|', '&', '*', '{', '[', '"', '\''])
        || value.contains(char::is_control);
    (!declined).then_some(value)
}

/// A destination the browser resolves rather than the generator: mkdocs
/// publishes every page at a directory of its own name and rewrites no
/// destination written as raw HTML, so such a destination is relative to the
/// page's directory and its trailing slash names that page rather than a tree.
/// The document's own directory follows, so a destination that reached a file
/// beside the source still reaches it.
fn mkdocs_anchors(
    snapshot: &SnapshotDiscovery,
    document: &RepoPath,
    construct: Option<SourceConstruct>,
    path_part: &str,
) -> Vec<(Vec<u8>, String)> {
    if !matches!(
        construct,
        Some(SourceConstruct::HtmlAnchor | SourceConstruct::HtmlImage)
    ) || path_part.is_empty()
        || path_part.starts_with('/')
        || scheme(path_part).is_some()
        || declared_root(snapshot, document.as_bytes(), &MKDOCS).is_none()
    {
        return Vec::new();
    }
    let relative = path_part.strip_suffix('/').unwrap_or(path_part);
    let beside = directory(document.as_bytes());
    let published = page_route(document.as_bytes());
    let mut out = vec![(published, relative.to_owned())];
    if !out.iter().any(|(held, _)| held == beside) {
        out.push((beside.to_vec(), relative.to_owned()));
    }
    out
}

/// The route a page is published at, which mkdocs serves from a directory of
/// that name: the source name without its extension, or the document's own
/// directory when the source is that directory's index, the pair of names
/// both mkdocs and Docusaurus publish at the directory itself.
fn page_route(document: &[u8]) -> Vec<u8> {
    let parent = directory(document);
    let name = document.rsplit(|byte| *byte == b'/').next().unwrap_or(b"");
    let stem = match name.iter().rposition(|byte| *byte == b'.') {
        Some(dot) if dot > 0 => name.get(..dot).unwrap_or_default(),
        Some(_) | None => name,
    };
    if stem == b"index" || stem == b"README" {
        return parent.to_vec();
    }
    join(parent, stem)
}

/// The plugin content path a document sits under, when it sits under one of
/// the paths Docusaurus reads by default.
fn content_root(site: &[u8], document: &[u8]) -> Option<Vec<u8>> {
    let relative = document.strip_prefix(site)?;
    let relative = if site.is_empty() {
        relative
    } else {
        relative.strip_prefix(b"/")?
    };
    let segments: Vec<&[u8]> = relative.split(|byte| *byte == b'/').collect();
    DOCUSAURUS_CONTENT_ROOTS.iter().find_map(|pattern| {
        let head = segments.get(..pattern.len())?;
        let matched = segments.len() > pattern.len()
            && pattern
                .iter()
                .zip(head)
                .all(|(want, have)| *want == "*" || want.as_bytes() == *have);
        matched.then(|| join(site, &head.join(&b'/')))
    })
}

/// A source-root-absolute `:doc:` target, anchored at the directory holding
/// `conf.py`, with the source suffix an extensionless docname takes.
fn sphinx_anchor(
    snapshot: &SnapshotDiscovery,
    document: &RepoPath,
    construct: Option<SourceConstruct>,
    path_part: &str,
) -> Option<(Vec<u8>, String)> {
    if construct? != SourceConstruct::RstDocRole {
        return None;
    }
    let relative = path_part.strip_prefix('/')?;
    if relative.is_empty() || relative.starts_with('/') {
        return None;
    }
    let root = declared_root(snapshot, document.as_bytes(), &SPHINX)?;
    let named = relative
        .rsplit('/')
        .next()
        .is_some_and(|last| last.contains('.'));
    let relative = if named {
        relative.to_owned()
    } else {
        format!("{relative}.rst")
    };
    Some((root, relative))
}

/// The nearest directory on the document's ancestor chain holding one of the
/// files that declare this rule's generator.
pub(crate) fn declared_root(
    snapshot: &SnapshotDiscovery,
    document: &[u8],
    rule: &RouteRule,
) -> Option<Vec<u8>> {
    let mut end = document.len();
    loop {
        let cut = document.get(..end)?.iter().rposition(|byte| *byte == b'/');
        let directory = cut.and_then(|cut| document.get(..cut)).unwrap_or_default();
        if rule
            .declared_by
            .iter()
            .any(|name| regular_file(snapshot, join(directory, name.as_bytes())))
        {
            return Some(directory.to_vec());
        }
        end = cut?;
    }
}

fn regular_file(snapshot: &SnapshotDiscovery, path: Vec<u8>) -> bool {
    RepoPath::from_bytes(path).is_some_and(|path| {
        matches!(
            snapshot.locate(&path),
            Some(Located::Entry(
                GitMode::RegularFile | GitMode::ExecutableFile,
                _
            ))
        )
    })
}

fn join(directory: &[u8], name: &[u8]) -> Vec<u8> {
    if directory.is_empty() {
        return name.to_vec();
    }
    [directory, b"/", name].concat()
}

fn output_extension(raw: &[u8]) -> Option<Vec<u8>> {
    let stem = raw.strip_suffix(b".html")?;
    (!stem.is_empty()).then(|| [stem, b".md"].concat())
}

fn extensionless(raw: &[u8]) -> Option<Vec<u8>> {
    let last = raw.rsplit(|byte| *byte == b'/').next()?;
    let named = !last.is_empty()
        && !last.ends_with(b".md")
        && !last.ends_with(b".markdown")
        && !last.ends_with(b".html");
    named.then(|| [raw, b".md"].concat())
}

/// Only a directory's index is answered by its README, because that is the
/// shape the harvest covered.
fn readme_index(raw: &[u8]) -> Option<Vec<u8>> {
    let cut = raw.iter().rposition(|byte| *byte == b'/')?;
    let head = raw.get(..=cut)?;
    let tail = raw.get(cut.saturating_add(1)..)?;
    (tail == b"index.md").then(|| [head, b"README.md"].concat())
}
