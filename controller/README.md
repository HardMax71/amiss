# Amiss controller

This nested Rust workspace defines the provider-facing library boundary. The root workspace stays
offline and keeps its networking dependency bans.

The implemented core is transport-neutral. A bounded ingress gate and rotating HMAC key ring
provide standalone GitHub, GitLab Standard Webhooks, and Gitea-family signature verifiers without
a provider enum. Verifier proofs bind the exact checked route, receipt time, headers, and body;
ingress requires freshness for GitLab's signed timestamp and assigns each accepted delivery a
permanent or bounded replay lifetime. GitHub and Gitea-family routes use exact-body replay keys and
permanent completion markers because their signatures contain no trusted attempt time.
`ProviderAdapter`, `DeliveryLedger`, and `Runner` separate authentication and provider access,
durable retry coordination, and trusted execution.
[Controller delivery](../docs/src/controller.md) documents the full flow, logical record,
heartbeats, races, and retry rules. `FileLedger` implements the record with operating-system file
locks and cross-platform atomic file replacement. Checksummed root metadata fixes the lease
duration, record cap, and replay window and keeps a high-water clock. One maintenance lock, one
admission lock, one clock lock, and at most 256 row-lock shards bound lock growth. New identities
stop at the cap; existing work can finish. Cleanup removes dead files and only bounded completed
rows whose authenticated lifetime has ended. Permanent completion markers, running work, and saved
results remain. A row uses one state path and at most one report path; completion removes the
report. It deliberately uses no SQL or database and does not add storage to the offline root
workspace.

`run_bootstrap` is the concrete provider-neutral execution primitive. It re-verifies the exact
repository and action commit-tree roots, derives the sealed job from the `RunRequest`, checks the
bootstrap bytes against the plan's pinned digest, and prepares a private run directory under a
caller-supplied scratch directory. The controller creates and retains the report and result file
handles; replacing their path names cannot replace what it later reads. The child receives a
cleared environment and closed standard streams. Pinned ProcessKit 2.2.5 provides one
cross-platform process-tree boundary. Every terminal path hard-kills that tree and confirms the
group is empty before the retained outputs are read. Supervision enforces a wall limit of at most
120 seconds and renews ledger ownership halfway through each controller-derived relative lease
window, capped at five seconds. The report is bounded by the machine report ceiling, and a small
result record written last distinguishes completion from missing, malformed, oversized, timed-out,
signalled, or tampered execution.

No HTTP server, authenticated payload decoder, concrete provider adapter, API client, credential
store, acquisition worker, provider check publisher, publication transport, or deployment
packaging is implemented yet. A signature verifier alone does not prove current authorization or
acquire an exact repository tree, and no current path feeds authenticated provider state and
independently acquired trees into the runner. Exact publisher idempotence remains a contract
requirement, not a working guarantee without those implementations. They will use the existing
boundaries; this workspace does not claim that any provider is currently enforceable.

Run its checks without adding anything to the root workspace:

```sh
cargo nextest run --manifest-path controller/Cargo.toml --locked
cargo clippy --manifest-path controller/Cargo.toml --all-targets --locked -- -D warnings
```
