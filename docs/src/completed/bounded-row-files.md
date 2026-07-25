# A row is one bounded state file and, briefly, a report

Unbounded per-row storage is how a broken or hostile provider fills a disk, and a partial write
is how a restart resumes into a state that never existed. Both are ordinary failures, so the
row format assumes them.

Each row is one bounded state file, plus one bounded report file for as long as the report is
needed and no longer. Neither can grow past its ceiling, so a row's cost is known before it is
written rather than discovered afterwards.

Write order carries the invariant. Saving a result writes the report before the state that
names it, so no state file ever points at a report that is not there. Completion writes `done`
before removing the report, so nothing removes the evidence while a claim on it could still be
made. A crash between any two steps leaves a state the next open can recognize, which is the
property that matters: not "this cannot be interrupted", but "an interruption is legible".

Both opening the root and explicit cleanup remove dead reports and known atomic-write
leftovers, so the debris of an interrupted write is collected by ordinary operation rather than
by an administrator noticing.

Finished in [#105](https://github.com/hardmax71/amiss/pull/105). The format is
[`controller/src/file_ledger/format/`](https://github.com/hardmax71/amiss/tree/main/controller/src/file_ledger/format), and the states it
moves between are [Claim, lease, result, and completion](delivery-record-contract.md).
