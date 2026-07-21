# Amiss controller

This nested Rust workspace owns the provider-facing trust boundary. The root
workspace stays offline and keeps its networking dependency bans.

The implemented core is transport-neutral. `ProviderAdapter`, `DeliveryLedger`, and `Runner`
separate authentication and provider access, durable retry coordination, and trusted execution.
[Controller delivery](../docs/src/controller.md) documents the full flow, logical record,
heartbeats, races, and retry rules. The contract deliberately ships no SQL or database backend;
a future integration must satisfy it through a non-database mechanism without leaking storage
into the offline root workspace.

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
