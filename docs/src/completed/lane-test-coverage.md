# The lanes are tested through, and against, themselves

A lane test that only walks the happy path proves the pieces connect. It says nothing about
the cases a gate exists for.

End-to-end and focused tests carry a signed delivery through authentication, durable
admission, provider refresh, the runner, the provider gate, completion, and replay
suppression. The negative cases are the point: wrong provider, repository, target, runner,
policy, reviewer, commit, and tree; changed bootstrap or merge rule; expiry and replay;
missing output and timeout; malformed or tampered input and state; capacity and restart; lost
ownership; ref or gate drift; oversized and malformed packs; `REF_DELTA`; excessive delta
depth; and conflicting provider evidence.

The lane suites are under each service, such as
[`controller/github-service/tests/lane/`](https://github.com/HardMax71/amiss/tree/main/controller/github-service/tests/lane), with shared cases in
[`controller/tests/`](https://github.com/HardMax71/amiss/tree/main/controller/tests). Completed in [#107](https://github.com/HardMax71/amiss/pull/107).
