# Claim, lease, result, and completion are one contract

The controller before this could preserve an evaluation ID across retries and little else. It
had no way to represent two workers owning the same delivery, and no way to represent the
window between finishing an evaluation and publishing it, which is exactly where a crash costs
you either a lost verdict or a second one. A stale worker could not be rejected, because there
was nothing to reject it with, and a retry had no immutable result to resume from, so it
re-evaluated and hoped.

`DeliveryLedger` replaced that with one atomic claim whose answer is the whole coordination
contract:

```rust
pub enum DeliveryClaim {
    Execute(DeliveryLease),
    Publish(StagedPublication),
    Busy {
        evaluation_id: ControllerEvaluationId,
        retry_at_unix_millis: i64,
    },
    Duplicate {
        evaluation_id: ControllerEvaluationId,
    },
    BindingConflict,
}
```

`Execute` grants ownership. `Publish` hands back a result that a previous owner already froze,
so the retry publishes rather than recomputes. `Busy` says someone else holds it and when to
come back. `Duplicate` is reserved for terminal, durably completed work, and `BindingConflict`
for a delivery whose identity does not match what the record already holds under that key.

Ownership is fenced rather than timed. The lease carries a monotonic `fence`, and the deadline
in it is documented as advisory for a reason worth repeating:

```rust
/// Advisory deadline; only the ledger transaction decides ownership.
pub expires_at_unix_millis: i64,
```

A worker that believes its lease is live is not the authority on that. The transaction is. That
one decision removes the class of bug where two processes disagree about a clock and both
publish.

Three rules follow and are pinned by test. An owner whose lease has expired cannot save a new
result. A result saved before expiry stays publishable on retry, because the work was real and
the clock running out later does not make it wrong. A retained completion marker is repeatable
without granting new work, so a redelivery is answered from the record instead of evaluated
again.

Introduced with the controller foundation in [#98](https://github.com/hardmax71/amiss/pull/98), given this lifecycle in
[#100](https://github.com/hardmax71/amiss/pull/100), and finished in [#105](https://github.com/hardmax71/amiss/pull/105). The contract is
[`controller/src/orchestration/ledger.rs`](https://github.com/hardmax71/amiss/blob/main/controller/src/orchestration/ledger.rs) and the
operator view is [Controller delivery](../controller.md).
