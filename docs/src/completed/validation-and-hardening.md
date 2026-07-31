# Validation and hardening

Closed July 2026. The engine was already written; this phase asked whether its claims survive
contact with repositories nobody here wrote.

## The book's contract tables are generated, not written

Before this closed, the book mixed four maturity levels on the same page: shipped scanner
behavior, the convenience Action, controller components that existed but were not wired to
anything, and research ideas. Several mechanical claims had also drifted from the constants they
described, which is the exact failure this repository sells a tool to catch.

Dispositions, resource ceilings, finding meanings, the refusal grammar, and the worked examples
each exist twice: once in the engine, once on a page. The pages therefore do not keep their own
copy. A contract test regenerates each table from the source of truth and compares:

- `documented_profiles_are_generated_from_the_policy_contract` rebuilds the disposition table in
  [Profiles and findings](../profiles.md) from the policy contract.
- `documented_limits_are_generated_from_runtime_constants` rebuilds every row of the ceiling table
  in [Limits and refusals](../limits.md) from the runtime constants.
- `documented_finding_meanings_are_generated_from_the_engine_text` and its error twin compare the
  meaning sentences on the page against the engine's own strings.
- `documented_grammar_matches_the_refusal_grammar` compares the usage block in
  [Invocation](../invocation.md) against the grammar the binary prints when it refuses.
- `documented_finding_examples_cover_the_report_schema` and
  `all_public_contract_examples_clear_their_schema_and_registered_reader` run every published
  example through the schema and the reader that ships, so an example cannot be aspirational.
- `the_llms_index_names_real_chapters_on_the_published_book` resolves every row of the agent index
  to a chapter file, so the index cannot advertise a page that was renamed away.

The generators are ordinary functions in the same file, so a failure shows the expected table next
to the one on the page rather than an assertion that something differs.

A claim that can be generated is generated. A claim that cannot links the code that implements it,
which is why so much of the book is link-dense: the link is the check.

Aligned in [#46](https://github.com/hardmax71/amiss/pull/46), which also split the factual
[Project status](../status.md) from the forward-looking [Roadmap](../roadmap.md). Published
examples became executable in [#60](https://github.com/hardmax71/amiss/pull/60), semantic vectors
were enforced in [#62](https://github.com/hardmax71/amiss/pull/62), and the markers were made to
say what they do in [#155](https://github.com/hardmax71/amiss/pull/155). All of it lives in
[`crates/amiss/tests/documentation_contracts/`](https://github.com/hardmax71/amiss/tree/main/crates/amiss/tests/documentation_contracts).

## Embedded code cannot buy unbounded parse time

The pinned MDX lexer answers one question at every candidate closing brace: can the embedded code
end here. Each ask rescans the whole accumulated region. A region that never closes therefore
costs time quadratic in its length, and a document full of unterminated regions is a cheap way to
make a scanner spend an afternoon on a file nobody will read. The corpus notes recorded the case
and left the bound to the resource ceilings, which is a polite way of saying the hole was known
and open.

The fix charges the cost where it is spent. A resource,
`aggregate-embedded-code-evaluation-bytes-per-snapshot`, joins the wire enum, both schemas, the
floor-tightening map, and the generated limits table:

```rust
aggregate_embedded_code_evaluation_bytes_per_snapshot: 536_870_912,
```

The parse hooks charge every ask against the snapshot's remaining allowance before the lexical
scan reads it. Crossing the ceiling aborts the parse and surfaces as an ordinary
`RESOURCE_LIMIT_EXCEEDED` row carrying the resource triple. It never becomes a claim about the
document, because the scanner did not finish reading the document and saying anything about it
would be a guess. Spend accumulates across documents rather than resetting per file, so the
ceiling is a snapshot budget: a thousand small hostile documents cost the same as one large one.
The 64Ki-brace hostile fixture that used to be quadratic now finishes in under 30 milliseconds.

Outside the engine, the convenience Action gained a wall-clock watchdog on the scan step, 120
seconds by default and movable through `watchdog-seconds`. It is written in plain bash rather than
`timeout` from coreutils, because coreutils is not the same program on all four runner platforms
and a watchdog that behaves differently per platform is worse than none. When it fires the engine
is terminated, the step says so, and the job fails with no report, which is the correct outcome:
no result is not the same as a pass.

Bounded in [#81](https://github.com/hardmax71/amiss/pull/81). The meter is
[`crates/amiss-md/src/accounting.rs`](https://github.com/hardmax71/amiss/blob/main/crates/amiss-md/src/accounting.rs),
the value is in
[`crates/amiss-scan/src/resources.rs`](https://github.com/hardmax71/amiss/blob/main/crates/amiss-scan/src/resources.rs),
and the watchdog input is in
[`action.yml`](https://github.com/hardmax71/amiss/blob/main/action.yml).

## Ten public repositories were scanned and the counts kept

A tool that reports missing references can only be evaluated against repositories it did not grow
up with. The first six such scans lived in a session scratchpad, one crash away from gone, which
made the adoption argument a memory rather than evidence.

They became a book page instead, [The scan ledger](../ledger.md), and then grew to ten. One row is
one scan: a public repository, a base and candidate commit pair, the observe profile, a release
build. Each row records the commit range, references extracted, missing count, advisory rows,
changed documentation lines, the historical density per hundred changed lines, and the class of any
finding a maintainer would reject. Every raw value comes from the kept machine report or from
`git diff --numstat` over the same commit pair. Derived columns are derived from those two
artifacts and never remembered.

The result splits three ways. Four repositories came back spotless. Three carried only real
breaks: one introduced in helix, twelve pre-existing in bat across four translated READMEs whose
relative links carry the wrong prefix, and one pre-existing in alacritty where the escape-sequence
docs moved into the manpage and `docs/features.md` still links the deleted page. The remaining
three mapped systematic non-adoption classes rather than defects.

The page also fixed its own column definitions and a small-denominator rule before more rows
accumulated, because a density figure over nine changed lines is noise and would otherwise be
quoted as a finding.

Every class the study named has since been answered in the engine rather than left as a caveat,
which is [Reference coverage](reference-coverage.md). The ledger carries the rescans that measured
each one.

Committed in [#82](https://github.com/hardmax71/amiss/pull/82) with six rows, grown to ten in
[#86](https://github.com/hardmax71/amiss/pull/86).

## A false missing target is a bug, not a statistic

A checker that reports references which actually resolve teaches maintainers to ignore it, and a
muted check is worse than no check because it also consumed the attention it was meant to protect.
The usual industry answer is a false-positive rate. This project does not have one.

A false `explicit-target-missing` on a supported reference is a resolver defect. It gets a pinned
test and the accepted count is zero. That distinction is what makes [The scan ledger](../ledger.md)
readable: a nonzero missing count in a row is either a real break or a named class, never a
tolerated error margin, so nobody has to guess which.

Holding that line costs a large test surface, because every supported reference shape needs a case.
[`crates/amiss-scan/tests/resolve/`](https://github.com/hardmax71/amiss/tree/main/crates/amiss-scan/tests/resolve)
runs to around 1,450 lines for that reason, covering component splitting in RFC order,
line-selection bounds as structural outcomes, LFS pointer targets, exact target digests,
directories resolved identically through a commit and through the index, paths compared as bytes
with no case folding and no normalization, and the GitHub, GitLab, and Gitea URL dialects each
resolved against the tree rather than pattern-matched.

The same rule is why a GitHub URL needs the whole trusted chain before it resolves, pinned by
`github_urls_need_the_whole_trusted_chain`: guessing that a URL belongs to this repository, and
reporting it missing when the guess is wrong, would be the same defect wearing a different hat.

## Review feedback is grouped, ordered, and bounded

Before this, consumers reshaped raw findings themselves. The command line did it one way, the
Action another. That leaked the engine's internal taxonomy into review, duplicated the
classification logic in two places that could disagree, and let harmless inventory rows crowd out
the two lines a reviewer needed to read.

The engine now owns one deterministic reviewer projection. `feedback` groups review work by the
target it concerns and classifies each item as Fix, Check, or Existing. Classification derives from
correlation, attribution, and location metadata rather than from a match over `FindingKind`, so
adding a finding kind does not mean editing the reviewer's view to keep it sensible.

The ordering and the caps are the part that matters in practice. Fixes come before Checks, so what
must change is read first. Existing findings never take a pull-request annotation, because
annotating code the author did not touch is precisely how a check earns a mute. Scan errors stay
separate from findings, since "the run did not complete" is a different statement from "this
reference is broken". The human and Action views cap at ten combined items, and only
candidate-located displayed Fixes become annotations. Nothing is dropped: every exact finding stays
in the JSON report, which is the artifact for tooling, while the reviewer view is for people.

An incomplete run reports feedback as explicitly unavailable rather than as an empty list, because
an empty list reads as "nothing to do" and that would be a lie about a run that failed.

Shipped in [#95](https://github.com/hardmax71/amiss/pull/95), which also made removed references
recorded facts, and rendered in
[`crates/amiss-scan/src/feedback.rs`](https://github.com/hardmax71/amiss/blob/main/crates/amiss-scan/src/feedback.rs).
The annotation boundary is the `annotations` input in
[`action.yml`](https://github.com/hardmax71/amiss/blob/main/action.yml); annotation flooding was
addressed in [#68](https://github.com/hardmax71/amiss/pull/68).

## The self-scan runs the event shapes it claims to support

This repository gates itself with its own action under `enforce`, which is only evidence if the
self-scan exercises the event shapes real users hit. A gate that has only ever seen an ordinary
push proves nothing about a shallow checkout, and the failure mode there is not a crash: a scan
with a truncated history silently compares against the wrong base.

Recorded runs cover push, same-repository pull request, depth-two shallow checkout, and the
staged-index path, the last of which runs `--base "$(git rev-parse HEAD^)" --index` against the
checkout's clean index on every CI run. Two corrections were needed to make those rows mean
anything. The self-scan first had to fetch full history, since a shallow clone gave it a base it
could not resolve. Then the pull-request base had to be derived from the merge commit itself rather
than from the event payload, because the payload's base is where the branch started, not what the
merge would actually compare against.

The fork path deliberately uses the same unprivileged pull-request workflow rather than a second
privileged one, so there is no second code path that only forks take and only forks can break.

Fork and merge-group runs are not retained as phase gates, and the reason is recorded rather than
hidden: as of July 2026 GitHub offers no merge queue to this public, user-owned repository. The
`merge_group` trigger and its event mapping stay in place so a repository that does have a queue is
covered. That is a claim about readiness, not about testing, and the distinction is the point.

The job is `self-scan` in
[`.github/workflows/ci.yml`](https://github.com/hardmax71/amiss/blob/main/.github/workflows/ci.yml).
History depth fixed in [#70](https://github.com/hardmax71/amiss/pull/70), the shallow and
staged-index rows recorded in [#85](https://github.com/hardmax71/amiss/pull/85), and the base
derived from the merge commit in [#88](https://github.com/hardmax71/amiss/pull/88).

## Mutation and fuzz runs are installed with recorded baselines

A suite that only ever runs green says little about whether it would catch a regression. Two
non-gating runs answer that question separately from the gates: a mutation run and a nightly
coverage-guided fuzz run.

The first mutation run recorded 2,728 mutants with 664 missed on 2026-07-18, and it paid for itself
immediately by showing the release-manifest laws untested, which 323 lines of tests then covered.
That is the intended use of a mutation run: not a score, a list of places where a lie would go
unnoticed.

Both runs are deliberately non-gating. A weekly signal converted into a merge gate becomes a flaky
merge gate, and a fuzz run that must finish before a merge is a fuzz run that stops looking hard.

The baseline has moved since, and the current reading lives with the tooling that produces it in
[Development](../development.md) rather than here: the sweep of 2026-07-28 measured 6,523 mutants
over both workspaces. The lanes were rebuilt around that scale, so a pull request now measures only
the mutants its own diff reaches.

Installed and excluded from the fixtures crate in
[#83](https://github.com/hardmax71/amiss/pull/83); the untested manifest laws the first run exposed
were covered in [#87](https://github.com/hardmax71/amiss/pull/87). The schedules are
[`.github/workflows/mutants.yml`](https://github.com/hardmax71/amiss/blob/main/.github/workflows/mutants.yml)
and
[`.github/workflows/fuzz-long.yml`](https://github.com/hardmax71/amiss/blob/main/.github/workflows/fuzz-long.yml).
