use crate::{CommitChain, Staged, staged_repository};

/// An MDX tree whose identities are written down rather than slugged from
/// heading text. `page.mdx` declares one heading identity and then two
/// expressions that declare none: one that never had a `#`, and one that has
/// one but does not end the heading. Both leave the slug standing. `element.mdx`
/// declares one from a plain JSX element and one nested inside another, beside
/// a component that declares neither its own `id` nor the one under it.
/// `parent.mdx` renders a partial that renders another, imports a component
/// that is no document at all, and the two `cycle` documents render each other.
const MDX_IDENTITIES: [(&str, Staged<'static>); 8] = [
    ("guide.mdx", Staged::File(b"# Guide\n")),
    (
        "docs/page.mdx",
        Staged::File(
            b"## Explicit {#custom-id}\n\n## Value {price}\n\n## Trailing {#late} after\n",
        ),
    ),
    (
        "docs/element.mdx",
        Staged::File(
            b"<details id=\"node-env\">\n\nInside.\n\n</details>\n\n\
              <div id=\"outer\"><span id=\"inner\" /></div>\n\n\
              <APITable id=\"component\">\n\n<span id=\"under-component\" />\n\n</APITable>\n",
        ),
    ),
    (
        "docs/parent.mdx",
        Staged::File(
            b"import Tags from './_tags.mdx';\n\
              import APITable from '@site/src/components/APITable';\n\n\
              ## Parent heading {#parent-id}\n\n<Tags />\n\n<APITable />\n",
        ),
    ),
    (
        "docs/_tags.mdx",
        Staged::File(
            b"import Deep from './_deep.mdx';\n\n\
              ## Tags file {#tags-file}\n\n\
              See [the parent](#parent-id).\n\n<Deep />\n",
        ),
    ),
    ("docs/_deep.mdx", Staged::File(b"## Deep {#deep-id}\n")),
    (
        "docs/cycle-a.mdx",
        Staged::File(b"import B from './cycle-b.mdx';\n\n## A {#a-id}\n\n<B />\n"),
    ),
    (
        "docs/cycle-b.mdx",
        Staged::File(b"import A from './cycle-a.mdx';\n\n## B {#b-id}\n\n<A />\n"),
    ),
];

/// A mkdocs site under `site/`, where the snippet line is resolved from the
/// directory holding `mkdocs.yml` rather than from beside the document that
/// writes it. One page is nothing but a snippet, one carries a section
/// coordinate this engine cannot reproduce, one names a file the tree does not
/// hold, and `outside/notes.md` writes the same line where no `mkdocs.yml`
/// governs it.
const MKDOCS_SNIPPETS: [(&str, Staged<'static>); 8] = [
    ("README.md", Staged::File(b"# Widgets\n")),
    ("site/mkdocs.yml", Staged::File(b"site_name: widgets\n")),
    (
        "site/CONTRIBUTING.md",
        Staged::File(b"## Installing\n\nText.\n"),
    ),
    ("site/docs/index.md", Staged::File(b"# Index\n")),
    (
        "site/docs/about/contributing.md",
        Staged::File(b"--8<-- \"CONTRIBUTING.md\"\n"),
    ),
    (
        "site/docs/section.md",
        Staged::File(b"--8<-- \"CONTRIBUTING.md:install\"\n"),
    ),
    ("site/docs/gone.md", Staged::File(b"--8<-- \"absent.md\"\n")),
    (
        "outside/notes.md",
        Staged::File(b"--8<-- \"CONTRIBUTING.md\"\n"),
    ),
];

/// A mkdocs site under `site/` whose API page is a generator instruction:
/// one heading the page writes itself and a body the generator fills in.
/// `README.md` links the heading, an identity only the generator knows, and a
/// heading an ordinary page never had. `note.md` opens an admonition fence,
/// which names no generator, and `outside/api.md` writes the same instruction
/// where no `mkdocs.yml` governs it.
const MKDOCS_GENERATED: [(&str, Staged<'static>); 6] = [
    (
        "README.md",
        Staged::File(
            b"[a](site/docs/api.md#widgets-api)\n\n[b](site/docs/api.md#widgets.core.Widget)\n\n\
              [c](site/docs/guide.md#absent)\n\n[d](site/docs/note.md#absent)\n\n\
              [e](outside/api.md#widgets.core.Widget)\n",
        ),
    ),
    ("site/mkdocs.yml", Staged::File(b"site_name: widgets\n")),
    (
        "site/docs/api.md",
        Staged::File(
            b"# Widgets API\n\n::: widgets.core\n    options:\n      show_root_heading: true\n",
        ),
    ),
    ("site/docs/guide.md", Staged::File(b"# Guide\n")),
    (
        "site/docs/note.md",
        Staged::File(b"# Note\n\n:::note\nRead this.\n:::\n"),
    ),
    (
        "outside/api.md",
        Staged::File(b"# Widgets API\n\n::: widgets.core\n"),
    ),
];

/// A Sphinx project whose documents are `MyST` Markdown. `docs/index.md` writes
/// a docname that resolves, one that does not, a source-root docname, a label
/// a page declares, a label nobody declares, a Python domain role, and a link
/// into the identity the target declares. `outside/notes.md` writes the same
/// role where no `conf.py` governs it.
const SPHINX_MYST: [(&str, Staged<'static>); 4] = [
    (
        "docs/conf.py",
        Staged::File(b"extensions = ['myst_parser']\n"),
    ),
    (
        "docs/index.md",
        Staged::File(
            b"# Index\n\nSee {doc}`quickstart`.\n\nSee {doc}`gone`.\n\nSee {doc}`/quickstart`.\n\n\
              See {ref}`install-step`.\n\nSee {ref}`absent-step`.\n\n\
              See {py:class}`widgets.Widget`.\n\nSee [the step](quickstart.md#install-step).\n",
        ),
    ),
    (
        "docs/quickstart.md",
        Staged::File(b"# Quickstart\n\n(install-step)=\n\n## Install\n"),
    ),
    (
        "outside/notes.md",
        Staged::File(b"# Notes\n\nSee {doc}`quickstart`.\n"),
    ),
];

/// # Errors
///
/// Any filesystem failure.
pub fn mdx_identities() -> std::io::Result<CommitChain> {
    staged_repository(&MDX_IDENTITIES)
}

/// # Errors
///
/// Any filesystem failure.
pub fn mkdocs_generated() -> std::io::Result<CommitChain> {
    staged_repository(&MKDOCS_GENERATED)
}

/// # Errors
///
/// Any filesystem failure.
pub fn sphinx_myst() -> std::io::Result<CommitChain> {
    staged_repository(&SPHINX_MYST)
}

/// # Errors
///
/// Any filesystem failure.
pub fn mkdocs_snippets() -> std::io::Result<CommitChain> {
    staged_repository(&MKDOCS_SNIPPETS)
}
