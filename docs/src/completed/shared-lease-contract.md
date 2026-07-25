# The record and the runner share one lease

Two components with two ideas of who owns a delivery will eventually both act on it. The
result is a duplicate verdict, or a stale one overwriting a current one.

The controller record and the runner share a single lease contract. The runner renews before
its relative lease window closes rather than after, and losing ownership stops the run instead
of letting stale work reach publication. That makes ownership loss a stop condition rather
than a race to publish.

[Controller delivery](../controller.md) states the ownership, retry, and publication rules,
and the durable side is [The record is ordinary files](file-ledger-implementation.md). Defined
in [#100](https://github.com/HardMax71/amiss/pull/100).
