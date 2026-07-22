# Amiss controller

This nested Rust workspace defines the provider-facing library boundary. The root workspace stays
offline and keeps its networking dependency bans.

The implemented core is transport-neutral. A bounded ingress gate and rotating HMAC key ring
provide standalone GitHub, GitLab Standard Webhooks, and Gitea-family signature verifiers without
a provider enum. Verifier proofs bind the exact checked route, receipt time, headers, and body;
ingress requires freshness for GitLab's signed timestamp. GitHub and Gitea-family routes use
exact-body replay keys and cannot age out done records without another authenticated replay rule.
`ProviderAdapter`, `DeliveryLedger`, and `Runner` separate authentication and provider access,
durable retry coordination, and trusted execution.
[Controller delivery](../docs/src/controller.md) documents the full flow, logical record,
heartbeats, races, and retry rules. `FileLedger` implements the record with operating-system file
locks and cross-platform atomic file replacement. Replacement syncs the new bytes before switching
the current path; an interrupted write may leave a temporary file but cannot make partial bytes
current. It deliberately uses no SQL or database and does not add storage to the offline root
workspace.

No HTTP server, authenticated payload decoder, concrete provider adapter, API client, credential
store, acquisition worker, bootstrap runner, provider check publisher, publication transport, or
deployment packaging is implemented yet. A signature verifier alone does not prove current
authorization or acquire an exact repository tree. Exact publisher idempotence remains a contract
requirement, not a working guarantee without those implementations. They will use the existing
boundaries; this workspace does not claim that any provider is currently enforceable.

Run its checks without adding anything to the root workspace:

```sh
cargo nextest run --manifest-path controller/Cargo.toml --locked
cargo clippy --manifest-path controller/Cargo.toml --all-targets --locked -- -D warnings
```
