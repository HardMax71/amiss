# Cleanup removes only what is safe to forget

Cleanup that is too eager reopens replay windows or deletes work in progress. Cleanup that is
too shy leaves the record growing forever.

Only completed rows whose authenticated replay lifetime has ended are removed. Permanent
completion markers stay, running work stays, and saved results stay. The persisted high-water
clock means a local clock that jumps backwards cannot reopen work that already expired.
Focused tests pin the inclusive end of the window, the rollback case, permanent retention,
preservation of running and saved work, fixed lock growth, full-root behaviour, recovery from
an interrupted capacity update, and cleanup's fail-closed root scan.

The rules are [`controller/src/file_ledger/transitions/`](https://github.com/HardMax71/amiss/tree/main/controller/src/file_ledger/transitions), pinned by
[`controller/tests/`](https://github.com/HardMax71/amiss/tree/main/controller/tests). Finished in [#105](https://github.com/HardMax71/amiss/pull/105).
