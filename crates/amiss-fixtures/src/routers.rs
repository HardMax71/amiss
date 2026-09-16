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
/// a Markdown file. The repository `README.md` writes
/// the same destinations from outside the site, where none of them reach.
const DOCUSAURUS_SITE: [(&str, Staged<'static>); 8] = [
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
              [remote](https://example.com/docs/README.md)\n",
        ),
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
        Staged::File(b"[static](static-assets.mdx) [logo](@site/static/img/logo.png)\n"),
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
pub fn sphinx_source() -> std::io::Result<CommitChain> {
    staged_repository(&SPHINX_SOURCE)
}
