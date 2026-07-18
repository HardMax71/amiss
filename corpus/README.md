# Parser-profile corpus

`parser-profile-corpus.json` is the gate the scanner spec puts in front of parser
integration. Each case carries its raw source, what upstream says about it, the exact node count
and depth that `parser-work-accounting` charges for it under every profile, and, under the two
parsing profiles, the full extraction goldens: every occurrence with both destination
representations, its byte span, its node-path address, its block owner, and the document's opaque
partition. An implementation that does not reproduce this manifest may not sit under the
evaluator.

The manifest is canonical JSON with a trailing newline, and its digest is pinned in
`crates/amiss-md/tests/corpus.rs`. Regenerate with:

    AMISS_CORPUS_BLESS=1 cargo nextest run -p amiss-md -E 'test(manifest)'

That rewrites the file and prints the new digest, which then has to be pasted into the test by
hand. A golden cannot move without the move showing up in review.

## Inputs

Every input is upstream bytes, pinned by SHA-256 in `crates/amiss-md/src/corpus.rs`. Nothing in
this directory is touched by the formatting hooks, because a fixer that appended a newline would
silently break a pin.

The directory is named `third_party/` because that is what these files are, and because
`third_party` is one of the seven tree names the scanner always excludes. Amiss therefore never
reads its own fixtures as documents. That matters more than it looks: the footnote fixtures carry
broken links deliberately, a reference that goes nowhere being exactly the input a parser has to
handle. Under any other name, running Amiss on this repository under `--profile enforce` fails on
ten of those links, and the tool cannot pass its own strictest gate.

| File | Source | Cases |
| --- | --- | --- |
| `third_party/commonmark-0.31.2.spec.json` | spec.commonmark.org | 652 |
| `third_party/gfm-0.29.spec.txt` | github/cmark-gfm `test/spec.txt` | 672 |
| `third_party/micromark-mdx-jsx-3.0.2.test.js` | micromark-extension-mdx-jsx, commit `ad0a49c` | 141 |
| `third_party/micromark-mdx-expression-3.0.1.test.js` | micromark-extension-mdx-expression, commit `2891b75` | 85 |
| `third_party/micromark-mdxjs-esm-3.0.0.test.js` | micromark-extension-mdxjs-esm, commit `7cc9131` | 31 |
| `third_party/micromark-gfm-footnote-2.1.0.test.js` | micromark-extension-gfm-footnote, commit `df527f5` | 18 |
| `third_party/micromark-gfm-strikethrough-2.1.0.test.js` | micromark-extension-gfm-strikethrough, commit `a3a75cc` | 11 |
| `third_party/gfm-footnote-fixtures/` | the same suite's documents and github.com's HTML for them | 29 |

The five JavaScript suites are the grammars' own fixtures, so the harness reads each
`micromark(...)` call out of them: the first argument is the source, an enclosing `assert.throws`
means upstream rejects it, and the equality's second argument is the HTML it expects. A source
assembled by concatenation is refused rather than truncated to its first literal. Twelve calls
cannot be read as a literal, and the manifest records the count per family, so a dropped fixture
is never silent: eight test acorn token positions, two build a 999-character identifier by
concatenation, and two pass a variable. Those commits are the ones npm published
`remark-mdx@3.1.1`, `remark-gfm@4.0.1`, and their extensions from.

Footnotes and single-tilde strikethrough are the pinned bundle's additions beyond formal GFM 0.29,
which is why they carry suites of their own rather than living in the 0.29 spec text. Seven of
their fixtures configure the extension away from what this profile pins (a different footnote
label, a clobber prefix, single tilde turned off, a construct disabled). They are testing another
profile, so they stay in the corpus as inputs and only their HTML comparison is skipped.

The footnote suite also renders 29 documents against the HTML github.com itself produces for them.
That is where the interactions the spec names live: a footnote call against a link, against an
image, against a duplicate definition, against a reference definition, and nested inside every
container. The directory is pinned whole, by one digest over the canonical JSON of every file in
it, so a fixture cannot be edited, added, or dropped without the pin moving.

## What the grammar pin rests on

The scanner spec froze a Node oracle (`unified` + `remark-parse` + `remark-gfm`, with
`remark-mdx` for MDX) and allowed a different parser only where it reproduces that pipeline. This
implementation is Rust with no Node anywhere, so the oracle is re-pinned to the `markdown` crate,
which is the same lineage: it is wooorm's port of micromark and `mdast-util-from-markdown`, the
two halves of `remark-parse`, and it produces the same mdast.

The equivalence is not asserted on lineage alone. It is held up by upstream ground truth:

- all 652 CommonMark 0.31.2 examples reproduce byte for byte, with the extensions off;
- all 22 examples that GFM 0.29 tags with an extension and actually executes reproduce under the
  pinned `commonmark-gfm` options, except the one divergence below;
- of the 257 MDX fixtures, none is rejected here that the pinned grammar accepts, 166 of the 172
  that publish HTML reproduce it exactly, and every remaining difference is one of the recorded
  cases below;
- all 22 footnote and tilde fixtures under the pinned configuration reproduce their HTML, and 28
  of the 29 documents reproduce what github.com renders for them.

What the Rust pipeline cannot prove on its own is mdast shape equality with the Node oracle for
node counts and depths, since no upstream publishes those. They are a property of the tree, and
the tree is only pinned here. This is a real gap in the evidence, and it stays open rather than
being papered over: the manifest, not the Node pipeline, is now the thing implementations
reproduce.

## Embedded JavaScript is lexed, never parsed

MDX puts JavaScript inside the document, and the parser has to know where each piece of it ends.
It offers every `}` as a candidate close and asks whether the code can end there. The pinned
bundle answers with acorn. This implementation answers with a lexical scan (in
`crates/amiss-md/src/js.rs`): the code can end when no string, template, or comment is open and
every bracket has been closed.

That is enough to make the opaque intervals right, which is the property that matters. A `}`
inside a string, a comment, or a template substitution no longer cuts a region short, and an
`export {` whose brackets are still open carries across the blank line that would otherwise end
it. Both are pinned by tests over the byte intervals themselves, not over rendered output.

It is not enough to judge whether the JavaScript is valid, and it does not try. The consequence is
recorded exactly: 26 of the 257 fixtures are accepted here and rejected upstream, and every one of
them is rejected for a reason that needs a syntax tree (acorn could not parse it; an attribute
expression is not a lone spread; an ESM block holds something other than imports and exports; an
expression is empty). The conformance test asserts that, so a rejection for any other reason
fails the run. None of them moves an interval, so extraction is unaffected: the scanner reads a
document that MDX itself would refuse to compile, and the code regions stay opaque either way.

Two limits of the lexical scan, both stated rather than discovered later. A `/` is always
division, never the start of a regular expression, so a `}` inside a regular-expression literal at
bracket depth zero would close a region one character early; telling the two apart needs the token
before the slash, and guessing wrong the other way would swallow the rest of the document, which
is worse. And a statement whose brackets are balanced across a blank line (`export const a = 1 +`,
blank, `2`) ends at the blank line here and continues upstream.

Asking at every `}` is quadratic in the length of a region that stays open, which the pinned
bundle is too, since it runs acorn at each candidate. A document built to exploit that (an
unterminated string holding a million braces) would be slow here and slow upstream. The resource
ceilings bound it: every ask charges the accumulated region against
`aggregate-embedded-code-evaluation-bytes-per-snapshot` before the scan reads it, and a crossing
aborts the parse as a resource row, never as a claim about the document. The trip, its
determinism, and the one-ask overshoot bound are pinned in `crates/amiss-md/tests/mdx.rs`.

## The extraction goldens

An occurrence is one supported syntax node: an inline link or image, a full, collapsed, or
shortcut reference form, or an autolink, where all four autolink shapes (angle URI, angle email,
`www.`, protocol and email literals) share the one `markdown-autolink` construct and differ in
their tokens. Footnote references are not links, definitions consumed by nothing produce nothing,
and anything inside raw HTML or a flattened image label produces no node, so nothing is extracted
from it.

Each occurrence publishes two destination representations, because they answer different
questions. `raw_destination` is the exact source-token byte slice: angle brackets dropped, titles
and whitespace excluded, and for reference forms the token of the first winning definition in
document order, never the consuming label. `semantic_destination` is the token after the
construct's own decoding (backslash escapes and character references for link destinations,
verbatim bytes for angle URIs, `mailto:` and `http://` prefixes for email and `www.` literals),
which is exactly the value the parser publishes as the node's URL. So `[a](&amp;b)` records
`&amp;b` and `&b`, and the pair pins both the spelling and the meaning.

Spans are zero-based half-open byte offsets into the raw document; a span endpoint never splits a
CRLF pair. `node_path` is the zero-based child-index path from the post-frontmatter root to the
syntax node itself, not to its owner, and frontmatter shifts every byte offset while shifting no
path. The block owner follows the override order the spec fixes (nearest ancestor list item,
otherwise nearest table cell, otherwise nearest paragraph, otherwise the document root), so a link
in a heading is owned by the root, and raw HTML never owns anything.

The opaque partition is frontmatter first, then MDX intervals, then raw-HTML intervals on what
remains: spans sorted, contained spans discarded, overlapping or exactly adjacent spans unioned. A
JSX element's outer span covers its Markdown-looking children, so nothing inside one is extracted.
The three interval families never overlap, and a Markdown document has no MDX intervals as an MDX
document has no raw-HTML nodes.

Two locators read source bytes rather than the tree, because the tree does not carry them. The
destination token is found by walking past the label (`](`, optional whitespace, angle or bare
form under CommonMark's escape and balanced-parenthesis rules), where a destination on the next
line may resume behind a block quote's own `>` markers, which are line prefix, not destination
bytes. And an image's label end is found by a bracket scanner (images, unlike links, may hold
links in their labels), with backslash escapes and code spans stepped over. Both locators fault
loudly as `INVALID_SOURCE_SPAN` or `PARSER_ERROR` rather than guessing, which is how four corpus
documents caught their first two bugs: indented definitions and image labels holding links.

## Recorded divergences

GFM example 628 autolinks `ftp://foo.bar.baz`. The pinned bundle does not, and neither does
github.com: micromark's autolink-literal extension recognizes `www.`, `http://`, `https://`, and
email, and says so. The spec pins the `remark-gfm` bundle rather than `cmark-gfm`, so the bundle
wins and the 0.29 spec text is stale here.

Six MDX fixtures produce different HTML, and none of the six is a grammar difference. Five differ
only in which line endings survive: the suites drop a tag with a throwaway HTML extension, while a
compiler that understands MDX also slurps the line ending the tag left behind, and the content is
identical in both. The sixth indents `{}` by four spaces and expects an indented code block,
because it loads one extension at a time and never loads the one that removes indented code from
MDX; this profile is the whole bundle, so the expression is an expression.

Each divergence set is asserted by equality, so a new one fails the run rather than joining the
list.

GFM's two task-list examples are marked `disabled` upstream and are not executed by cmark-gfm's
own suite either. They remain corpus inputs with node and depth goldens; only their HTML is
skipped.

## The one document github.com renders and this does not

`footnotes-in-constructs` holds `[link[^1]](#)`, a footnote call inside a link label. The pinned
grammar makes that a link, and so does github.com. `markdown-rs` 1.0.0 does not: the brackets stay
literal and no link node is built. `[link](#)` and `[a *b* c](#)` are links, so it is the footnote
call in the label that does it.

This one matters more than a rendering difference, because the scanner reads links. A
`[see the guide[^1]](./guide.md)` in a repository would go unseen, and the reference it carries
would be missing from the report rather than wrong in it. That is under-reporting, which is the
safer direction to fail in but still a hole, and it is disclosed here rather than discovered later.
It is worth reporting upstream. The conformance test asserts the divergence set is exactly this
one document.

Comparing against github.com's HTML needs two normalizations, both stated rather than hidden. The
suite's own compensations for bugs in GitHub's renderer are applied exactly as upstream applies
them, so that what remains is a difference here rather than a difference there. And a
back-reference's `aria-label` is erased on both sides: micromark 2.1.0 writes one per reference
(`Back to reference 1`), `markdown-rs` has a single static string and cannot express that. It is a
compile option with no parse meaning, and the scanner renders no HTML.

## An upstream bug, and what the contract says to do about it

`markdown-rs` 1.0.0 fails an internal assertion on `a [open <b> close](c) </b> d.`, and on the
image form of it: a JSX tag that opens inside a link label and closes outside it. Both are
accepted by the pinned grammar, so this is a bug, not a rejection. It is worth reporting upstream.

A repository can therefore hand the scanner a document that panics its parser. The contract
already has the answer: `PARSER_PANIC` is defined as a caught panic that bypasses the parser's own
result, which means the engine catches it and reports it rather than dying. So the release profile
unwinds instead of aborting, the parse is guarded, and those two documents come back as
`PARSER_PANIC` with the run intact. A hostile document cannot take the scanner down with it.

The same table settles what a grammar rejection is: it is attributable to the source, so an
unmatched JSX tag is `DOCUMENT_INVALID`, not a parser failure.

## Coverage, and what is missing

Published profiles: `commonmark-gfm`, `mdx-source`, and `plain-zero-lexer`. Every case is
charged under all three, so a grammar change anywhere moves the manifest.

With extraction, span, address, owner, and opaque goldens in the manifest, every golden family
the spec names for this gate is present. What the corpus still cannot prove is tree-shape equality
with the frozen Node oracle (nothing upstream publishes mdast node counts), and the two recorded
parser bugs stand until markdown-rs fixes land: the `[link[^1]](#)` link this parser does not
form, and the two documents that panic it.

The manifest names the families and profiles it covers, so a partial corpus cannot be mistaken
for a complete one.
