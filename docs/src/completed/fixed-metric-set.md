# Ten counters, no labels, no cardinality surprise

Metrics with provider-supplied labels let a provider decide how much memory the registry uses,
which turns a monitoring feature into a denial-of-service surface. A metric set that grows as
the code grows becomes a compatibility surface nobody agreed to maintain.

`/metrics` exposes exactly ten fixed, label-free, process-local counters under the
`amiss_controller` prefix:

```rust
pub provider_requests: Counter,
pub provider_acceptances: Counter,
pub provider_refusals: Counter,
pub provider_unavailable: Counter,
pub delivery_attempts: Counter,
pub delivery_completions: Counter,
pub delivery_retries: Counter,
pub delivery_discards: Counter,
pub maintenance_runs: Counter,
pub maintenance_removals: Counter,
```

The set cannot grow from a repository, an identity, or a result, which is the property that
matters: nothing a provider sends can add a series. Lifecycle transitions are emitted as events
instead of folded into counters, so a restart does not move a number that someone alerts on.

Process-local is also a deliberate limit rather than an oversight. These counters describe one
process since it started; they are not a durable record, and the durable record is the delivery
ledger, which is designed for that job.

Added in [#122](https://github.com/hardmax71/amiss/pull/122), in
[`controller/service/src/operations.rs`](https://github.com/hardmax71/amiss/blob/main/controller/service/src/operations.rs).
