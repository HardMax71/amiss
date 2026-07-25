# Authenticate first, save the raw bytes, then acknowledge

Two ordering mistakes are easy to make in a webhook receiver, and both are quiet. Parse before
authenticating, and a parser meets hostile input for free. Acknowledge before storing, and a
restart in the wrong millisecond loses a delivery the provider will never send again.

The receiver authenticates before admission and saves the exact raw delivery before
acknowledging it. Raw means the bytes that were signed, not a re-serialized version of them,
because a signature covers bytes and anything else is a different document.

The inbox is ordinary files, like the delivery record it feeds, and carries the properties that
make a queue survivable: it outlives a restart, enforces both row and byte capacity, renews
ownership while the controller works, retries temporary provider failures rather than treating
them as verdicts, and removes the raw bytes only once the delivery ledger has completed. That
last ordering means the bytes outlive every state that might still need them.

The listener is bounded before any of that: a fixed body ceiling, a fixed header count and
header byte budget, and a delivery permit taken before the body is read and held through
durable admission, so the memory a hostile sender can commit is decided by configuration rather
than by the sender.

Completed in [#107](https://github.com/hardmax71/amiss/pull/107). The receiver and inbox are
[`controller/service/src/`](https://github.com/hardmax71/amiss/tree/main/controller/service/src), pinned by
[`controller/service/tests/`](https://github.com/hardmax71/amiss/tree/main/controller/service/tests).
