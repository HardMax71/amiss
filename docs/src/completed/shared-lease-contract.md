# The record and the runner share one lease

Two components with two ideas of who owns a delivery will eventually both act on it, and the
result is either a duplicate verdict or a stale one overwriting a current one. The usual fix is
a longer timeout, which converts a common bug into a rare one and makes it harder to reproduce.

The controller record and the runner share a single lease contract instead. The runner renews
before its relative lease window closes rather than after, and losing ownership stops the run
rather than letting it continue to publication. Ownership loss is a stop condition, not a race
to finish first.

The lease is fenced, so "I still hold this" is a claim the record settles rather than the
holder. A worker that was paused long enough to lose its lease finds out at the next
transaction, and the work it was doing is dropped rather than published late.

Defined in [#100](https://github.com/hardmax71/amiss/pull/100). The rules are on [Controller delivery](../controller.md), the
durable side is [The record is ordinary files](file-ledger-implementation.md), and the claim
shape is [Claim, lease, result, and completion](delivery-record-contract.md).
