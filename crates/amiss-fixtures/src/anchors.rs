use crate::{CommitChain, Staged, staged_repository};

/// An MDX tree whose identities are written down rather than slugged from
/// heading text. `page.mdx` declares one heading identity and then two
/// expressions that declare none: one that never had a `#`, and one that has
/// one but does not end the heading. Both leave the slug standing. `element.mdx`
/// declares one from a plain JSX element and one nested inside another, beside
/// a component that declares neither its own `id` nor the one under it.
const MDX_IDENTITIES: [(&str, Staged<'static>); 3] = [
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
];

/// # Errors
///
/// Any filesystem failure.
pub fn mdx_identities() -> std::io::Result<CommitChain> {
    staged_repository(&MDX_IDENTITIES)
}
