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

No HTTP server, provider SDK, database, credential store, acquisition worker,
or deployment packaging is implemented yet. Those components will implement
the existing traits; this crate does not claim that any provider is currently
an enforceable integration.

Run its checks without adding anything to the root workspace:

```sh
cargo nextest run --manifest-path controller/Cargo.toml --locked
cargo clippy --manifest-path controller/Cargo.toml --all-targets --locked -- -D warnings
```
