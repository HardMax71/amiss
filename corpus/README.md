# Parser-profile corpus

`parser-profile-corpus-v1.json` is the gate the scanner spec puts in front of parser
integration. Each case carries its raw source, what upstream says about it, and the exact node
count and depth that `parser-work-accounting-v1` charges for it under every profile. Nothing
downstream (spans, extraction, addresses, the evaluator) may be built against a parser that does
not reproduce this manifest.

The manifest is canonical JSON with a trailing newline, and its digest is pinned in
`crates/amiss-md/tests/corpus.rs`. Regenerate with:

    AMISS_CORPUS_BLESS=1 cargo nextest run -p amiss-md -E 'test(manifest)'

That rewrites the file and prints the new digest, which then has to be pasted into the test by
hand. A golden cannot move without the move showing up in review.

## Inputs

Every input is upstream bytes, pinned by SHA-256 in `crates/amiss-md/src/corpus.rs`. Nothing in
this directory is touched by the formatting hooks, because a fixer that appended a newline would
silently break a pin.

| File | Source | Cases |
| --- | --- | --- |
| `inputs/commonmark-0.31.2.spec.json` | spec.commonmark.org | 652 |
| `inputs/gfm-0.29.spec.txt` | github/cmark-gfm `test/spec.txt` | 672 |
| `inputs/micromark-mdx-jsx-3.0.2.test.js` | micromark-extension-mdx-jsx, commit `ad0a49c` | 141 |
| `inputs/micromark-mdx-expression-3.0.1.test.js` | micromark-extension-mdx-expression, commit `2891b75` | 85 |
| `inputs/micromark-mdxjs-esm-3.0.0.test.js` | micromark-extension-mdxjs-esm, commit `7cc9131` | 31 |
| `inputs/micromark-gfm-footnote-2.1.0.test.js` | micromark-extension-gfm-footnote, commit `df527f5` | 18 |
| `inputs/micromark-gfm-strikethrough-2.1.0.test.js` | micromark-extension-gfm-strikethrough, commit `a3a75cc` | 11 |
| `inputs/gfm-footnote-fixtures/` | the same suite's documents and github.com's HTML for them | 29 |

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
  pinned `commonmark-gfm-v1` options, except the one divergence below;
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
ceilings have to bound it; that work is not in this slice.

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

Published profiles: `commonmark-gfm-v1`, `mdx-source-v1`, and `plain-zero-lexer-v1`. Every case is
charged under all three, so a grammar change anywhere moves the manifest.

Still to land before the gate is actually closed: extraction, span, address, owner, and opaque
goldens, which this manifest does not yet carry.

The manifest names the families and profiles it covers, so a partial corpus cannot be mistaken
for a complete one.
