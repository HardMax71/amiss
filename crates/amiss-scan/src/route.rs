use std::collections::{BTreeMap, BTreeSet};

use amiss_wire::controls::{GitMode, SourceConstruct};
use amiss_wire::model::{Adapter, RepoPath};
use amiss_wire::uri::scheme;

use crate::discovery::{DocumentStatus, Located, SnapshotDiscovery};
use crate::document::excluded_by_built_in;
use crate::resolve::syntax::normalized_path_under;

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
    PageUrl,
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

pub(crate) const ANTORA: RouteRule = RouteRule {
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

pub(crate) const ELEVENTY: RouteRule = RouteRule {
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

pub(crate) const HUGO: RouteRule = RouteRule {
    name: "hugo",
    declared_by: &[
        "hugo.toml",
        "hugo.yaml",
        "hugo.json",
        "config.toml",
        "config.yaml",
        "config.json",
        "config/_default/hugo.toml",
        "config/_default/hugo.yaml",
        "config/_default/hugo.json",
        "config/_default/config.toml",
        "config/_default/config.yaml",
        "config/_default/config.json",
    ],
    serves: &[Spelling::BuiltRoute],
};

/// The names more than one generator is configured under, which select a rule
/// only where what the file binds says whose it is.
pub(crate) const SHARED_CONFIGS: [&str; 3] = ["config.toml", "config.yaml", "config.json"];

const JEKYLL: RouteRule = RouteRule {
    name: "jekyll",
    declared_by: &["_config.yml"],
    serves: &[Spelling::BuiltRoute],
};

/// A site publishing every page at a directory of its own name and rewriting
/// no destination, which no file in the tree shows. Hugo builds it by default,
/// Jekyll's pretty permalink produces it, and Eleventy and Astro build it
/// unless asked for a file, while `hugo.toml` says which generator builds a
/// tree and nothing about the URLs it serves. So this rule is selected by the
/// declaration alone, under a name no one generator owns.
pub(crate) const DIRECTORY_PAGES: RouteRule = RouteRule {
    name: "directory-pages",
    declared_by: &[],
    serves: &[Spelling::PageUrl],
};

/// Every router rule the resolver knows. A spelling reaches a source file only
/// when that file is in the tree, so a rule can widen what resolves and can
/// never invent a target.
pub const ROUTERS: [RouteRule; 14] = [
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
    DIRECTORY_PAGES,
    JEKYLL,
];

/// The file a repository names its own router in, for a tree whose generator
/// is configured somewhere else, and the two keys it opens a line with. The
/// router is a router name, and a declaration turns on that router's
/// resolving spellings under the directory holding the file. The base is the
/// URL path that router serves the directory at, which is what a site route
/// is read against.
pub const ROUTER_DECLARATION: &str = ".amiss/router.yml";
const DECLARED_ROUTER: &[u8] = b"router:";
const PUBLISHED_BASE: &[u8] = b"base:";

/// The frontmatter key a page lists the URLs it moved away from under, which
/// a router serving page URLs answers with the page that carries the block.
const PAGE_REDIRECTS: &[u8] = b"aliases:";

/// Every spelling a declaration turns on, which is every spelling that
/// resolves a destination against the tree and nothing else. A rule serving
/// one this list omits is a rule no declaration can name: `built-route` and
/// `built-page` withhold an answer rather than serve a file, so a repository
/// cannot clear a finding by declaring anything, and the three a router serves
/// in every tree need no declaring.
pub const DECLARABLE: [Spelling; 6] = [
    Spelling::SiteAlias,
    Spelling::ContentRoot,
    Spelling::DocumentId,
    Spelling::SourceRoot,
    Spelling::DirectoryUrl,
    Spelling::PageUrl,
];

/// The two keys an Antora component descriptor opens a line with that this
/// rule reads: the component the root contributes to, and the block reserved
/// for the extensions that assemble that component when the site is built.
const ANTORA_COMPONENT: &[u8] = b"name:";
const ANTORA_EXTENSIONS: &[u8] = b"ext:";

/// The key a Sphinx configuration opens a line with to say which suffixes it
/// reads, the parser name that means this engine's reStructuredText, and the
/// suffix a source file carries where nothing declares another.
const SOURCE_SUFFIX: &[u8] = b"source_suffix";

/// The keys a Hugo configuration opens a line with to say where its content
/// sits and where its site is served, and the root Hugo reads without them.
const CONTENT_DIR: &[u8] = b"contentDir";
const BASE_URL: &[u8] = b"baseURL";
const DEFAULT_LANGUAGE: &[u8] = b"defaultContentLanguage";
const LANGUAGE_IN_SUBDIR: &[u8] = b"defaultContentLanguageInSubdir";
const LANGUAGE_TABLE: &str = "languages.";
const MOUNT_TABLE: &str = "[[module.mounts]]";
const MOUNT_SOURCE: &[u8] = b"source";
const MOUNT_TARGET: &[u8] = b"target";
const DEFAULT_CONTENT_DIR: &str = "content";
const BOOK_TABLE: &[u8] = b"[book]";
const BOOK_SOURCE: &[u8] = b"src";
const DEFAULT_BOOK_SOURCE: &str = "src";
const RESTRUCTUREDTEXT: &str = "restructuredtext";
pub const DEFAULT_SOURCE_SUFFIX: &str = ".rst";

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
/// path. The opening decides on its own, since a host grammar ends a
/// destination where its own syntax says and hands back an expression with
/// the closer cut off, so a file whose name merely carries a brace is a path
/// like any other and one carrying an opening is not.
#[must_use]
pub fn template_expression(semantic: &str) -> bool {
    TEMPLATE_EXPRESSIONS
        .iter()
        .any(|(open, _)| semantic.contains(open))
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
        for suffix in PAGE_SUFFIXES {
            push(Spelling::Extensionless, extensionless(raw, suffix));
        }
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

/// Whether a document sits inside one project, which the repository root
/// holds every document of.
fn within(document: &[u8], project: &[u8]) -> bool {
    project.is_empty()
        || document
            .strip_prefix(project)
            .is_some_and(|rest| rest.starts_with(b"/"))
}

/// What one route names under a declared base: the rest of it once the base
/// it opens with is off.
fn under_base<'a>(base: &str, route: &'a str) -> Option<&'a str> {
    let rest = route.strip_prefix(base)?;
    if base.is_empty() {
        Some(rest)
    } else {
        rest.strip_prefix('/')
    }
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
        .is_some_and(|(path, kind)| {
            snapshot
                .locate(&crate::resolve::routed(snapshot, &path, kind))
                .is_some()
        })
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
pub fn unplaced(snapshot: &SnapshotDiscovery, document: &RepoPath, missing: &RepoPath) -> bool {
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
pub fn unrouted(snapshot: &SnapshotDiscovery, adapter: Adapter, document: &RepoPath) -> bool {
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

/// What one `antora.yml` says about its own root: the component it contributes
/// to, and whether it reserves the `ext` block Antora hands to the extensions
/// that assemble the component. Each is a plain scalar on a line of its own,
/// read the way a document's frontmatter is, and nothing else in the file is
/// looked at.
#[must_use]
pub(crate) fn antora_descriptor(source: &[u8]) -> Option<(String, bool)> {
    let mut name = None;
    let mut extended = false;
    for line in amiss_md::lines::scan(source) {
        let content = line.content(source);
        if let Some(value) = scalar(content, ANTORA_COMPONENT) {
            name.get_or_insert_with(|| value.to_owned());
        } else if content.starts_with(ANTORA_EXTENSIONS) {
            extended = true;
        }
    }
    name.map(|name| (name, extended))
}

/// What one `.amiss/router.yml` says about the directory it sits in: the
/// router serving it, and the URL path that router serves it at. Each is a
/// plain scalar on a line of its own, read the way a component descriptor is.
/// A name no rule in the table carries is declined here, so a declaration
/// reaches exactly the rules this engine already models, and a file naming no
/// router declares nothing whatever else it holds.
#[must_use]
pub(crate) fn declared_router(source: &[u8]) -> Option<(String, Option<String>)> {
    let mut router = None;
    let mut base = None;
    for line in amiss_md::lines::scan(source) {
        let content = line.content(source);
        if let Some(value) = scalar(content, DECLARED_ROUTER)
            .filter(|value| ROUTERS.iter().any(|rule| rule.name == *value))
        {
            router.get_or_insert_with(|| value.to_owned());
        } else if let Some(value) = scalar(content, PUBLISHED_BASE).and_then(site_base) {
            base.get_or_insert_with(|| value.to_owned());
        }
    }
    router.map(|router| (router, base))
}

/// The path prefix one base declares, which is what every site route the
/// directory answers opens with: an absolute URL path without the slashes
/// bounding it, so a directory served at the site root declares nothing
/// before the route.
fn site_base(value: &str) -> Option<&str> {
    value
        .strip_prefix('/')
        .map(|path| path.trim_end_matches('/'))
}

/// Every content root one Hugo configuration names and the path each is
/// served under: the project's own, where a key opens a line of the file
/// itself, and one for every language table. Where the file names no root,
/// Hugo reads `content`.
#[must_use]
pub(crate) fn hugo_project(source: &[u8]) -> BTreeMap<String, String> {
    let project = project_lines(source);
    let bound = |key| project.iter().find_map(|line| configured(line, key));
    let base = bound(BASE_URL).map(served_under).unwrap_or_default();
    let root = bound(CONTENT_DIR)
        .map(str::to_owned)
        .or_else(|| mounted_root(source))
        .unwrap_or_else(|| DEFAULT_CONTENT_DIR.to_owned());
    let mut roots = BTreeMap::from([(root, base.clone())]);
    for (code, directory) in language_roots(source) {
        let subdir = code != bound(DEFAULT_LANGUAGE).unwrap_or_default()
            || bound(LANGUAGE_IN_SUBDIR) == Some("true");
        let served = if subdir {
            under(&base, &code)
        } else {
            base.clone()
        };
        roots.insert(directory, served);
    }
    roots
}

/// Where one language of a site is served: under the site's own base, and
/// under its code below that.
fn under(base: &str, code: &str) -> String {
    if base.is_empty() {
        code.to_owned()
    } else {
        format!("{base}/{code}")
    }
}

/// The directory a module mount reads the content of a site from, where the
/// configuration names exactly one. A mount binds a source to a target, and
/// the one targeting the content directory is where the pages are. Several
/// such mounts are a site composed of parts this does not take apart, so
/// none of them names a root.
fn mounted_root(source: &[u8]) -> Option<String> {
    let mut mounts: Vec<(Option<String>, bool)> = Vec::new();
    let mut inside = false;
    for line in amiss_md::lines::scan(source) {
        let content = line.content(source).trim_ascii_start();
        if content.starts_with(b"[") {
            inside = content.starts_with(MOUNT_TABLE.as_bytes());
            if inside {
                mounts.push((None, false));
            }
        } else if let Some(mount) = inside.then(|| mounts.last_mut()).flatten() {
            if let Some(value) = configured(content, MOUNT_SOURCE) {
                mount.0 = Some(value.to_owned());
            } else if configured(content, MOUNT_TARGET) == Some(DEFAULT_CONTENT_DIR) {
                mount.1 = true;
            }
        }
    }
    let mut roots: Vec<String> = mounts
        .into_iter()
        .filter(|(_, content)| *content)
        .filter_map(|(source, _)| source)
        .collect();
    (roots.len() == 1).then(|| roots.remove(0))
}

/// The content root every language table names, paired with the code that
/// table is keyed by. A table header ends the table above it, so a key
/// reaches only the language whose header opened it.
fn language_roots(source: &[u8]) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let mut current: Option<String> = None;
    for line in amiss_md::lines::scan(source) {
        let content = line.content(source).trim_ascii_start();
        if content.starts_with(b"[") {
            current = language_table(content);
        } else if let (Some(code), Some(directory)) =
            (current.as_ref(), configured(content, CONTENT_DIR))
        {
            found.push((code.clone(), directory.to_owned()));
        }
    }
    found
}

/// The language one table header keys, where the header opens a table of the
/// configuration's own language map and names exactly one language.
fn language_table(header: &[u8]) -> Option<String> {
    let inner = header
        .strip_prefix(b"[")?
        .split(|byte| *byte == b']')
        .next()?;
    let code = std::str::from_utf8(inner)
        .ok()?
        .strip_prefix(LANGUAGE_TABLE)?;
    (!code.is_empty() && !code.contains('.')).then(|| code.to_owned())
}

/// The directory one `book.toml` reads its chapters from, as a repository
/// path: the `src` key of its `[book]` table read against the directory the
/// file sits in, and `src` where the table names none. A key this reader
/// cannot spell, or a path climbing out of the repository, names no directory,
/// so the book is left unread rather than read under the default.
#[must_use]
pub(crate) fn book_source(root: &[u8], source: &[u8]) -> Option<Vec<u8>> {
    let mut inside = false;
    let mut named: Option<Option<&str>> = None;
    for line in amiss_md::lines::scan(source) {
        let content = line.content(source).trim_ascii_start();
        if content.starts_with(b"[") {
            inside = content.starts_with(BOOK_TABLE);
        } else if inside && named.is_none() && assigned(content, BOOK_SOURCE).is_some() {
            named = Some(configured(content, BOOK_SOURCE));
        }
    }
    let directory = named.unwrap_or(Some(DEFAULT_BOOK_SOURCE))?;
    normalized_path_under(root, false, directory)
        .ok()
        .map(|(path, _kind)| path.as_bytes().to_vec())
}

/// The lines a configuration file binds at its own top level, where an
/// indented line and everything after the first table header belong to a
/// table rather than to the project.
fn project_lines(source: &[u8]) -> Vec<&[u8]> {
    amiss_md::lines::scan(source)
        .map(|line| line.content(source))
        .take_while(|content| !content.starts_with(b"["))
        .filter(|content| !content.first().is_some_and(u8::is_ascii_whitespace))
        .collect()
}

/// What one configuration binds to a key, spelled the way the file it sits
/// in spells a binding: an equals sign in TOML and a colon in YAML.
fn configured<'a>(line: &'a [u8], key: &[u8]) -> Option<&'a str> {
    match assigned(line, key) {
        Some(value) => scalar(value, b""),
        None => scalar(line, &[key, b":"].concat()),
    }
}

/// The path a site URL is served under, without the slashes bounding it: the
/// authority is off, so a site at the root of its host declares nothing
/// before the route, the way a base does.
fn served_under(url: &str) -> String {
    let path = url.split_once("://").map_or(url, |(_scheme, rest)| {
        rest.split_once('/').map_or("", |(_host, path)| path)
    });
    path.trim_matches('/').to_owned()
}

/// Which suffixes one `conf.py` says Sphinx reads as reStructuredText. A
/// Python assignment binds a name at the top level, so the key opens the line
/// the way a descriptor's does, and an indented call or a commented-out line
/// declares nothing. The value is one quoted suffix, or a mapping closed on
/// the same line whose value names the reStructuredText parser. A mapping left
/// open, a list, and a name spelled anywhere else are declined rather than
/// parsed.
#[must_use]
pub(crate) fn source_suffixes(source: &[u8]) -> BTreeSet<String> {
    let mut declared = BTreeSet::new();
    for line in amiss_md::lines::scan(source) {
        let Some(value) = assigned(line.content(source), SOURCE_SUFFIX) else {
            continue;
        };
        match scalar(value, b"") {
            Some(single) => {
                declared.insert(single.to_owned());
            }
            None => declared.extend(mapped(value)),
        }
    }
    declared
}

/// What one top-level Python assignment binds, where the key opens the line
/// and an equals sign follows it.
fn assigned<'a>(line: &'a [u8], key: &[u8]) -> Option<&'a [u8]> {
    line.strip_prefix(key)?
        .trim_ascii_start()
        .strip_prefix(b"=")
}

/// Every key a one-line mapping binds to the reStructuredText parser, each
/// side read as the scalar it is quoted as.
fn mapped(value: &[u8]) -> Vec<String> {
    let Some(entries) = std::str::from_utf8(value)
        .ok()
        .map(|text| text.trim_matches([' ', '\t']))
        .and_then(|text| text.strip_prefix('{'))
        .and_then(|text| text.strip_suffix('}'))
    else {
        return Vec::new();
    };
    entries
        .split(',')
        .filter_map(|entry| entry.split_once(':'))
        .filter(|(_, parser)| scalar(parser.as_bytes(), b"") == Some(RESTRUCTUREDTEXT))
        .filter_map(|(suffix, _)| scalar(suffix.as_bytes(), b"").map(str::to_owned))
        .collect()
}

/// Whether a Sphinx root reads this path as one of its own source files,
/// which is the nearest `conf.py` above it declaring the suffix it carries.
#[must_use]
pub(crate) fn declared_source(snapshot: &SnapshotDiscovery, path: &[u8]) -> bool {
    let Some(root) = declared_root(snapshot, path, SPHINX.declared_by) else {
        return false;
    };
    snapshot.source_suffixes.get(&root).is_some_and(|declared| {
        declared
            .iter()
            .any(|suffix| path.ends_with(suffix.as_bytes()))
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

/// Every route the snapshot's documents claim and the document claiming each,
/// as the routes pages are published at and, apart from them, the page URLs
/// pages declare they moved away from. The two stay apart because a published
/// route answers a path as well as a URL while a redirect answers only a URL.
#[must_use]
pub(crate) fn published_routes(
    snapshot: &SnapshotDiscovery,
) -> (BTreeMap<RepoPath, RepoPath>, BTreeMap<RepoPath, RepoPath>) {
    let names = published_by(snapshot, &DOCUSAURUS);
    let redirects = published_by(snapshot, &DIRECTORY_PAGES);
    let mut published: Vec<(RepoPath, RepoPath)> = Vec::new();
    let mut moved: Vec<(RepoPath, RepoPath)> = Vec::new();
    if !names && !redirects {
        return (sole_claims(published), sole_claims(moved));
    }
    for record in &snapshot.documents {
        let DocumentStatus::Scanned(scanned) = &record.status else {
            continue;
        };
        if !matches!(record.adapter, Some(Adapter::Markdown | Adapter::Mdx)) {
            continue;
        }
        if names
            && let Some(route) =
                published_route(snapshot, &record.path, scanned.declared_name.as_deref())
        {
            published.push((route, record.path.clone()));
        }
        if redirects {
            moved.extend(
                moved_from(snapshot, &record.path, &scanned.declared_redirects)
                    .into_iter()
                    .map(|route| (route, record.path.clone())),
            );
        }
    }
    (sole_claims(published), sole_claims(moved))
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

/// The document each route names, from the claims the documents made. A route
/// two of them claim is left out rather than decided between them.
fn sole_claims(claims: Vec<(RepoPath, RepoPath)>) -> BTreeMap<RepoPath, RepoPath> {
    let mut routes: BTreeMap<RepoPath, RepoPath> = BTreeMap::new();
    let mut claimed_twice: BTreeSet<RepoPath> = BTreeSet::new();
    for (route, document) in claims {
        if routes
            .insert(route.clone(), document.clone())
            .is_some_and(|held| held != document)
        {
            claimed_twice.insert(route);
        }
    }
    for route in claimed_twice {
        routes.remove(&route);
    }
    routes
}

/// Every page URL one document declares it moved away from. The block holds
/// what a browser would ask for, so each entry is read against the directory
/// the page's own URL sits in, which is one level above the route the page is
/// published at. An entry opening with a slash names a site route, and stays
/// where every site route stays. A router that serves no page URL above this
/// document leaves the block meaning nothing.
fn moved_from(
    snapshot: &SnapshotDiscovery,
    document: &RepoPath,
    declared: &[String],
) -> Vec<RepoPath> {
    if declared.is_empty() || site_root(snapshot, document.as_bytes(), &DIRECTORY_PAGES).is_none() {
        return Vec::new();
    }
    let published = page_route(document.as_bytes(), true);
    let parent = directory(&published).to_vec();
    declared
        .iter()
        .filter(|entry| !entry.starts_with('/'))
        .filter_map(|entry| normalized_path_under(&parent, false, entry).ok())
        .map(|(route, _kind)| route)
        .collect()
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
        return RepoPath::from_bytes(page_route(raw, false));
    };
    let Some(absolute) = name.strip_prefix('/') else {
        return RepoPath::from_bytes(join(directory(raw), name.as_bytes()));
    };
    let site = declared_root(snapshot, raw, DOCUSAURUS.declared_by)?;
    let root = content_root(&site, raw).unwrap_or(site);
    RepoPath::from_bytes(join(&root, absolute.as_bytes()))
}

/// Every directory this tree declares one rule's generator in. A declaration
/// under a tree the scan excludes belongs to a fixture or a dependency rather
/// than to a site the repository publishes, so it names no directory here.
fn declaring_directories(snapshot: &SnapshotDiscovery, rule: &RouteRule) -> BTreeSet<Vec<u8>> {
    snapshot
        .entries
        .iter()
        .filter(|(path, (mode, _))| {
            matches!(mode, GitMode::RegularFile | GitMode::ExecutableFile)
                && !excluded_by_built_in(path.as_bytes())
        })
        .filter_map(|(path, _)| {
            let name = rule
                .declared_by
                .iter()
                .filter(|name| declaring_directory(path.as_bytes(), name).is_some())
                .max_by_key(|name| name.len())?;
            let bound = !SHARED_CONFIGS.contains(name)
                || snapshot
                    .bound_configs
                    .get(path)
                    .is_some_and(|owner| *owner == rule.declared_by);
            declaring_directory(path.as_bytes(), name)
                .filter(|_| bound)
                .map(<[u8]>::to_vec)
        })
        .collect()
}

/// Whether the routes one rule serves are read for this tree at all: a
/// configuration file selecting the rule somewhere in the tree, or a
/// declaration naming it.
fn published_by(snapshot: &SnapshotDiscovery, rule: &RouteRule) -> bool {
    !declaring_directories(snapshot, rule).is_empty()
        || snapshot
            .declared_routers
            .values()
            .any(|(declared, _)| declared == rule.name)
}

/// The directory each generator this tree declares exactly once is configured
/// in. Which sites a tree declares is a question about the whole tree, since
/// a site under `website/` commonly reads `../docs` and the configuration
/// naming that directory is one this engine does not read. A generator
/// declared in several places fixes no owner for a document outside them all.
#[must_use]
pub(crate) fn sole_sites(snapshot: &SnapshotDiscovery) -> BTreeMap<&'static str, Vec<u8>> {
    ROUTERS
        .iter()
        .filter_map(|rule| {
            let mut declaring = declaring_directories(snapshot, rule).into_iter();
            let root = declaring.next()?;
            declaring.next().is_none().then_some((rule.name, root))
        })
        .collect()
}

/// Which site owns a document: the nearest configuration above it, the router
/// the repository declares for the directory, and failing both the single
/// site the tree declares. A rule serving a built page or a built route
/// answers for the build rather than for the tree, so widening one would
/// withhold an answer for a document no site publishes, and those keep the
/// ancestor walk. The rest can only reach a file the tree already holds.
pub fn site_root(snapshot: &SnapshotDiscovery, path: &[u8], rule: &RouteRule) -> Option<Vec<u8>> {
    let widens = !rule.serves(Spelling::BuiltRoute) && !rule.serves(Spelling::BuiltPage);
    let tree = snapshot.sole_sites.get(rule.name).filter(|_| widens);
    declared_root(snapshot, path, rule.declared_by)
        .or_else(|| declared_site(snapshot, path, rule))
        .or_else(|| tree.cloned())
}

/// The nearest directory above the document whose own declaration names this
/// rule's router. A repository may only name a rule whose every spelling
/// resolves a destination against the tree, so what it declares widens what
/// resolves and withholds no answer.
fn declared_site(
    snapshot: &SnapshotDiscovery,
    document: &[u8],
    rule: &RouteRule,
) -> Option<Vec<u8>> {
    if !declarable(rule) {
        return None;
    }
    let root = ancestor_root(document, &|directory| {
        declared_at(snapshot, directory).is_some()
    })?;
    (declared_at(snapshot, &root)?.0 == rule.name).then_some(root)
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

/// What a declaration sitting in this exact directory names. The declarations
/// a comparison reads are the candidate's on both sides, so this asks what
/// was declared rather than which tree holds the file.
fn declared_at<'snapshot>(
    snapshot: &'snapshot SnapshotDiscovery,
    directory: &[u8],
) -> Option<&'snapshot (String, Option<String>)> {
    RepoPath::from_bytes(join(directory, ROUTER_DECLARATION.as_bytes()))
        .and_then(|path| snapshot.declared_routers.get(&path))
}

/// Whether a repository's own declaration may name this rule.
#[must_use]
pub fn declarable(rule: &RouteRule) -> bool {
    rule.serves
        .iter()
        .all(|spelling| DECLARABLE.contains(spelling))
}

/// Whether a path's own name is one of the files declaring this rule's
/// generator, wherever in the tree it sits.
pub(crate) fn declares(rule: &RouteRule, path: &RepoPath) -> bool {
    rule.declared_by
        .iter()
        .any(|name| declaring_directory(path.as_bytes(), name).is_some())
}

/// The directory a file declares its rule from, where its path spells one of
/// the rule's names: the name is read against that directory, so a file under
/// `config/_default` declares the project two levels above it.
pub(crate) fn declaring_directory<'a>(path: &'a [u8], name: &str) -> Option<&'a [u8]> {
    let above = path.strip_suffix(name.as_bytes())?;
    if above.is_empty() {
        Some(above)
    } else {
        above.strip_suffix(b"/")
    }
}

/// The rule a configuration under a shared name belongs to, by the key it
/// binds for its site's address: Hugo reads `baseURL` in any case, and Zola
/// reads `base_url`. A file binding neither configures no site.
#[must_use]
pub(crate) fn addressed_rule(source: &[u8]) -> Option<&'static RouteRule> {
    let project = project_lines(source);
    let binds = |key: &[u8]| {
        project.iter().any(|line| {
            let rest = line.get(key.len()..).map(<[u8]>::trim_ascii_start);
            line.get(..key.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(key))
                && matches!(rest.and_then(<[u8]>::first), Some(b'=' | b':'))
        })
    };
    if binds(b"base_url") {
        Some(&ZOLA)
    } else if binds(b"baseurl") {
        Some(&HUGO)
    } else {
        None
    }
}

/// What a document declares about its own publication in frontmatter: the
/// name it publishes under, which is `slug` before `id`, and the page URLs it
/// is also served at. A name is a plain scalar on a line of its own and the
/// URLs are the block under `aliases`. Nothing else in the region is read, and
/// the region stays opaque to the grammar.
#[must_use]
pub(crate) fn declared_publication(
    adapter: Adapter,
    source: &[u8],
) -> (Option<String>, Vec<String>) {
    if !matches!(adapter, Adapter::Markdown | Adapter::Mdx) {
        return (None, Vec::new());
    }
    let Some(body) = amiss_md::frontmatter::recognize(source)
        .and_then(|region| source.get(region.bom_bytes..region.suffix_offset))
    else {
        return (None, Vec::new());
    };
    let mut id = None;
    let mut slug = None;
    let mut redirects: Vec<String> = Vec::new();
    let mut listing = false;
    for line in amiss_md::lines::scan(body) {
        let content = line.content(body);
        if listing && let Some(entry) = sequence_entry(content) {
            redirects.push(entry.to_owned());
            continue;
        }
        listing = opens_sequence(content, PAGE_REDIRECTS);
        if let Some(value) = scalar(content, b"slug:") {
            slug = slug.or(Some(value));
        } else if let Some(value) = scalar(content, b"id:") {
            id = id.or(Some(value));
        }
    }
    (slug.or(id).map(str::to_owned), redirects)
}

/// Whether a key opens a block sequence: it opens the line and carries no
/// value of its own, so what follows is the block rather than a scalar.
fn opens_sequence(line: &[u8], key: &[u8]) -> bool {
    line.strip_prefix(key)
        .is_some_and(|rest| rest.iter().all(|byte| matches!(*byte, b' ' | b'\t')))
}

/// One entry of the block a key opened: a dash at whatever depth, a space,
/// and the value a scalar is read as. A line shaped any other way is not an
/// entry and closes the block, so nothing outside it is read.
fn sequence_entry(line: &[u8]) -> Option<&str> {
    let opened = line
        .iter()
        .position(|byte| !matches!(*byte, b' ' | b'\t'))?;
    let entry = line.get(opened..)?;
    entry
        .starts_with(b"- ")
        .then(|| scalar(entry, b"-"))
        .flatten()
}

/// One key's value where the key opens the line: a scalar closed by the quote
/// it opened with, or plain text up to an inline comment.
fn scalar<'a>(line: &'a [u8], key: &[u8]) -> Option<&'a str> {
    let text = std::str::from_utf8(line.strip_prefix(key)?)
        .ok()?
        .trim_matches([' ', '\t']);
    for quote in ['"', '\''] {
        let Some(rest) = text.strip_prefix(quote) else {
            continue;
        };
        if let Some(inner) = rest.strip_suffix(quote) {
            return spelled(inner);
        }
        if let Some((inner, after)) = rest.split_once(quote)
            && after.trim_start().starts_with('#')
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

/// The names a page bundle's own source takes under the directory the page is
/// published at, which is how a destination naming that directory reaches the
/// file that writes the page rather than the directory itself.
const BUNDLE_INDEX: [&str; 2] = ["_index.md", "index.md"];

/// The route a page is published at, which such a router serves from a
/// directory of that name: the source name without its extension, or the
/// document's own directory when the source is that directory's index. A tree
/// published in page bundles adds Hugo's `_index` to that pair, where the
/// branch's own page is the directory holding it.
fn page_route(document: &[u8], bundles: bool) -> Vec<u8> {
    let parent = directory(document);
    let name = document.rsplit(|byte| *byte == b'/').next().unwrap_or(b"");
    let stem = match name.iter().rposition(|byte| *byte == b'.') {
        Some(dot) if dot > 0 => name.get(..dot).unwrap_or_default(),
        Some(_) | None => name,
    };
    if stem == b"index" || stem == b"README" || (bundles && stem == b"_index") {
        return parent.to_vec();
    }
    join(parent, stem)
}

/// The plugin content path a document sits under, when it sits under one of
/// the paths Docusaurus reads by default. A document outside the site
/// directory is read from the directory holding that site, which is where a
/// site under `website/` finds the `../docs` it reads.
fn content_root(site: &[u8], document: &[u8]) -> Option<Vec<u8>> {
    plugin_path(site, document).or_else(|| plugin_path(directory(site), document))
}

fn plugin_path(site: &[u8], document: &[u8]) -> Option<Vec<u8>> {
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

/// A docname a leading slash anchored at the source root.
fn rooted_docname(root: Vec<u8>, absolute: &str, suffix: &str) -> Vec<(Vec<u8>, String)> {
    if absolute.is_empty() || absolute.starts_with('/') {
        return Vec::new();
    }
    vec![(root, docname_spelling(absolute, suffix))]
}

/// A docname read beside its own document, offered under the suffix its root
/// reads and again as authored, so a name the tree spells either way still
/// resolves. One already carrying a suffix was spelled by the adapter, which
/// runs before any root is known, and is left the way it was spelled.
fn docname_beside(document: &RepoPath, docname: &str, suffix: &str) -> Vec<(Vec<u8>, String)> {
    if docname.ends_with(suffix) || docname.ends_with(DEFAULT_SOURCE_SUFFIX) {
        return Vec::new();
    }
    let Some((beside, relative)) = beside_document(document, docname) else {
        return Vec::new();
    };
    vec![
        (beside.clone(), docname_spelling(&relative, suffix)),
        (beside, relative),
    ]
}

/// The directory a destination is read beside and the name with any trailing
/// slash normalized away, where the destination is a relative path rather
/// than a root, a scheme, or nothing at all.
fn beside_document(document: &RepoPath, path_part: &str) -> Option<(Vec<u8>, String)> {
    let name = path_part.strip_suffix('/').unwrap_or(path_part);
    (!name.is_empty() && !name.starts_with('/') && scheme(name).is_none())
        .then(|| (directory(document.as_bytes()).to_vec(), name.to_owned()))
}

/// A docname names a source file without its suffix, so the name takes the
/// suffix its root reads and a dot inside the name stays part of the name.
fn docname_spelling(docname: &str, suffix: &str) -> String {
    if docname.ends_with(suffix) {
        docname.to_owned()
    } else {
        format!("{docname}{suffix}")
    }
}

/// The nearest directory on the document's ancestor chain holding one of the
/// named files, which is where a generator's own configuration selects its
/// rule and where a repository's own declaration sits.
pub(crate) fn declared_root(
    snapshot: &SnapshotDiscovery,
    document: &[u8],
    declared_by: &[&str],
) -> Option<Vec<u8>> {
    ancestor_root(document, &|directory| {
        declared_by.iter().any(|name| {
            let path = join(directory, name.as_bytes());
            let bound = !SHARED_CONFIGS.contains(name)
                || RepoPath::from_bytes(path.clone())
                    .and_then(|path| snapshot.bound_configs.get(&path))
                    .is_some_and(|owner| *owner == declared_by);
            bound && regular_file(snapshot, path)
        })
    })
}

/// The nearest directory on the document's ancestor chain the test holds for,
/// the document's own directory first and the repository root last.
fn ancestor_root(document: &[u8], holds: &dyn Fn(&[u8]) -> bool) -> Option<Vec<u8>> {
    let mut end = document.len();
    loop {
        let cut = document.get(..end)?.iter().rposition(|byte| *byte == b'/');
        let directory = cut.and_then(|cut| document.get(..cut)).unwrap_or_default();
        if holds(directory) {
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

pub(crate) fn join(directory: &[u8], name: &[u8]) -> Vec<u8> {
    if directory.is_empty() {
        return name.to_vec();
    }
    [directory, b"/", name].concat()
}

/// The suffixes a page's own source file carries. A router that elides the
/// extension elides any of them, so a destination naming none is looked up
/// under each in turn.
const PAGE_SUFFIXES: [&[u8]; 3] = [b".md", b".mdx", b".markdown"];

fn output_extension(raw: &[u8]) -> Option<Vec<u8>> {
    let stem = raw.strip_suffix(b".html")?;
    (!stem.is_empty()).then(|| [stem, b".md"].concat())
}

fn extensionless(raw: &[u8], suffix: &[u8]) -> Option<Vec<u8>> {
    let last = raw.rsplit(|byte| *byte == b'/').next()?;
    let named = !last.is_empty()
        && !last.ends_with(b".html")
        && !PAGE_SUFFIXES.iter().any(|held| last.ends_with(held));
    named.then(|| [raw, suffix].concat())
}

/// Only a directory's index is answered by its README, because that is the
/// shape the harvest covered.
fn readme_index(raw: &[u8]) -> Option<Vec<u8>> {
    let cut = raw.iter().rposition(|byte| *byte == b'/')?;
    let head = raw.get(..=cut)?;
    let tail = raw.get(cut.saturating_add(1)..)?;
    (tail == b"index.md").then(|| [head, b"README.md"].concat())
}
