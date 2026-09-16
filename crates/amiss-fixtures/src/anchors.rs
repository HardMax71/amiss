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

/// # Errors
///
/// Any filesystem failure.
pub fn mdx_identities() -> std::io::Result<CommitChain> {
    staged_repository(&MDX_IDENTITIES)
}
