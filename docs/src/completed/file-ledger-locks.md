# Lock growth is fixed and admission does not scan

A record that takes a lock per row, or counts the directory on every insert, gets slower
exactly as it gets busier. On v0.9.0 that was measurable: admitting one new row into a root
holding 100,000 retained entries took about 57.5 milliseconds, because admission counted what
was already there.

The lock set is now fixed and small. One maintenance lock, one admission lock, one clock lock,
and at most 256 lazily created row-lock shards:

```text
.amiss-root.state        .amiss-maintenance.lock
.amiss-capacity.state    .amiss-admission.lock
.amiss-clock.lock        .amiss-row-7a.lock
```

Row locks are named by one hex byte of the row key, so the count is bounded by the shard space
rather than by the number of rows, and a busy root creates the same 256 files a quiet one does.

Admission stops scanning. The checksummed capacity frame answers "is there room" without
reading the directory, and the same measurement fell from about 57.5 milliseconds to about
0.25 milliseconds, with a full-capacity rejection at about 0.085 milliseconds. New identities
are admitted under the configured cap while work already inside the cap is allowed to finish,
so filling up refuses new arrivals rather than stalling what is already running.

`FileLedgerRoot` moved preparation and cleanup out of the request path. A service prepares and
cleans the root once, then creates independent fenced owner sessions without repeating startup
maintenance per request. Each new row also takes a fresh random evaluation suffix, so an old
retry cannot match a later row that happens to reuse a key after a safe deletion.

The measurement is not a merge gate, because a machine-specific timing threshold is a flaky
test wearing a stopwatch. A weekly release-mode run records admission, full rejection, and
cleanup separately at 1,000, 10,000, 50,000, and 100,000 retained entries, and the numbers are
kept as evidence rather than asserted.

Bounded in [#118](https://github.com/hardmax71/amiss/pull/118). The store is
[`controller/src/file_ledger/store/`](https://github.com/hardmax71/amiss/tree/main/controller/src/file_ledger/store) and the run is
[`.github/workflows/bench.yml`](https://github.com/hardmax71/amiss/blob/main/.github/workflows/bench.yml).
