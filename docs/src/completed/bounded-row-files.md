# A row is one bounded state file and, briefly, a report

Unbounded per-row storage is a way for a hostile or merely broken provider to fill a disk,
and partial writes are a way to resume into a state that never existed.

Each row is one bounded state file, plus one bounded report file only while the report is
needed. Write order carries the invariant: saving the result writes the report before the
state that names it, so a state file never points at a report that is not there, and
completion saves `done` before removing the report, so nothing removes evidence still being
claimed. Opening the root and explicit cleanup both remove dead reports and known
atomic-write leftovers.

The row format is [`controller/src/file_ledger/format/`](https://github.com/HardMax71/amiss/tree/main/controller/src/file_ledger/format). Finished in [#105](https://github.com/HardMax71/amiss/pull/105).
