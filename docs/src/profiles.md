# Profiles and findings

A finding is one fact the scan established: one link, one file, one document, with four
parts. The kind says what happened. The attribution says whose change it is: `introduced` by
this candidate, `pre-existing` before it, `resolved` by it, `not-applicable` when the
before-and-after framing does not apply, or `unknown` when the match-up could not be decided
without guessing. The disposition says what the run does about it: `record` (noted),
`warn` (shown), or `fail` (blocks). The location says where, down to byte offsets.

The profile picks the default disposition for each kind. `observe` warns on everything and
blocks on nothing; that is where a rollout starts, because it makes existing breakage
visible without turning the repository red on day one. `enforce` makes a broken reference
fail the run, and is meant to become a required check once the old breakage is cleaned up.

Two rules hold in both profiles. A link that does not resolve is always the serious case:
`explicit-target-missing` is what blocks under enforce. And a file changing under an
unchanged paragraph never blocks in any profile, because the change might be exactly what
the paragraph already says. Amiss reports that the code moved and the prose did not. Whether
that is a problem is a human call, and the tool refuses to fake it.

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
| `unlinked-document` | a document nothing links to and that links to nothing |
| `external-out-of-scope` | a URL to somewhere Amiss does not check, counted and left alone |
| `opaque-html-region` | raw HTML the parser cannot see into, size and place reported |
| `opaque-mdx-region` | the same for MDX expressions and JSX |
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

The last four families exist so that loosening the rules is itself reported under the rules
being loosened. A repository policy may raise any kind's disposition and may never lower
one. There is no suppression syntax anywhere; the way to silence a finding is to fix what it
points at.
