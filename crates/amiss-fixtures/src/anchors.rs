use crate::{CommitChain, Staged, staged_repository};

/// An MDX tree whose identities are written down rather than slugged from
/// heading text. `page.mdx` declares one heading identity and then two
/// expressions that declare none: one that never had a `#`, and one that has
/// one but does not end the heading. Both leave the slug standing. `element.mdx`
/// declares one from a plain JSX element and one nested inside another, beside
/// a component that declares neither its own `id` nor the one under it, and
/// then one inside a fence Docusaurus unwraps and one inside a fence that
/// stays code. `parent.mdx` renders a partial that renders another, imports a
/// component that is no document at all, and the two `cycle` documents render
/// each other.
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
              <APITable id=\"component\">\n\n<span id=\"under-component\" />\n\n</APITable>\n\n\
              ```mdx-code-block\n<details id=\"spliced\">\n<summary>Open</summary>\n```\n\n\
              ```html\n<div id=\"quoted\"></div>\n```\n",
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

/// A mkdocs site under `site/` whose pages take identities from a generator,
/// a hook and an extension: the API page is an instruction with one heading it
/// writes itself, `settings.md` is a heading a hook expands, and `tabs.md`
/// opens a content tab. `README.md` links each of those, an identity only a
/// plugin knows, and a heading an ordinary page never had. `note.md` opens an
/// admonition fence, which names no generator, and the two pages under
/// `outside/` write the same spellings where no `mkdocs.yml` governs them.
const MKDOCS_GENERATED: [(&str, Staged<'static>); 9] = [
    (
        "README.md",
        Staged::File(
            b"[a](site/docs/api.md#widgets-api)\n\n[b](site/docs/api.md#widgets.core.Widget)\n\n\
              [c](site/docs/guide.md#absent)\n\n[d](site/docs/note.md#absent)\n\n\
              [e](outside/api.md#widgets.core.Widget)\n\n\
              [f](site/docs/settings.md#config.enabled)\n\n[g](site/docs/tabs.md#tabs-latest)\n\n\
              [h](outside/tabs.md#tabs-latest)\n",
        ),
    ),
    ("site/mkdocs.yml", Staged::File(b"site_name: widgets\n")),
    (
        "site/docs/settings.md",
        Staged::File(b"# Settings\n\n#### <!-- md:setting config.enabled -->\n\nText.\n"),
    ),
    (
        "site/docs/tabs.md",
        Staged::File(b"# Tabs\n\n=== \"Latest\"\n\n    Install it.\n"),
    ),
    (
        "outside/tabs.md",
        Staged::File(b"# Tabs\n\n=== \"Latest\"\n\n    Install it.\n"),
    ),
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

/// A Hugo site under `site/` whose pages call shortcodes. `Store.md` opens
/// a block call and links the anchor that call writes, beside a heading of its
/// own; `Get.md` writes the same call in the flow of a sentence; `guide.md`
/// calls nothing. `README.md` links each of those and the same `Store.md`
/// copied to `outside/`, where no `hugo.toml` governs the spelling.
const HUGO_SHORTCODES: [(&str, Staged<'static>); 6] = [
    (
        "README.md",
        Staged::File(
            b"[a](site/content/methods/Store.md#scope)\n\n\
              [b](site/content/methods/Store.md#determinate-values)\n\n\
              [c](site/content/guide.md#absent)\n\n[d](outside/Store.md#scope)\n\n\
              [e](site/content/methods/Get.md#absent)\n",
        ),
    ),
    (
        "site/hugo.toml",
        Staged::File(b"baseURL = 'https://example.org/'\n"),
    ),
    ("site/content/methods/Store.md", Staged::File(STORE)),
    (
        "site/content/methods/Get.md",
        Staged::File(b"# Get\n\nThe {{% new-in 0.1.0 %}} method returns a value.\n"),
    ),
    ("site/content/guide.md", Staged::File(b"# Guide\n")),
    ("outside/Store.md", Staged::File(STORE)),
];

/// One page written twice, once under the site and once outside it, so the
/// only thing between the two readings is the declaration above the document.
const STORE: &[u8] = b"# Store\n\nTo scope a value, see the [scope](#scope) section.\n\n\
                       {{% include \"_common/store-scope.md\" %}}\n\n\
                       ## Determinate values\n\nText.\n";

/// A tree whose identities are written as definition-list terms, which one
/// renderer publishes beside the headings. `README.md` links a term the page
/// writes, a term it never wrote, and the same identity on a page that holds
/// no definition list at all.
const DEFINITION_TERMS: [(&str, Staged<'static>); 3] = [
    (
        "README.md",
        Staged::File(
            b"[a](docs/options.md#background-color)\n\n[b](docs/options.md#absent-term)\n\n\
              [c](docs/guide.md#background-color)\n",
        ),
    ),
    (
        "docs/options.md",
        Staged::File(b"# Options\n\nbackground color\n: The colour behind the image.\n"),
    ),
    (
        "docs/guide.md",
        Staged::File(b"# Guide\n\nbackground color is a setting.\n"),
    ),
];

/// A Sphinx project whose documents are `MyST` Markdown. `docs/index.md` writes
/// a docname that resolves, one that does not, a source-root docname, a label
/// a page declares, a label nobody declares, a Python domain role, and a link
/// into the identity the target declares. Then it writes the same two labels as
/// plain links, once as the destination and once as a bare fragment, and a
/// destination the tree holds under a name `docs/notes.md` also declares as a
/// label. `docs/widgets.md` writes the names a directive publishes, a
/// `figure-md` argument, a `:name:` inside an `eval-rst` body, a glossary term,
/// a bracketed span, and an attribute block sharing an opener's paragraph, then
/// two spellings that publish nothing: a fence with no brace tag and a
/// definition list that is no glossary. `outside/notes.md` writes a role, a
/// label link and a directive name where no `conf.py` governs any of them.
const SPHINX_MYST: [(&str, Staged<'static>); 6] = [
    (
        "docs/conf.py",
        Staged::File(b"extensions = ['myst_parser']\n"),
    ),
    (
        "docs/index.md",
        Staged::File(
            b"# Index\n\nSee {doc}`quickstart`.\n\nSee {doc}`gone`.\n\nSee {doc}`/quickstart`.\n\n\
              See {ref}`install-step`.\n\nSee {ref}`absent-step`.\n\n\
              See {py:class}`widgets.Widget`.\n\nSee [the step](quickstart.md#install-step).\n\n\
              See [the label](install-step).\n\nSee [the anchor](#install-step).\n\n\
              See [no label](absent-step).\n\nSee [the page](quickstart.md).\n\n\
              See [the note](build-note).\n\nSee [the figure](#widget-figure).\n\n\
              See [the embedded name](rst-widget).\n\nSee [the term](<#sprocket term>).\n\n\
              See [the span](span-name).\n\nSee [the block](nested-block).\n\n\
              See [the plain fence](fence-name).\n\nSee [the plain term](<#plain term>).\n\n\
              See [the ungoverned name](outside-name).\n",
        ),
    ),
    (
        "docs/quickstart.md",
        Staged::File(b"# Quickstart\n\n(install-step)=\n\n## Install\n"),
    ),
    (
        "docs/notes.md",
        Staged::File(b"# Notes\n\n(quickstart.md)=\n\n## Aliased\n"),
    ),
    (
        "docs/widgets.md",
        Staged::File(
            b"# Widgets\n\n\
              :::{note}\n:name: build-note\n\nBuild it.\n:::\n\n\
              :::{figure-md} widget-figure\nCaption only.\n:::\n\n\
              ```{eval-rst}\n.. note::\n  :name: rst-widget\n\n  Again.\n```\n\n\
              {.glossary}\nsprocket term\n: A toothed wheel.\n\n\
              A [span of text]{#span-name}.\n\n\
              :::{admonition} Aside\n{#nested-block}\nText under the opener.\n:::\n\n\
              :::note\n:name: fence-name\n:::\n\n\
              plain term\n: Not a glossary.\n",
        ),
    ),
    (
        "outside/notes.md",
        Staged::File(
            b"# Notes\n\nSee {doc}`quickstart`.\n\nSee [the label](install-step).\n\n\
              :::{note}\n:name: outside-name\n\nUngoverned.\n:::\n",
        ),
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
pub fn hugo_shortcodes() -> std::io::Result<CommitChain> {
    staged_repository(&HUGO_SHORTCODES)
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
pub fn definition_terms() -> std::io::Result<CommitChain> {
    staged_repository(&DEFINITION_TERMS)
}

/// # Errors
///
/// Any filesystem failure.
pub fn mkdocs_snippets() -> std::io::Result<CommitChain> {
    staged_repository(&MKDOCS_SNIPPETS)
}
