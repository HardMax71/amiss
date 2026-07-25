# Cleanup removes only what is safe to forget

Cleanup is where a durable record usually gets quietly wrong. Too eager and it reopens a replay
window or deletes work in progress. Too shy and the root grows until admission starts refusing
real deliveries.

The rule is narrow on purpose: only completed rows whose authenticated replay lifetime has
ended are removed. Permanent completion markers stay, because a delivery with no provable issue
time has no safe end. Running work stays. Saved results stay, because a result that was frozen
before its owner expired is still the answer for that delivery.

Clock movement is the subtle case. A local clock that jumps backwards would make expired work
look live again, so the persisted high-water clock in the root metadata refuses to go
backwards and a rollback cannot reopen anything.

The pinned cases are the honest measure of the rule: the inclusive end of the window, clock
rollback, permanent retention, preservation of running and saved work, fixed lock growth,
behavior on a full root, recovery from an interrupted capacity update, and cleanup's own
fail-closed root scan. Cleanup that cannot read the root does nothing rather than assuming the
root is empty, which is the difference between a maintenance job and a data-loss incident.

Finished in [#105](https://github.com/hardmax71/amiss/pull/105) with the adversarial cleanup, capacity, and corruption tests, and
made a single recoverable batch in [#118](https://github.com/hardmax71/amiss/pull/118). The transitions are
[`controller/src/file_ledger/transitions/`](https://github.com/hardmax71/amiss/tree/main/controller/src/file_ledger/transitions).
