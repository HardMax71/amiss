# Discovery

Discovery decides which files count as documents, and it is deliberately narrow. Files with
the exact lowercase suffix `.md` or `.markdown` are `structured-markdown`; `.mdx` files are
`structured-mdx`. Six exact extensionless basenames, `README`, `CONTRIBUTING`, `CHANGELOG`,
`SECURITY`, `SUPPORT`, and `CODE_OF_CONDUCT`, are `extensionless-markdown` and use the
Markdown adapter. `.cursorrules` and `llms.txt` are `plain-advisory`: they are scanned by an
adapter that extracts no references. Every other file is a possible reference target, not a
built-in document. These rows come directly from the
[classifier](https://github.com/HardMax71/amiss/blob/main/crates/amiss-scan/src/document.rs).

Seven directory names are always skipped, wherever they appear in a path:

```text
node_modules  vendor  third_party  dist  build  .next  target
```

The list is fixed and cannot be narrowed by configuration. Skipping is visible in both
directions: skipped documents still show up in the report's counts, as excluded. This
repository relies on the rule itself: its vendored parser test corpus lives under
`corpus/third_party/` exactly so that fixture files full of deliberately broken links are
never read as prose.

Six paths through the classifier:

```text
docs/guide.md               structured-markdown   scanned
site/page.mdx               structured-mdx        scanned
README                      extensionless-markdown scanned
llms.txt                    plain-advisory        scanned, nothing extracted
vendor/lib/README.md        excluded              the vendor component is in the closed set
src/parser.rs               not a document        a reference target only
```

Markdown and MDX recognize frontmatter only at byte zero, optionally after one UTF-8 BOM.
The first complete line must be exactly `---` or `+++`; the closing line repeats it, except
that `---` also permits `...`. A recognized region is opaque to the document grammar and may
contain at most 65,536 bytes, excluding the BOM. An opener without a permitted closer, or a
closer past that bound, remains ordinary document text. The published
[frontmatter vectors](https://github.com/HardMax71/amiss/blob/main/spec/examples/frontmatter-vectors.json) execute this boundary,
including LF, CRLF, bare CR, BOM, and exact-limit cases, through the production recognizer in
the [frontmatter test](https://github.com/HardMax71/amiss/blob/main/crates/amiss-md/tests/frontmatter.rs).

A reference definition whose decoded label begins with exact lowercase `amiss:` is a
reserved governed claim. Entity and escape decoding happens before that test; case is not
folded. Every reserved definition node contributes its exact source digest, including a
losing normalized duplicate, and only the first normalized definition controls whether a
consumer becomes an ordinary reference. A governed claim on the candidate side is an
unsupported capability boundary: the run ends incomplete with exit 2. A base-only claim
does not. The [governed-definition vectors](https://github.com/HardMax71/amiss/blob/main/spec/examples/governed-definition-vectors.json)
drive extraction, source hashing, candidate-only grouping, and report construction in the
[governed test](https://github.com/HardMax71/amiss/blob/main/crates/amiss-scan/tests/governed.rs).

Every count is reported: discovered, scanned, unsupported, excluded, unlinked. Despite its
historical name, `unlinked-document` means a scanned document from which Amiss extracted
zero references. The evaluator does not construct an inbound reachability graph, so the
finding does not assert that no other page links to the document. The exact predicate is in
the [document finding evaluator](https://github.com/HardMax71/amiss/blob/main/crates/amiss-scan/src/evaluate.rs).

Paths are treated as bytes. Amiss does not fold case and does not normalize Unicode,
because Git addresses files by exact bytes, and a checker that guesses two names are
equivalent will eventually insist that two different files are the same file. A name whose
bytes are not valid UTF-8 is still a name: the entry is classified by the same suffix
rules, scanned, and reported, with its path written as a `bytes_hex` object naming the raw
bytes as lowercase hex, since JSON text cannot carry them directly. Only a name outside
the path grammar itself, one containing a backslash or a NUL byte, or a bare `.` or `..`
segment, is refused. That refusal is never quiet: the run stops as incomplete, the error
is recorded as `UNREPRESENTABLE_PATH` with the exact bytes in `path_bytes_hex`, and the
exit is 2. Dropping such an entry silently would be the worst bug this tool could have:
the report would come back green with a document missing from it, and a missing row is
the one defect no reader can notice.

Both commit-tree and staged-index discovery emit document rows strictly increasing and
unique by those raw path bytes. That ordering is load-bearing: exact document queries and
policy-inventory checks use binary search over it, and two-sided report construction
merge-joins the ordered sides. The
[`discovery` ordering test](https://github.com/HardMax71/amiss/blob/main/crates/amiss-scan/tests/discovery.rs)
pins the Git directory-boundary ordering against both snapshot modes, the report test
pins interleaved base and candidate rows, and the `amiss-scan` `pipeline` benchmark
tracks lookup and merge cost as the row count grows.
