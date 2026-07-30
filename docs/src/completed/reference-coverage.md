# Reference coverage

Closed July 2026. [The scan ledger](../ledger.md) had measured what the engine could not resolve
across ten public repositories and sorted it into named classes. Naming a class is not answering it,
and a class left named is a caveat a reader has to carry. This phase answered all four, and refused
a fifth with the measurement that killed it.

Each answer had the same entry condition, the one the Markdown adapters already met: a pinned
grammar, a conformance corpus, extraction goldens, resource accounting, and honest opaque regions.
None of them entered on intuition, and the rescans are in the ledger beside the original counts.

## A heading anchor belongs to the renderer, so twelve of them are pinned

`## Setup & Config` has no identity until something renders it, and renderers disagree. github.com
publishes `setup--config`, VitePress publishes `setup-config`, and Gitea publishes nothing at all if
the heading empties out under its filter. The engine used to decline every anchor for that reason,
which was honest and also meant 123 of the ledger's missing rows were invisible.

Twelve rules are pinned, one per renderer or per configuration of one, and the resolver asks whether
any of them would publish the anchor. The union is deliberate: adding a rule can only grow what an
anchor may match, and no repository policy narrows it. A document can add to the set by declaring an
identity the way it would add a heading, in raw HTML, in an attribute block, or in the MDX comment
Docusaurus reads, which is an edit a reviewer sees rather than a setting that clears a finding.

Seven of the twelve have a runnable implementation, and against those the table reproduces all 9,049
headings harvested from the ten ledger repositories with no mismatch. The other five are transcribed
by hand from their renderer's source, and the published vectors say which is which and what each
transcription is not checked against, because a rule that quietly stops matching its renderer looks
exactly like one that still matches.

What the rules are and how far apart they sit is
[What twelve renderers call a heading](../anchor-rules.md). Published in
[#135](https://github.com/hardmax71/amiss/pull/135), pinned in
[#137](https://github.com/hardmax71/amiss/pull/137), resolved in
[#138](https://github.com/hardmax71/amiss/pull/138), then extended over four more changes to reach
raw-HTML headings, declared identities, the MDX comment, and the entity spellings a raw-HTML heading
anchors under, ending at [#156](https://github.com/hardmax71/amiss/pull/156).

## A destination is asked again under the spellings a router serves

A documentation site serves `guide` and `guide.html` for a file called `guide.md`, and a directory's
`README.md` as its `index`. An author who writes the served spelling is writing a working link, and
the engine was reporting it missing.

A relative destination the tree does not hold is asked once more under those spellings. The first
spelling that names a file resolves the reference, and the report names the file that answered while
the occurrence keeps the destination the author wrote. The safety property is what makes the union
acceptable: a spelling can only reach a file the tree already holds, so it widens what resolves and
can never invent a target. A promised directory and a same-repository forge URL are never re-spelled
at all.

The spellings were harvested from three routers rather than transcribed from documentation. Across
the ten trees they moved 247 of the 516 missing references and moved nothing else. All 241 of
starship's were preset page names across twenty-two translations; mdBook's six were its own output
extension. The union, the routers it came from, and what it costs are in
[What a documentation router serves](../route-spellings.md).

Split from the anchor class in [#141](https://github.com/hardmax71/amiss/pull/141), pinned in
[#142](https://github.com/hardmax71/amiss/pull/142), answered in
[#143](https://github.com/hardmax71/amiss/pull/143), measured in
[#146](https://github.com/hardmax71/amiss/pull/146) and
[#149](https://github.com/hardmax71/amiss/pull/149).

## A generated target is answered from the declaration the repository already publishes

The largest class the ledger measured was targets a documentation build writes and a tree never
holds: 102 of ruff's 104 missing rows, 63 of them into one `docs/settings.md`. The engine cannot run
a docs build, and a configuration file naming generated paths would be a new thing for maintainers
to write and for the engine to trust.

So it asks a declaration the repository already publishes for Git. Only the tracked `.gitignore`
files on the path's own ancestor chain can answer, and a line qualifies only when it is anchored with
a leading slash, carries no pattern or escape byte, is neither a comment nor a negation, and spells a
path with no empty, `.`, or `..` segment. The nearest file that names the path answers and travels
with the result.

The narrowness is the point. The engine never asks whether a path is ignored; it asks whether a
tracked ignore file names exactly that path, because one wildcard would let a single line answer for
an unbounded number of references. Git applies no ignore rule to a file already tracked, and neither
does this, so a path the tree holds never reaches the question.

The outcome is `target-declared-untracked`, a record under both profiles rather than a cleared
finding, so the reference stays counted and the claim travels with it. Measured on the binary from
that branch: ruff moves 94 of 104 rows out of `explicit-target-missing` and keeps every one counted,
all declared by `docs/.gitignore`, and uv moves 54 of 55 from its root `.gitignore`. Both still exit
1 under enforce, because the leftovers are real.

Read in [#171](https://github.com/hardmax71/amiss/pull/171) and asked in
[#172](https://github.com/hardmax71/amiss/pull/172). The parser is
[`crates/amiss-scan/src/declared.rs`](https://github.com/hardmax71/amiss/blob/main/crates/amiss-scan/src/declared.rs).

## AsciiDoc and reStructuredText are read by their own parsers

Both had sat as one roadmap candidate for months, and treating either as Markdown with different
punctuation would have produced confident wrong answers. Their reference vocabularies are their own:
`xref:`, `<<id>>`, `link:`, and `include::` on one side, hyperlink targets and four file-naming
directives on the other, with roles left declared rather than guessed at because they are an open
extension point.

Each is read against the grammar its renderer defines. Docutils' `make_id` and Asciidoctor's
`Section.generate_id` became the eleventh and twelfth anchor rules, so a heading anchor on either
document type resolves the same way a Markdown one does.

Two AsciiDoc behaviors needed rules of their own, and Quarkus found both. The first end-to-end run
over its 355 documents reported 1,098 missing targets and not one was real. A target still holding a
`{name}` attribute cannot be a path, because the value arrives when the site is built and this engine
reads two trees; across Quarkus that is roughly a quarter of every reference. Charging those as
unsupported semantics rather than as misses left nine, every one a `.adoc` file the tree genuinely
does not hold. Enabling anchors then surfaced the second: a document that transcludes another
publishes a partial anchor set, because `include::` splices before Asciidoctor parses and this engine
does not splice. Quarkus produced 127 of those, and an anchor absent from a transcluding document is
now undecided rather than absent.

Both are described in [Resolution](../resolution.md). AsciiDoc landed over five changes from
[#177](https://github.com/hardmax71/amiss/pull/177), which first counted the markup the engine could
not read, to [#181](https://github.com/hardmax71/amiss/pull/181), which published the identity
Asciidoctor gives a section. reStructuredText followed the same three steps in
[#183](https://github.com/hardmax71/amiss/pull/183),
[#184](https://github.com/hardmax71/amiss/pull/184), and
[#185](https://github.com/hardmax71/amiss/pull/185). The crates are
[`crates/amiss-adoc/`](https://github.com/hardmax71/amiss/tree/main/crates/amiss-adoc) and
[`crates/amiss-rst/`](https://github.com/hardmax71/amiss/tree/main/crates/amiss-rst).

## The fifth candidate was measured and refused

Inferring a reference from a bare filename in prose was the obvious next widening, and it is the one
this phase declined. Across three trees the strongest available signal is a path-shaped token inside
a code span, and 55 to 85 percent of those name nothing in the tree. Requiring a slash lowers the
rate rather than raising it.

What the non-resolving pile holds is documentation's own teaching examples. ruff's twenty-two most
frequent are `main.py`, `a.py`, `b.py`, `mypackage/__init__.py` and their kind: 564 mentions that
were never references and can never be fixed. A tool reporting them would file more than a thousand
rows against ruff to surface the ten real missing targets the explicit checker already finds, and it
would be worst exactly where documentation is densest, because the pages that teach with examples are
the pages full of filenames that do not exist.

The refusal went into [What Amiss is not](../non-goals.md) rather than staying on the roadmap, so the
next person who proposes it from intuition meets the measurement first. Dropped in
[#176](https://github.com/hardmax71/amiss/pull/176).
