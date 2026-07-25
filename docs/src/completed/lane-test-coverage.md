# The lanes are tested through, and against, themselves

A lane test that only walks the happy path proves the pieces connect. It says nothing about the
cases a gate exists for, and those cases are the product.

End-to-end and focused tests carry a signed delivery through authentication, durable admission,
provider refresh, the runner, the provider gate, completion, and replay suppression. The
negative list is the real coverage: wrong provider, repository, target, runner, policy,
reviewer, commit, and tree; changed bootstrap or merge rule; expiry and replay; missing output
and timeout; malformed or tampered input and state; capacity and restart; lost ownership; ref or
gate drift; oversized and malformed packs; `REF_DELTA`; excessive delta depth; and conflicting
provider evidence.

The limit of that coverage is worth recording, because live instances found it. Every double in
these suites answers the way the provider's documentation says it will. Gitea's test double
returned a tree name distinct from its commit name, which real Gitea never does. The Forgejo
lane test sent one signature header, which real Forgejo never does. Both suites passed while
neither provider could have worked, and no amount of adding cases to a double that agrees with
the code would have found it.

What did find it was standing up real instances, which is why
[Retained provider runs](../provider-evidence.md) exists as a separate kind of evidence rather
than as more tests. The fixtures are still the right regression net: they run in seconds, they
cover the negative cases exhaustively, and they catch a change that breaks a lane. They just
cannot tell you the provider was never like that.

Completed in [#107](https://github.com/hardmax71/amiss/pull/107). The suites live under each service, such as
[`controller/github-service/tests/lane/`](https://github.com/hardmax71/amiss/tree/main/controller/github-service/tests/lane), with shared
cases in [`controller/tests/`](https://github.com/hardmax71/amiss/tree/main/controller/tests).
