# The GitHub source accepts four events and binds them

Accepting every webhook a provider offers widens the attack surface for no gain. Most events
cannot change what a documentation check would conclude.

The source accepts signed `opened`, `reopened`, and `synchronize` pull-request events, plus
`edited` only when the signature covers a base-branch change. Admission binds the configured
repository and target, so a signed event for some other repository is refused rather than
evaluated. The App client then refreshes the exact repository, pull request, ref, commit,
tree, and test-merge facts, and requires a strict active status rule whose context is bound to
that App. It refreshes again before saving the result, because the state that mattered is the
state at publication.

The supported set is `SUPPORTED_ACTIONS` in [`controller/github/src/lib.rs`](https://github.com/HardMax71/amiss/blob/main/controller/github/src/lib.rs).
Completed in [#107](https://github.com/HardMax71/amiss/pull/107).
