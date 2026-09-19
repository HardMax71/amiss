use crate::{CommitChain, Staged, staged_repository};

/// An Antora component under `docs/`: a navigation file outside any family,
/// an `api` module with a page, a nested page, a partial and an image, and a
/// second module. Each family rule has one reference that reaches its
/// directory and one that names nothing there, and `guide/notes.adoc` sits
/// outside the component so the same xref stays beside its document.
const ANTORA_COMPONENT: [(&str, Staged<'static>); 8] = [
    (
        "docs/antora.yml",
        Staged::File(b"name: widgets\nversion: '1.0'\nnav:\n- modules/api/nav.adoc\n"),
    ),
    (
        "docs/modules/api/nav.adoc",
        Staged::File(b"* xref:index.adoc[]\n* xref:absent.adoc[]\n"),
    ),
    (
        "docs/modules/api/pages/index.adoc",
        Staged::File(
            b"= API\n\ninclude::partial$intro.adoc[]\n\ninclude::example$absent.rb[]\n\n\
              xref:install:steps.adoc[]\n\nxref:install:absent.adoc[]\n\n\
              image::diagram.png[]\n\nimage::absent.png[]\n\n\
              xref:widgets:install:steps.adoc[]\n",
        ),
    ),
    (
        "docs/modules/api/pages/sub/deep.adoc",
        Staged::File(b"= Deep\n\nxref:index.adoc[]\n"),
    ),
    (
        "docs/modules/api/partials/intro.adoc",
        Staged::File(b"Intro.\n"),
    ),
    (
        "docs/modules/api/images/diagram.png",
        Staged::File(b"png\n"),
    ),
    (
        "docs/modules/install/pages/steps.adoc",
        Staged::File(b"= Steps\n"),
    ),
    (
        "guide/notes.adoc",
        Staged::File(b"= Notes\n\nxref:index.adoc[]\n"),
    ),
];

/// One component assembled from two source roots, a second component beside
/// it, and a third whose descriptor reserves the `ext` block. The page under
/// `docs/` names a module only the other root of its own component holds, a
/// resource neither root holds, and a module that belongs to the component
/// next door. The page under `built/` names one resource its own root holds
/// and one an extension brings in.
const ANTORA_COMPONENT_ROOTS: [(&str, Staged<'static>); 9] = [
    (
        "docs/antora.yml",
        Staged::File(b"name: widgets\nversion: '1.0'\n"),
    ),
    (
        "docs/modules/api/pages/index.adoc",
        Staged::File(
            b"= API\n\nxref:plugin:build.adoc[]\n\nxref:plugin:absent.adoc[]\n\n\
              xref:extra:notes.adoc[]\n",
        ),
    ),
    (
        "plugin/antora.yml",
        Staged::File(b"name: widgets\nversion: '1.0'\n"),
    ),
    (
        "plugin/modules/plugin/pages/build.adoc",
        Staged::File(b"= Build\n"),
    ),
    (
        "other/antora.yml",
        Staged::File(b"name: gadgets\nversion: '1.0'\n"),
    ),
    (
        "other/modules/extra/pages/notes.adoc",
        Staged::File(b"= Notes\n"),
    ),
    (
        "built/antora.yml",
        Staged::File(
            b"name: parts\nversion: true\next:\n  zip_contents_collector:\n    include: []\n",
        ),
    ),
    (
        "built/modules/guide/pages/index.adoc",
        Staged::File(b"= Guide\n\nxref:guide:here.adoc[]\n\nxref:guide:absent.adoc[]\n"),
    ),
    (
        "built/modules/guide/pages/here.adoc",
        Staged::File(b"= Here\n"),
    ),
];

/// A Docusaurus site under `website/`: a nested current page and its
/// versioned twin, each linking a bare Markdown path that lives at the content
/// root, one at the site root, one nowhere, and the `@site/` alias to a static
/// asset that exists and one that does not, and a URL whose last segment spells
/// a Markdown file. The same page names three routes rather than paths: the
/// identity a sibling declares in its frontmatter, the site-absolute slug
/// another declares, and the path a document declaring neither is published
/// at, beside a route nothing publishes. The repository `README.md` writes
/// the same destinations from outside the site, where the bare path reaches
/// nothing and the alias has no site to name, and beside them the shapes a
/// docusaurus tree writes that are paths under no site at all: a webpack
/// inline request, a name that merely carries a bang, and a directory whose
/// own name opens with an at sign.
const DOCUSAURUS_SITE: [(&str, Staged<'static>); 13] = [
    (
        "website/docusaurus.config.ts",
        Staged::File(b"export default {};\n"),
    ),
    (
        "website/docs/api/themes/configuration.mdx",
        Staged::File(
            b"[static](static-assets.mdx) [absent](absent.mdx) [note](root-note.md) \
              [logo](@site/static/img/logo.png) [gone](@site/static/img/gone.png) \
              ![logo](@site/static/img/logo.png) [beside](./static-assets.mdx) \
              [remote](https://example.com/docs/README.md) [cli](cli) \
              [unclaimed](absent-id) [setup](../../guides/setup) [notes](notes)\n",
        ),
    ),
    (
        "website/docs/api/themes/command-line.mdx",
        Staged::File(b"---\nid: cli\ntitle: Command line\n---\n\n# CLI\n"),
    ),
    (
        "website/docs/api/themes/notes.mdx",
        Staged::File(b"# Notes\n"),
    ),
    (
        "website/docs/setup.mdx",
        Staged::File(b"---\nslug: /guides/setup\n---\n\n# Setup\n"),
    ),
    (
        "website/docs/static-assets.mdx",
        Staged::File(b"# Static\n"),
    ),
    ("website/root-note.md", Staged::File(b"# Note\n")),
    ("website/static/img/logo.png", Staged::File(b"png\n")),
    (
        "website/versioned_docs/version-1.0/api/themes/configuration.mdx",
        Staged::File(b"[static](static-assets.mdx)\n"),
    ),
    (
        "website/versioned_docs/version-1.0/static-assets.mdx",
        Staged::File(b"# Static 1.0\n"),
    ),
    (
        "README.md",
        Staged::File(
            b"[static](static-assets.mdx) [logo](@site/static/img/logo.png) \
              [assets](!file-loader!./asset.pdf) [named](weird!name.md) \
              [internal](@internal/notes.md)\n",
        ),
    ),
    ("weird!name.md", Staged::File(b"# Weird\n")),
    ("@internal/notes.md", Staged::File(b"# Notes\n")),
];

/// A Docusaurus partial under the site's content root, and the same file
/// where no site declares it. `_tags.mdx` opens with the character Docusaurus
/// excludes from routing, so it is served at no URL of its own: the page that
/// imports it writes the heading its fragment-only link names, and the same
/// link in the copy under `notes/` is answered by that copy alone. Each copy
/// also writes its own heading identity, a path beside it and a path nothing
/// holds. The importing page names an identity neither file publishes.
const DOCUSAURUS_PARTIAL: [(&str, Staged<'static>); 6] = [
    (
        "website/docusaurus.config.ts",
        Staged::File(b"export default {};\n"),
    ),
    (
        "website/docs/api/plugins/_tags.mdx",
        Staged::File(
            b"## Tags file {#tags-file}\n\nUse the [tags option](#tags) to name one.\n\n\
              See [this section](#tags-file), [the guide](./guide.mdx) \
              and [the note](./absent.mdx).\n",
        ),
    ),
    (
        "website/docs/api/plugins/plugin-content-docs.mdx",
        Staged::File(
            b"import Tags from './_tags.mdx';\n\n## Tags\n\n<Tags />\n\n\
              Read [the option](#tags) and [the rest](#absent).\n",
        ),
    ),
    (
        "website/docs/api/plugins/guide.mdx",
        Staged::File(b"# Guide\n"),
    ),
    (
        "notes/_tags.mdx",
        Staged::File(
            b"## Tags file {#tags-file}\n\nUse the [tags option](#tags) to name one.\n\n\
              See [this section](#tags-file), [the guide](./guide.mdx) \
              and [the note](./absent.mdx).\n",
        ),
    ),
    ("notes/guide.mdx", Staged::File(b"# Guide\n")),
];

/// A Docusaurus site under `website/` whose pages are the sibling `docs/`
/// tree its configuration reads as `../docs`, the layout the largest sites in
/// the harvest write. The nested page names a file at that content root, the
/// `@site/` alias, and a path nothing holds. A Jekyll site under `site/`
/// declares a generator whose pages this engine does not place, and
/// `notes/readme.md` is under neither declaration.
const DOCUSAURUS_SIBLING_DOCS: [(&str, Staged<'static>); 6] = [
    (
        "website/docusaurus.config.js",
        Staged::File(b"export default {};\n"),
    ),
    ("website/static/img/logo.png", Staged::File(b"png\n")),
    (
        "docs/guides/setup.md",
        Staged::File(
            b"# Setup\n\n[reference](reference.md) [absent](absent.md) \
              [logo](@site/static/img/logo.png)\n",
        ),
    ),
    ("docs/reference.md", Staged::File(b"# Reference\n")),
    ("site/_config.yml", Staged::File(b"title: Notes\n")),
    (
        "notes/readme.md",
        Staged::File(b"# Notes\n\n[gone](absent.md)\n"),
    ),
];

/// A mkdocs site under `site/`: an index page whose raw HTML writes a
/// directory URL that the tree answers with a page source and one it answers
/// with nothing, a markdown link the generator rewrites from the source
/// instead, and a nested page whose raw image climbs out of its own published
/// directory while its raw link stays inside it. That page also writes the
/// expression a build fills in, and the index links a file whose own name
/// carries a brace. A document declares an identity no mkdocs tree publishes,
/// and `notes/index.md` writes the same directory URL from outside the site,
/// where it stays a directory.
const MKDOCS_SITE: [(&str, Staged<'static>); 8] = [
    ("site/mkdocs.yml", Staged::File(b"site_name: Widgets\n")),
    (
        "site/docs/index.md",
        Staged::File(
            b"# Widgets\n\n<a href=\"getting-started/\">Start</a>\n<a href=\"absent/\">Gone</a>\n\n\
              [start](getting-started.md)\n[named](named-page)\n[brace](a{b}.md)\n",
        ),
    ),
    (
        "site/docs/getting-started.md",
        Staged::File(b"# Getting started\n"),
    ),
    (
        "site/docs/named.md",
        Staged::File(b"---\nid: named-page\n---\n\n# Named\n"),
    ),
    ("site/docs/a{b}.md", Staged::Absent(b"# Brace\n")),
    (
        "site/docs/user-guide/choosing-your-theme.md",
        Staged::File(
            b"# Themes\n\n<img src=\"../../img/light.png\">\n<img src=\"../../img/gone.png\">\n\
              <a href=\"absent/\">Gone</a>\n<a href=\"{{ page.url }}\">Self</a>\n",
        ),
    ),
    ("site/docs/img/light.png", Staged::File(b"png\n")),
    (
        "notes/index.md",
        Staged::File(b"# Notes\n\n<a href=\"getting-started/\">Start</a>\n"),
    ),
];

/// A Sphinx source tree under `docs/`: `conf.py` beside the index, a nested
/// page writing the same source-root-absolute `:doc:` targets, one target
/// that exists and one that does not, and `notes/readme.rst` outside any
/// `conf.py`, where the absolute target names the one source tree there is.
const SPHINX_SOURCE: [(&str, Staged<'static>); 5] = [
    ("docs/conf.py", Staged::File(b"project = 'widgets'\n")),
    (
        "docs/index.rst",
        Staged::File(
            b"Index\n=====\n\nSee :doc:`/testing` and :doc:`/absent` and :doc:`/deploying/index`.\n",
        ),
    ),
    ("docs/testing.rst", Staged::File(b"Testing\n=======\n")),
    (
        "docs/deploying/index.rst",
        Staged::File(b"Deploying\n=========\n\nSee :doc:`/testing` and :doc:`../testing`.\n"),
    ),
    (
        "notes/readme.rst",
        Staged::File(b"Notes\n=====\n\nSee :doc:`/testing`.\n"),
    ),
];

/// A Hugo site under `site/`: a page writing the destination its own render
/// hook rewrites, a sibling page the tree holds, an anchor into that page
/// that no heading publishes, and a page outside the site whose missing
/// destination is still a missing file.
const HUGO_SITE: [(&str, Staged<'static>); 4] = [
    (
        "site/hugo.toml",
        Staged::File(b"baseURL = 'https://example.org/'\n"),
    ),
    (
        "site/content/en/guide.md",
        Staged::File(
            b"# Guide\n\n[glossary](g)\n[install](install.md)\n[setup](install.md#absent)\n",
        ),
    ),
    (
        "site/content/en/install.md",
        Staged::File(b"# Install\n\n## Setup\n"),
    ),
    (
        "notes/readme.md",
        Staged::File(b"# Notes\n\n[gone](absent.md)\n"),
    ),
];

/// Two mdBooks on one site: the outer book at the repository root and an
/// archived one under `second/`, whose page climbs out of its own root the
/// way the URL it is served at does. One climb reaches the outer book's
/// source, one reaches a page no book answers, one climbs past the outermost
/// root, and a source destination the tree lacks stays a missing file.
const MDBOOK_SITE: [(&str, Staged<'static>); 5] = [
    ("book.toml", Staged::File(b"[book]\ntitle = 'Guide'\n")),
    (
        "src/ch01.md",
        Staged::File(b"# One\n\n[std](../std/index.html)\n[next](ch02.md)\n"),
    ),
    ("src/ch02.md", Staged::File(b"# Two\n")),
    (
        "second/book.toml",
        Staged::File(b"[book]\ntitle = 'Archive'\n"),
    ),
    (
        "second/src/ch01.md",
        Staged::File(
            b"# Old\n\n[current](../ch01.html)\n[gone](../ch09.html)\n[source](ch07.md)\n",
        ),
    ),
];

/// A Zola site under `docs/`: `config.toml` beside the `content` directory it
/// anchors, a page writing the content-root prefix to a page that exists and
/// to one that does not, and a theme page whose colocated asset is missing
/// beside it. The `config.toml` under `.cargo/` has no content directory, so
/// it anchors nothing.
const ZOLA_SITE: [(&str, Staged<'static>); 6] = [
    (
        "docs/config.toml",
        Staged::File(b"base_url = 'https://example.org'\n"),
    ),
    (
        "docs/content/documentation/overview.md",
        Staged::File(
            b"# Overview\n\n[page](@/documentation/page.md)\n[gone](@/documentation/absent.md)\n",
        ),
    ),
    (
        "docs/content/documentation/page.md",
        Staged::File(b"# Page\n"),
    ),
    (
        "docs/content/themes/persona/index.md",
        Staged::File(b"# Persona\n\n[report](pagespeed-report.svg)\n"),
    ),
    (".cargo/config.toml", Staged::File(b"[build]\njobs = 3\n")),
    (
        ".cargo/notes.md",
        Staged::File(b"# Notes\n\n[page](@/documentation/page.md)\n"),
    ),
];

/// # Errors
///
/// Any filesystem failure.
pub fn antora_component() -> std::io::Result<CommitChain> {
    staged_repository(&ANTORA_COMPONENT)
}

/// # Errors
///
/// Any filesystem failure.
pub fn antora_component_roots() -> std::io::Result<CommitChain> {
    staged_repository(&ANTORA_COMPONENT_ROOTS)
}

/// # Errors
///
/// Any filesystem failure.
pub fn docusaurus_site() -> std::io::Result<CommitChain> {
    staged_repository(&DOCUSAURUS_SITE)
}

/// # Errors
///
/// Any filesystem failure.
pub fn docusaurus_partial() -> std::io::Result<CommitChain> {
    staged_repository(&DOCUSAURUS_PARTIAL)
}

/// # Errors
///
/// Any filesystem failure.
pub fn docusaurus_sibling_docs() -> std::io::Result<CommitChain> {
    staged_repository(&DOCUSAURUS_SIBLING_DOCS)
}

/// # Errors
///
/// Any filesystem failure.
pub fn mkdocs_site() -> std::io::Result<CommitChain> {
    staged_repository(&MKDOCS_SITE)
}

/// # Errors
///
/// Any filesystem failure.
pub fn sphinx_source() -> std::io::Result<CommitChain> {
    staged_repository(&SPHINX_SOURCE)
}

/// # Errors
///
/// Any filesystem failure.
pub fn hugo_site() -> std::io::Result<CommitChain> {
    staged_repository(&HUGO_SITE)
}

/// # Errors
///
/// Any filesystem failure.
pub fn mdbook_site() -> std::io::Result<CommitChain> {
    staged_repository(&MDBOOK_SITE)
}

/// # Errors
///
/// Any filesystem failure.
pub fn zola_site() -> std::io::Result<CommitChain> {
    staged_repository(&ZOLA_SITE)
}
