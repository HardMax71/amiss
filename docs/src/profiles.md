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
