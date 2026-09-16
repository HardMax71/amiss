use crate::{CommitChain, Staged, staged_repository};

/// Three `AsciiDoc` documents, one per identity an internal cross reference
/// can name. `title.adoc` names a section by its own title and then names a
/// section no document holds; `inline.adoc` names an anchor written in the
/// flow of a list item and then one nothing declares; `literal.adoc` names its
/// own section and writes a second reference inside an indented paragraph,
/// where the text is literal and no reference is read at all.
const ASCIIDOC_IDENTITIES: [(&str, Staged<'static>); 3] = [
    (
        "docs/title.adoc",
        Staged::File(
            b"= Title\n\n[#entrypoints]\n== API entrypoints\n\n\
              See <<API entrypoints>> and <<Absent Section>>.\n",
        ),
    ),
    (
        "docs/inline.adoc",
        Staged::File(
            b"= Inline\n\n* [[remove-refs]]Remove the hyperlinks.\n\n\
              See <<remove-refs,how to remove them>> and <<absent-anchor>>.\n",
        ),
    ),
    (
        "docs/literal.adoc",
        Staged::File(
            b"= Literal\n\n[#figures]\n== Figures\n\nSee <<Figures>>.\n\n $ perl -W -pe 's!Figure (1)!<<fig-absent>>!g' -i out.adoc\n",
        ),
    ),
];

/// # Errors
///
/// Any filesystem failure.
pub fn asciidoc_identities() -> std::io::Result<CommitChain> {
    staged_repository(&ASCIIDOC_IDENTITIES)
}
