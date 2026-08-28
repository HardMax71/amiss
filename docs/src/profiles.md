# Profiles and findings

A finding is one fact the scan established, and four of its parts carry the story. The
kind says what happened. The attribution says whose change it is: `introduced` by this
candidate, `pre-existing` before it, `resolved` by it, `not-applicable` when the
before-and-after framing does not apply, or `unknown` when the match-up could not be
decided without guessing. The disposition says what the run does about it, and it comes
twice on every row: configured is what the rules asked, effective is what happened, and
only the effective one decides the exit. `record` is noted, `warn` is shown, `fail`
blocks. The location says where, down to byte offsets. The full row carries more,
twenty-one members; [The report](report.md) holds the shape.

The profile picks the built-in disposition for each kind. Six kinds flip between the
columns: the three structural reference failures, both value-claim kinds, and projection drift
warn under `observe` and fail under `enforce`. Seven control kinds fail under both profiles, one
kind warns under both, and the remaining thirteen are records. The exact table below copies
[`FindingKind::built_in_disposition`](https://github.com/HardMax71/amiss/blob/main/crates/amiss-wire/src/report.rs),
and CI checks the two stay equal.

`enforce-introduced` is the ramp between the two. It applies the enforce column, and
after repository policy and any floor have raised what they raise, it lowers every
failing finding whose attribution is `pre-existing` to `warn`, writing a
`scanner-policy-defaults/<kind>/enforce-introduced` step into the row's trace. Configured
stays `fail`; effective becomes `warn`. The backlog stays visible and counted while
anything the comparison introduced still blocks. An attribution the engine cannot
establish keeps its enforce disposition. And an organization floor whose minimum is
`enforce` does not merely refuse the ramp: the run ends incomplete at exit 2 with a
control-binding mismatch.

<!-- amiss-doc-contract:profiles:start -->
| Finding kind | Observe | Enforce |
| --- | --- | --- |
| `explicit-target-missing` | `warn` | `fail` |
| `explicit-target-type-mismatch` | `warn` | `fail` |
| `invalid-reference` | `warn` | `fail` |
| `target-declared-untracked` | `record` | `record` |
| `unsupported-reference-semantics` | `record` | `record` |
| `unsupported-document-format` | `record` | `record` |
| `unsupported-target-kind` | `record` | `record` |
| `unsupported-version-scope` | `record` | `record` |
| `unsupported-capability` | `fail` | `fail` |
| `dependency-changed-subject-unchanged` | `warn` | `warn` |
| `dependency-and-subject-cochanged` | `record` | `record` |
| `subject-changed` | `record` | `record` |
| `explicit-reference-removed` | `record` | `record` |
| `document-removed` | `record` | `record` |
| `opaque-mdx-region` | `record` | `record` |
| `opaque-html-region` | `record` | `record` |
| `observation-correlation-ambiguous` | `record` | `record` |
| `unlinked-document` | `record` | `record` |
| `site-build-defect` | `warn` | `fail` |
| `policy-weakened` | `fail` | `fail` |
| `coverage-reduced` | `fail` | `fail` |
| `control-plane-changed` | `fail` | `fail` |
| `debt-worsened` | `fail` | `fail` |
| `debt-expired` | `fail` | `fail` |
| `waiver-invalid` | `fail` | `fail` |
| `claim-broken` | `warn` | `fail` |
| `claim-target-missing` | `warn` | `fail` |
| `projection-drift` | `warn` | `fail` |
<!-- amiss-doc-contract:profiles:end -->

## What each kind means

One fixed sentence per kind, copied from
[`FindingKind::meaning`](https://github.com/HardMax71/amiss/blob/main/crates/amiss-wire/src/report.rs)
and checked against it in CI. The machine report carries the same sentence on every
finding row, so this page is a reference, not a second source of truth.

<!-- amiss-doc-contract:finding-meanings:start -->
- `explicit-target-missing`: a reference names a repository path, a line range inside one, or a heading anchor no known renderer publishes; restore the target or correct the link
- `explicit-target-type-mismatch`: the referenced path exists as a different kind than the reference promises, as when a trailing slash names a regular file; make the spelling match the target
- `invalid-reference`: the destination cannot name a repository target: it escapes the repository or carries a backslash, an encoded separator, or control bytes; fix the destination
- `target-declared-untracked`: a reference names a path a tracked ignore file names literally, so the repository declares it does not keep that target and no tree can answer for the link; the reference is recorded and counted, never cleared
- `unsupported-reference-semantics`: the reference uses semantics this run did not evaluate: a site route, a protocol-relative destination, a query string the selected grammar does not recognize, a destination that needs a document attribute this run does not evaluate, or a fragment on a target it cannot answer for; the unchecked part is declared instead of guessed
- `unsupported-document-format`: a document this run discovered has no parser in this engine, whether a markup it does not read or a policy include; it is counted, and its content is never scanned
- `unsupported-target-kind`: the reference resolves to a symlink or submodule, which Amiss does not follow; the boundary is declared instead of crossed
- `unsupported-version-scope`: a forge URL names this repository at another named version or an exact commit whose required objects are unavailable; use the candidate ref, or make the exact commit available
- `unsupported-capability`: a candidate document declares a reserved amiss: capability this engine does not implement; the run ends incomplete rather than guessing at the claim
- `dependency-changed-subject-unchanged`: the referenced content changed and the block citing it did not; a reason for a person to reread the prose, never a machine verdict that it is wrong
- `dependency-and-subject-cochanged`: the referenced content and the block citing it changed together, the shape of a maintained page; recorded with nothing to act on
- `subject-changed`: the block holding the reference changed while its target did not; recorded so prose moving over an unchanged dependency stays visible
- `explicit-reference-removed`: a reference that existed in the base is gone from the candidate; the removal is recorded as a fact, never treated as evidence that the edit was wrong
- `document-removed`: a scanned document left the tree; recorded so the disappearance is a stated fact rather than a silent one
- `opaque-mdx-region`: an MDX expression region the parser cannot see into; a reference inside it is a stated blind spot, reported with size and place
- `opaque-html-region`: a raw HTML region the parser cannot see into; a reference inside it is a stated blind spot, reported with size and place
- `observation-correlation-ambiguous`: an occurrence has more than one plausible counterpart across the comparison; Amiss never chooses by input order, so the match is recorded as undecided
- `unlinked-document`: a scanned structured document inside a complete site build's source root is unreachable from every rendered navigation entrypoint; link the page from rendered navigation or keep non-page material outside that root
- `site-build-defect`: a complete site build reports a route with conflicting owners or a redirect whose declared terminal route or anchor is not uniquely published; repair the route table or its available routing source
- `policy-weakened`: the candidate loosens its own repository policy, dropping an include, a protected path, a projection assertion, or a raised disposition; loosening the rules is reported under the rules being loosened
- `coverage-reduced`: a protected path is gone or not a scannable document while its protection stands; restore it or amend the protection in a reviewed change
- `control-plane-changed`: a floor-protected control path is not the identical present blob on both sides, in mode and content; the floor exists so control edits are always visible
- `debt-worsened`: the finding an accepted debt item names no longer matches the recorded fact; debt tolerates exactly the recorded state, so any drift fails
- `debt-expired`: trusted time reached a debt item's expiry while its finding persists; fix the finding or renew the debt in a reviewed change
- `waiver-invalid`: a waiver item cannot apply, expired against trusted time or issued outside the floor's authority; an invalid waiver suppresses nothing
- `claim-broken`: a value claim's target line no longer says what the document claims it says; update the claim or the target so the two agree
- `claim-target-missing`: a value claim names a target line no regular file in the candidate can answer; point the claim at a tracked file and a line inside it
- `projection-drift`: a policy-owned projection cannot prove that its visible code block equals its selected repository source; restore its unique sink and source or make their projected bytes agree
<!-- amiss-doc-contract:finding-meanings:end -->

## Before and after

Only the shown state changes. Floor, debt, waiver, and trusted-time examples use the control
API described in [Controls and policy](controls.md).

<!-- amiss-doc-contract:finding-examples:start -->
| Finding kind | Before | After |
| --- | --- | --- |
| `explicit-target-missing` | `docs/index.md`: `# Index`; `docs/missing.md` is absent. | Append `[missing](missing.md)` to `docs/index.md`; the target remains absent. |
| `explicit-target-type-mismatch` | `docs/index.md`: `# Index`; `docs/guide.md` is a regular file. | Append `[guide](guide.md/)`; the trailing slash promises a directory. |
| `invalid-reference` | `docs/index.md`: `# Index`. | Append a link whose destination is `../../etc/passwd`, which escapes the repository from `docs/`. |
| `target-declared-untracked` | `docs/index.md`: `# Index`; `docs/settings.md` is absent and `docs/.gitignore` contains `/settings.md`. | Append `[settings](settings.md)` to `docs/index.md`; the target stays absent and the declaration stands. |
| `unsupported-reference-semantics` | `docs/index.md`: `[setup](guide.md)`; `docs/guide.md` exists. | Change the link to `[setup](/docs/guide.md)`; a leading slash names a site route, which no tree can answer. |
| `unsupported-document-format` | `docs/spec.rst` is absent. | Add `docs/spec.rst` containing `Title` and an `=====` underline; `.rst` is discovered and has no parser. |
| `unsupported-target-kind` | `alias` is a Git symlink; `docs/index.md` has no link to it. | Append `[alias](../alias)`; Amiss will not follow the symlink. |
| `unsupported-version-scope` | Run with forge `github`, repository `github.com/acme/widgets`, candidate ref `refs/heads/feature/x`, and default ref `refs/heads/main`; the link names `blob/feature/x/docs/guide.md`. | Keep that identity context but change the link to name `blob/main/docs/guide.md`. |
| `unsupported-capability` | `docs/claims.md`: `# Claims`. | Append `[amiss:foo]: <amiss:reference/path-exists?path=docs/a.md>`. |
| `dependency-changed-subject-unchanged` | `docs/guide.md`: `See [parser](../src/parser.rs).`<br>`src/parser.rs`: `tokenize()` | Leave `docs/guide.md` unchanged.<br>Change `src/parser.rs` to `lex()`. |
| `dependency-and-subject-cochanged` | `docs/guide.md`: `See [parser](../src/parser.rs).`<br>`src/parser.rs`: `tokenize()` | `docs/guide.md`: `See [revised parser](../src/parser.rs).`<br>`src/parser.rs`: `lex()` |
| `subject-changed` | `docs/guide.md`: `See [parser](../src/parser.rs).`<br>`src/parser.rs`: `tokenize()` | Change the paragraph to `See [revised parser](../src/parser.rs).`<br>Leave `src/parser.rs` unchanged. |
| `explicit-reference-removed` | `docs/guide.md` has separate `[parser](../src/parser.rs)` and `[lexer](../src/lexer.rs)` paragraphs. | Remove only the parser paragraph; both targets and the lexer paragraph remain. |
| `document-removed` | `docs/obsolete.md` contains `# Obsolete`. | Delete `docs/obsolete.md`. |
| `opaque-mdx-region` | `page.mdx`: `[Parser](src/parser.rs)`. | Append `<Note>{"hidden"}</Note>`. |
| `opaque-html-region` | `page.md`: `[Parser](src/parser.rs)`. | Append a separate `<div class="card">hidden</div>` block. |
| `observation-correlation-ambiguous` | `docs/guide.md`: `Old [parser](../src/parser.rs).` | Replace it with two paragraphs: `First [parser](../src/parser.rs).` and `Second [parser](../src/parser.rs).` |
| `unlinked-document` | Complete site-build evidence proves every scanned source beneath `docs/` reachable from its rendered homepage. | Add `docs/orphan.md` without adding a rendered navigation path to it. |
| `site-build-defect` | Complete site-build evidence maps `/old/` to the unique published route `/guide/`. | Change its attributed redirect rule to target absent route `/missing/`. |
| `policy-weakened` | Repository policy sets `explicit-target-missing` to `fail`. | Remove that `finding_dispositions` entry. |
| `coverage-reduced` | Repository policy protects `docs/required.md`, which contains `# Required`. | Keep the inventory obligation and delete `docs/required.md`. |
| `control-plane-changed` | A verified floor protects `.github/workflows/scan.yml`, whose content is `on: push`. | Keep the floor and change the protected file to `on: pull_request`. |
| `debt-worsened` | Verified debt accepts one occurrence of `see [gone](missing.md)`. | Keep the debt item and duplicate that occurrence, changing the finding fact. |
| `debt-expired` | Debt expires at `2026-07-10T00:00:00Z`; trusted time is `2026-07-09T00:00:00Z`. | Keep the finding and debt unchanged; trusted time advances to `2026-07-10T00:00:00Z`. |
| `waiver-invalid` | Waiver expires at `2026-08-01T00:00:00Z`; trusted time is `2026-07-12T10:00:00Z`. | Keep the finding and trusted time unchanged; set `expires_at` to `2026-07-10T00:00:00Z`. |
| `claim-broken` | `Cargo.toml` line 3 is `version = "0.16.0"` and a claim expects exactly that. | Bump line 3 to `version = "0.17.0"` and leave the claim unchanged. |
| `claim-target-missing` | A claim names `Cargo.toml` line 3, which exists. | Delete `Cargo.toml` or point the claim at line 9999. |
| `projection-drift` | A policy assertion selects `examples/request.json` lines 1–12, and the adjacent code block has the same projected bytes. | Change the selected lines without updating the visible code block. |
<!-- amiss-doc-contract:finding-examples:end -->

The control families exist so that loosening the rules and leaning on an invalid waiver
are themselves visible findings. Repository policy may raise only
`explicit-target-missing`, `explicit-target-type-mismatch`, and `invalid-reference`, as
the [policy parser and evaluator](https://github.com/HardMax71/amiss/blob/main/crates/amiss-scan/src/policy.rs)
enforces. A rule naming a lower disposition is a no-op, and dropping one the base carried
is `policy-weakened`. Repository policy has no suppression syntax at all. The only
lowerings anywhere are the ramp above, a verified debt item, and a verified waiver, each
leaving a trace step and none removing the row. The way to remove a repository-policy
finding is to fix what it points at.
