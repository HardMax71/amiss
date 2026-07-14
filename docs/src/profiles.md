# Profiles and findings

A finding is one fact about one reference or one document, with a kind, a location, an
attribution, and a disposition. The kind says what happened. The attribution says whose change
it is: `introduced` by the candidate, `pre-existing` before it, `resolved` by it,
`not-applicable` where the base and candidate story does not apply, and `unknown` where
correlation could not decide without guessing. The disposition says what the run does about
it: `record`, `warn`, or `fail`.

The profile picks the defaults table. `observe` warns on everything and is where a rollout
starts: the report is complete, nothing blocks, and the existing breakage becomes visible
without turning the repository red. `enforce` turns an unresolved reference into a failure and
is meant for a required check after that breakage is cleaned up.

Two laws hold in both profiles. A reference that does not resolve is a structural failure, so
`explicit-target-missing` is the finding that blocks under enforce. And a file changing under
a document never rises above a warning, whatever the profile:
`dependency-changed-subject-unchanged` is a signal, not a verdict, because the change may be
exactly what the prose already says. Amiss reports that the code moved and the prose did not,
and leaves the judgment where it belongs.

The kinds fall into families. Resolution failures: `explicit-target-missing`,
`explicit-target-type-mismatch`, `invalid-reference`. Boundaries stated instead of guessed:
`unsupported-reference-semantics`, `unsupported-document-format`, `unsupported-target-kind`,
`unsupported-version-scope`, `unsupported-capability`, `external-out-of-scope`,
`opaque-mdx-region`, `opaque-html-region`. Impact between base and candidate:
`subject-changed`, `dependency-changed-subject-unchanged`, `dependency-and-subject-cochanged`,
`explicit-reference-removed`, `document-removed`. Correlation:
`observation-correlation-ambiguous`. Surface accounting: `unlinked-document`. And the
control-plane family (`policy-weakened`, `coverage-reduced`, `control-plane-changed`,
`debt-worsened`, `debt-expired`, `waiver-invalid`), which exists so that a candidate cannot
quietly loosen the rules it is being judged under; see
[Controls and policy](controls.md).

A repository policy may raise a kind's disposition and may never lower it. There is no
suppression syntax anywhere: the way to stop a finding is to fix the thing it points at.
