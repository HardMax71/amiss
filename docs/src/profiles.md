# Profiles and findings

A finding is one fact the scan established: one link, one file, one document, with four
parts. The kind says what happened. The attribution says whose change it is: `introduced` by
this candidate, `pre-existing` before it, `resolved` by it, `not-applicable` when the
before-and-after framing does not apply, or `unknown` when the match-up could not be decided
without guessing. The disposition says what the run does about it: `record` (noted),
`warn` (shown), or `fail` (blocks). The location says where, down to byte offsets.

The profile picks the built-in disposition for each kind. `observe` turns the three
structural reference failures into warnings, while `enforce` makes them blocking. Several
control-integrity findings fail under both profiles, and many coverage or change observations
are records rather than warnings. The exact table below is generated from
[`FindingKind::built_in_disposition`](../../crates/amiss-wire/src/report.rs)
and checked in CI.

<!-- amiss-doc-contract:profiles:start -->
| Finding kind | Observe | Enforce |
| --- | --- | --- |
| `explicit-target-missing` | `warn` | `fail` |
| `explicit-target-type-mismatch` | `warn` | `fail` |
| `invalid-reference` | `warn` | `fail` |
| `unsupported-reference-semantics` | `record` | `record` |
| `unsupported-document-format` | `record` | `record` |
| `unsupported-target-kind` | `record` | `record` |
| `unsupported-version-scope` | `record` | `record` |
| `unsupported-capability` | `fail` | `fail` |
| `dependency-changed-subject-unchanged` | `warn` | `warn` |
| `dependency-and-subject-cochanged` | `record` | `record` |
| `subject-changed` | `record` | `record` |
| `explicit-reference-removed` | `warn` | `warn` |
| `document-removed` | `record` | `record` |
| `external-out-of-scope` | `record` | `record` |
| `opaque-mdx-region` | `record` | `record` |
| `opaque-html-region` | `record` | `record` |
| `observation-correlation-ambiguous` | `record` | `record` |
| `unlinked-document` | `record` | `record` |
| `policy-weakened` | `fail` | `fail` |
| `coverage-reduced` | `fail` | `fail` |
| `control-plane-changed` | `fail` | `fail` |
| `debt-worsened` | `fail` | `fail` |
| `debt-expired` | `fail` | `fail` |
| `waiver-invalid` | `fail` | `fail` |
<!-- amiss-doc-contract:profiles:end -->

## What each kind means

One fixed sentence per kind, generated from
[`FindingKind::meaning`](../../crates/amiss-wire/src/report.rs) and checked in CI. The
human output prints the same sentence as a `note` line under the findings it applies to,
so a CI log carries its own legend and this page is the reference, not a prerequisite.

<!-- amiss-doc-contract:finding-meanings:start -->
- `explicit-target-missing`: a reference names a repository path, or a line range inside one, that the named tree does not hold; restore the target or correct the link
- `explicit-target-type-mismatch`: the referenced path exists as a different kind than the reference promises, as when a trailing slash names a regular file; make the spelling match the target
- `invalid-reference`: the destination cannot name a repository target: it escapes the repository or carries a backslash, an encoded separator, or control bytes; fix the destination
- `unsupported-reference-semantics`: the reference uses semantics Amiss does not evaluate, a heading fragment or a leading-slash site route; the unchecked part is declared instead of guessed
- `unsupported-document-format`: a policy-included document has no parser in this engine; it is discovered and counted, and its content is never scanned
- `unsupported-target-kind`: the reference resolves to a symlink or submodule, which Amiss does not follow; the boundary is declared instead of crossed
- `unsupported-version-scope`: a forge URL names this repository at another version, a different branch, tag, or commit; only the candidate version is read, so the link is recognized and left unresolved
- `unsupported-capability`: a candidate document declares a reserved amiss: capability this engine does not implement; the run ends incomplete rather than guessing at the claim
- `dependency-changed-subject-unchanged`: the referenced content changed and the block citing it did not; a reason for a person to reread the prose, never a machine verdict that it is wrong
- `dependency-and-subject-cochanged`: the referenced content and the block citing it changed together, the shape of a maintained page; recorded with nothing to act on
- `subject-changed`: the block holding the reference changed while its target did not; recorded so prose moving over an unchanged dependency stays visible
- `explicit-reference-removed`: a reference that existed in the base is gone from the candidate; removal may be deliberate, so this warns for review instead of blocking
- `document-removed`: a scanned document left the tree; recorded so the disappearance is a stated fact rather than a silent one
- `external-out-of-scope`: the destination is an external URL Amiss never fetches; counted, reported, and left alone
- `opaque-mdx-region`: an MDX expression region the parser cannot see into; a reference inside it is a stated blind spot, reported with size and place
- `opaque-html-region`: a raw HTML region the parser cannot see into; a reference inside it is a stated blind spot, reported with size and place
- `observation-correlation-ambiguous`: an occurrence has more than one plausible counterpart across the comparison; Amiss never chooses by input order, so the match is recorded as undecided
- `unlinked-document`: a scanned document from which zero references were extracted; despite the name, it claims nothing about inbound links from other pages
- `policy-weakened`: the candidate loosens its own repository policy, dropping an include, a protected path, or a raised disposition; loosening the rules is reported under the rules being loosened
- `coverage-reduced`: a protected path is gone or not a scannable document while its protection stands; restore it or amend the protection in a reviewed change
- `control-plane-changed`: a floor-protected control path is not the identical present blob on both sides, in mode and content; the floor exists so control edits are always visible
- `debt-worsened`: the finding an accepted debt item names no longer matches the recorded fact; debt tolerates exactly the recorded state, so any drift fails
- `debt-expired`: trusted time reached a debt item's expiry while its finding persists; fix the finding or renew the debt in a reviewed change
- `waiver-invalid`: a waiver item cannot apply, expired against trusted time or issued outside the floor's authority; an invalid waiver suppresses nothing
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
| `unsupported-reference-semantics` | `docs/index.md`: `[setup](guide.md)`; `docs/guide.md` exists. | Change the link to `[setup](guide.md#setup)`; Amiss resolves the file but does not validate its heading. |
| `unsupported-document-format` | Policy includes `docs/spec.rst` as a document; the file is absent. | Add `docs/spec.rst` containing `Title` and an `=====` underline. |
| `unsupported-target-kind` | `alias` is a Git symlink; `docs/index.md` has no link to it. | Append `[alias](../alias)`; Amiss will not follow the symlink. |
| `unsupported-version-scope` | Run with forge `github`, repository `github.com/acme/widgets`, candidate ref `refs/heads/feature/x`, and default ref `refs/heads/main`; the link names `blob/feature/x/docs/guide.md`. | Keep that identity context but change the link to name `blob/main/docs/guide.md`. |
| `unsupported-capability` | `docs/claims.md`: `# Claims`. | Append `[amiss:foo]: <amiss:reference/path-exists?path=docs/a.md>`. |
| `dependency-changed-subject-unchanged` | `docs/guide.md`: `See [parser](../src/parser.rs).`<br>`src/parser.rs`: `tokenize()` | Leave `docs/guide.md` unchanged.<br>Change `src/parser.rs` to `lex()`. |
| `dependency-and-subject-cochanged` | `docs/guide.md`: `See [parser](../src/parser.rs).`<br>`src/parser.rs`: `tokenize()` | `docs/guide.md`: `See [revised parser](../src/parser.rs).`<br>`src/parser.rs`: `lex()` |
| `subject-changed` | `docs/guide.md`: `See [parser](../src/parser.rs).`<br>`src/parser.rs`: `tokenize()` | Change the paragraph to `See [revised parser](../src/parser.rs).`<br>Leave `src/parser.rs` unchanged. |
| `explicit-reference-removed` | `docs/guide.md` has separate `[parser](../src/parser.rs)` and `[lexer](../src/lexer.rs)` paragraphs. | Remove only the parser paragraph; both targets and the lexer paragraph remain. |
| `document-removed` | `docs/obsolete.md` contains `# Obsolete`. | Delete `docs/obsolete.md`. |
| `external-out-of-scope` | `guide.md`: `# Guide`. | Append `[Manual](https://example.com/manual)`. |
| `opaque-mdx-region` | `page.mdx`: `[Parser](src/parser.rs)`. | Append `<Note>{"hidden"}</Note>`. |
| `opaque-html-region` | `page.md`: `[Parser](src/parser.rs)`. | Append a separate `<div class="card">hidden</div>` block. |
| `observation-correlation-ambiguous` | `docs/guide.md`: `Old [parser](../src/parser.rs).` | Replace it with two paragraphs: `First [parser](../src/parser.rs).` and `Second [parser](../src/parser.rs).` |
| `unlinked-document` | `README.md` is absent. | Add `README.md` containing only `# Title` and prose with no references. |
| `policy-weakened` | Repository policy sets `explicit-target-missing` to `fail`. | Remove that `finding_dispositions` entry. |
| `coverage-reduced` | Repository policy protects `docs/required.md`, which contains `# Required`. | Keep the inventory obligation and delete `docs/required.md`. |
| `control-plane-changed` | A verified floor protects `.github/workflows/scan.yml`, whose content is `on: push`. | Keep the floor and change the protected file to `on: pull_request`. |
| `debt-worsened` | Verified debt accepts one occurrence of `see [gone](missing.md)`. | Keep the debt item and duplicate that occurrence, changing the finding fact. |
| `debt-expired` | Debt expires at `2026-07-10T00:00:00Z`; trusted time is `2026-07-09T00:00:00Z`. | Keep the finding and debt unchanged; trusted time advances to `2026-07-10T00:00:00Z`. |
| `waiver-invalid` | Waiver expires at `2026-08-01T00:00:00Z`; trusted time is `2026-07-12T10:00:00Z`. | Keep the finding and trusted time unchanged; set `expires_at` to `2026-07-10T00:00:00Z`. |
<!-- amiss-doc-contract:finding-examples:end -->

The control families exist so that loosening rules and presenting invalid outside authority
are themselves visible. Repository policy may raise only `explicit-target-missing`,
`explicit-target-type-mismatch`, and `invalid-reference`, as enforced by the
[policy parser and evaluator](../../crates/amiss-scan/src/policy.rs).
It may never lower a disposition. There is no suppression syntax anywhere; the way to remove
a repository-policy finding is to fix what it points at.
