# Parser-profile corpus

`parser-profile-corpus-v1.json` is the gate the scanner spec puts in front of parser
integration. Each case carries its raw source and the exact node count and depth that
`parser-work-accounting-v1` charges for it. Nothing downstream (spans, extraction, addresses,
the evaluator) may be built against a parser that does not reproduce this manifest.

The manifest is canonical JSON with a trailing newline, and its digest is pinned in
`crates/amiss-md/tests/corpus.rs`. Regenerate with:

    AMISS_CORPUS_BLESS=1 cargo nextest run -p amiss-md -E 'test(manifest)'

That rewrites the file and prints the new digest, which then has to be pasted into the test by
hand. A golden cannot move without the move showing up in review.

## Inputs

Both inputs are upstream bytes, pinned by SHA-256 in `crates/amiss-md/src/corpus.rs`. Nothing in
this directory is touched by the formatting hooks, because a fixer that appended a newline would
silently break a pin.

| File | Source | Cases |
| --- | --- | --- |
| `inputs/commonmark-0.31.2.spec.json` | spec.commonmark.org | 652 |
| `inputs/gfm-0.29.spec.txt` | github/cmark-gfm `test/spec.txt` | 672 |

## What the grammar pin rests on

The scanner spec froze a Node oracle (`unified` + `remark-parse` + `remark-gfm`, with
`remark-mdx` for MDX) and allowed a different parser only where it reproduces that pipeline. This
implementation is Rust with no Node anywhere, so the oracle is re-pinned to the `markdown` crate,
which is the same lineage: it is wooorm's port of micromark and `mdast-util-from-markdown`, the
two halves of `remark-parse`, and it produces the same mdast.

The equivalence is not asserted on lineage alone. It is held up by upstream ground truth:

- all 652 CommonMark 0.31.2 examples reproduce byte for byte, with the extensions off;
- all 22 examples that GFM 0.29 tags with an extension and actually executes reproduce under the
  pinned `commonmark-gfm-v1` options, except the one divergence below.

What the Rust pipeline cannot prove on its own is mdast shape equality with the Node oracle for
node counts and depths, since no upstream publishes those. They are a property of the tree, and
the tree is only pinned here. This is a real gap in the evidence, and it stays open rather than
being papered over: the manifest, not the Node pipeline, is now the thing implementations
reproduce.

## Recorded divergence

GFM example 628 autolinks `ftp://foo.bar.baz`. The pinned bundle does not, and neither does
github.com: micromark's autolink-literal extension recognizes `www.`, `http://`, `https://`, and
email, and says so. The spec pins the `remark-gfm` bundle rather than `cmark-gfm`, so the bundle
wins and the 0.29 spec text is stale here. The conformance test asserts the divergence set is
exactly this one case, so a second one fails the run.

GFM's two task-list examples are marked `disabled` upstream and are not executed by cmark-gfm's
own suite either. They remain corpus inputs with node and depth goldens; only their HTML is
skipped.

## Coverage, and what is missing

Published profiles: `commonmark-gfm-v1` and `plain-zero-lexer-v1`.

Still to land before the gate is actually closed:

- the `mdx-source-v1` profile, the MDX 3.1.1 ESM, JSX, expression, and error fixtures, and the
  pinned `remark-gfm` footnote and single- and double-tilde examples;
- extraction, span, address, owner, and opaque goldens, which this manifest does not yet carry.

The manifest names the families and profiles it covers, so a partial corpus cannot be mistaken
for a complete one.
