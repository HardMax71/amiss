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
[`FindingKind::built_in_disposition`](https://github.com/HardMax71/amiss/blob/main/crates/amiss-wire/src/report.rs)
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

Two notable cases clarify what changes between profiles and what does not. A supported
explicit reference that does not resolve is the serious structural case:
`explicit-target-missing` warns under observe and fails under enforce. A file changing under
an unchanged paragraph is a warning in both profiles, because the change might be exactly
what the paragraph already says. Amiss reports that the target moved and the prose did not;
whether the prose is now wrong remains a human decision.

The kinds, with what each one means:

| kind | meaning |
| --- | --- |
| `explicit-target-missing` | a link or path points at nothing in the tree |
| `explicit-target-type-mismatch` | it points at a file where a directory was written, or the reverse |
| `invalid-reference` | the reference cannot be resolved as written, for example a path escaping the repository |
| `subject-changed` | the paragraph holding the reference changed |
| `dependency-changed-subject-unchanged` | the referenced file changed; the paragraph did not |
| `dependency-and-subject-cochanged` | both changed together, the healthy case |
| `explicit-reference-removed` | a reference that existed in the base is gone |
| `document-removed` | a whole document left the tree |
| `unlinked-document` | a scanned document from which Amiss extracted no references; inbound reachability is not computed |
| `external-out-of-scope` | a URL to somewhere Amiss does not check, counted and left alone |
| `opaque-html-region` | raw HTML the parser cannot see into, size and place reported |
| `opaque-mdx-region` | the same for [MDX](https://mdxjs.com) expressions and JSX |
| `unsupported-reference-semantics` | anchors, site routes, symbols: real checks that belong to other tools |
| `unsupported-document-format` | a document class Amiss knows it cannot parse |
| `unsupported-target-kind` | the target is a symlink or a submodule, which Amiss will not follow |
| `unsupported-version-scope` | a reference pinned to a branch or version the scan cannot vouch for |
| `unsupported-capability` | a policy asks for something this version does not do |
| `observation-correlation-ambiguous` | two matches were equally plausible, so no guess was made |
| `policy-weakened` | the candidate loosens its own policy file |
| `coverage-reduced` | the candidate shrinks what gets scanned |
| `control-plane-changed` | the candidate touches the control configuration |
| `debt-worsened`, `debt-expired`, `waiver-invalid` | the external-control cases, see [Controls and policy](controls.md) |

The control families exist so that loosening rules and presenting invalid outside authority
are themselves visible. Repository policy may raise only `explicit-target-missing`,
`explicit-target-type-mismatch`, and `invalid-reference`, as enforced by the
[policy parser and evaluator](https://github.com/HardMax71/amiss/blob/main/crates/amiss-scan/src/policy.rs).
It may never lower a disposition. There is no suppression syntax anywhere; the way to remove
a repository-policy finding is to fix what it points at.
