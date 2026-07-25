# Lock growth is fixed and admission does not scan

A record that takes a lock per row, or scans the root on every insert, degrades as it fills.
That degradation shows up exactly when a lane is busiest.

The root holds one maintenance lock, one new-record lock, one clock lock, and at most 256
lazily created row-lock shards, so lock count is bounded no matter how many rows exist.
Ordinary admission of a new row does not scan the root. New identities are admitted under the
configured record cap while work already inside the cap is allowed to finish. A prepared root
opens a fresh fenced owner session without repeating startup maintenance, and a fresh random
evaluation suffix stops an old retry from matching a later row after a safe deletion.

The layout is [`controller/src/file_ledger/store/`](https://github.com/HardMax71/amiss/tree/main/controller/src/file_ledger/store). Bounded, and measured weekly
against 1,000, 10,000, 50,000, and 100,000 retained entries with full-capacity rejection and
full-cleanup cost recorded separately, in [#118](https://github.com/HardMax71/amiss/pull/118); the run is
[`.github/workflows/bench.yml`](https://github.com/HardMax71/amiss/blob/main/.github/workflows/bench.yml).
