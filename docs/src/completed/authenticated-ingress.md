# Authenticate first, save the raw bytes, then acknowledge

A receiver that acknowledges a webhook before storing it can lose the delivery to a restart,
and one that parses before authenticating hands hostile input to a parser for free.

The bounded HTTP receiver authenticates before admission and saves the exact raw delivery
before acknowledging it. The inbox is ordinary files, survives restart, enforces row and byte
capacity, renews ownership while the controller works, retries temporary provider failures,
and removes the raw bytes only once the delivery ledger has completed. No SQL, no database,
same as the record it feeds.

The receiver and inbox are [`controller/service/src/`](https://github.com/HardMax71/amiss/tree/main/controller/service/src), pinned by
[`controller/service/tests/`](https://github.com/HardMax71/amiss/tree/main/controller/service/tests). Completed in
[#107](https://github.com/HardMax71/amiss/pull/107).
