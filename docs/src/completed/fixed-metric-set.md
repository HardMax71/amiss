# Ten counters, no labels, no cardinality surprise

Metrics with provider-supplied labels let a provider decide how much memory the metrics
registry uses, and a metric set that grows with the code becomes a compatibility surface
nobody agreed to.

`/metrics` exposes exactly ten fixed, label-free, process-local counters under the
`amiss_controller` prefix: provider requests, acceptances, refusals, and unavailable results;
delivery attempts, completions, retries, and discards; and maintenance runs and removals.
Lifecycle transitions are emitted as events rather than folded into the counters, so a
transition never inflates a number that operators alert on.

The set is [`controller/service/src/operations.rs`](https://github.com/HardMax71/amiss/blob/main/controller/service/src/operations.rs). Added in [#122](https://github.com/HardMax71/amiss/pull/122).
