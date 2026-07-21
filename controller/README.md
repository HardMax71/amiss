# Amiss controller

This nested Rust workspace owns the provider-facing trust boundary. The root
workspace stays offline and keeps its networking dependency bans.

The implemented core is transport-neutral: a registered provider adapter
authenticates an untouched delivery, the delivery ledger claims its replay key,
the adapter refreshes authoritative change state, the runner receives an exact
repository/dialect/ref/commit/tree request, state is refreshed again, and only then is a
result published for the original candidate. A provider run ID and positive
attempt remain distinct from the controller evaluation ID. Incomplete delivery
leases resume with that same evaluation ID; only a durably completed delivery
is treated as a duplicate. Refreshes resolve the authenticated provider run and
attempt, not the change's latest head; the authenticated candidate commit is checked
again before a runner or publisher can receive it. The URL dialect, candidate, target,
and default-branch refs are part of the rechecked run identity rather than runner guesses.

The ledger contract is storage-neutral. It requires the exact authenticated
delivery binding, one stable evaluation ID across retries, time-bounded leases,
monotonic fences, and fail-closed stale renewal and completion. Before external
I/O, one atomic operation verifies the live fence and freezes the exact publication;
a retry receives that same staged value instead of another execution lease. Completion
atomically moves that staged value to the terminal duplicate state. Publisher idempotence
is scoped by the authenticated delivery and controller evaluation ID. It deliberately ships
no SQL or database backend. A future integration must satisfy those laws through a non-database
mechanism without leaking storage into the offline root workspace.

No HTTP server, provider SDK, credential store, acquisition worker, bootstrap
runner, durable ledger implementation, provider check publisher, publication
transport, or deployment packaging is implemented yet. Staging and exact
publisher idempotence are contract requirements, not working guarantees without
those implementations. These components will implement the existing traits;
this workspace does not claim that any provider is currently an enforceable
integration.

Run its checks without adding anything to the root workspace:

```sh
cargo nextest run --manifest-path controller/Cargo.toml --locked
cargo clippy --manifest-path controller/Cargo.toml --all-targets --locked -- -D warnings
```
