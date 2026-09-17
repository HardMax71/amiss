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
/// `conf.py` so its absolute target stays a site route.
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

/// # Errors
///
/// Any filesystem failure.
pub fn antora_component() -> std::io::Result<CommitChain> {
    staged_repository(&ANTORA_COMPONENT)
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
pub fn mkdocs_site() -> std::io::Result<CommitChain> {
    staged_repository(&MKDOCS_SITE)
}

/// # Errors
///
/// Any filesystem failure.
pub fn sphinx_source() -> std::io::Result<CommitChain> {
    staged_repository(&SPHINX_SOURCE)
}
