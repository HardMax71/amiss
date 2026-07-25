# The GitHub source accepts four events and binds them

Accepting every webhook a provider offers widens the attack surface for nothing. Most events
cannot change what a documentation check would conclude, and each one accepted is another
payload shape that has to be parsed safely.

The source accepts signed `opened`, `reopened`, and `synchronize` pull-request events:

```rust
const SUPPORTED_ACTIONS: [&str; 3] = ["opened", "reopened", "synchronize"];
```

`edited` is accepted only when the signed payload says the base branch changed, because that is
the one edit that moves what the check is about. An edited title is not a new evaluation.

Admission then binds the configured repository and target, so a correctly signed event for
another repository is refused rather than evaluated. Signature validity answers "did GitHub
send this", not "is this mine".

After admission the App client refreshes the exact repository, pull request, ref, commit, tree,
and test-merge facts from the API rather than trusting the payload's copy of them, and requires
a strict active status rule whose context is bound to that App. It refreshes again before
saving the result, because the state that decides a verdict is the state at publication, not
the state when the webhook arrived.

Completed in [#107](https://github.com/hardmax71/amiss/pull/107). The source is
[`controller/github/src/lib.rs`](https://github.com/hardmax71/amiss/blob/main/controller/github/src/lib.rs).
