# Embedded code cannot buy unbounded parse time

The pinned MDX lexer answers one question at every candidate closing brace: can the embedded
code end here. Each ask rescans the whole accumulated region. A region that never closes
therefore costs time quadratic in its length, and a document full of unterminated regions is a
cheap way to make a scanner spend an afternoon on a file nobody will read. The corpus notes
recorded the case and left the bound to the resource ceilings, which is a polite way of saying
the hole was known and open.

The fix charges the cost where it is spent. A resource,
`aggregate-embedded-code-evaluation-bytes-per-snapshot`, joins the wire enum, both schemas, the
floor-tightening map, and the generated limits table, with a contract value of 536,870,912
bytes:

```rust
aggregate_embedded_code_evaluation_bytes_per_snapshot: 536_870_912,
```

The parse hooks charge every ask against the snapshot's remaining allowance before the lexical
scan reads it. Crossing the ceiling aborts the parse and surfaces as an ordinary
`RESOURCE_LIMIT_EXCEEDED` row carrying the resource triple. It never becomes a claim about the
document, because the scanner did not finish reading the document and saying anything about it
would be a guess. Spend accumulates across documents rather than resetting per file, so the
ceiling is a snapshot budget: a thousand small hostile documents cost the same as one large
one.

Four tests hold it: `an_exhausted_embedded_code_allowance_is_the_aggregate_crossing` for the
trip, the one-ask overshoot bound, `spent_embedded_code_bytes_are_deterministic_and_sufficient`
for the meter, and `embedded_code_spending_accumulates_across_documents` for the budget being a
snapshot rather than a grant. The 64Ki-brace hostile fixture that used to be quadratic now
finishes in under 30 milliseconds.

Outside the engine, the convenience Action gained a wall-clock watchdog on the scan step,
120 seconds by default and movable through `watchdog-seconds`, positive integers only. It is
written in plain bash rather than `timeout` from coreutils, because coreutils is not the same
program on all four runner platforms and a watchdog that behaves differently per platform is
worse than none. When it fires the engine is terminated, the step says so, and the job fails
with no report, which is the correct outcome: no result is not the same as a pass.

Bounded in [#81](https://github.com/hardmax71/amiss/pull/81). The meter is
[`crates/amiss-md/src/accounting.rs`](https://github.com/hardmax71/amiss/blob/main/crates/amiss-md/src/accounting.rs), the value is in
[`crates/amiss-scan/src/resources.rs`](https://github.com/hardmax71/amiss/blob/main/crates/amiss-scan/src/resources.rs), the crossing is
mapped in [`crates/amiss-scan/src/scan.rs`](https://github.com/hardmax71/amiss/blob/main/crates/amiss-scan/src/scan.rs), and the
watchdog input is in [`action.yml`](https://github.com/hardmax71/amiss/blob/main/action.yml). The parser case it bounds is recorded in
the [corpus notes](https://github.com/hardmax71/amiss/blob/main/corpus/README.md).
