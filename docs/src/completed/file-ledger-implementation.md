# The record is ordinary files, not a database

A controller that needs SQL to hold its own delivery record inherits a service to run, back up,
patch, and secure, and it inherits it inside the trust boundary. For a component whose whole
job is to be harder to lie to than a CI job, that is a poor trade. `FileLedger` implements the
entire contract with ordinary files, cross-process advisory locks, and atomic replacement.

The root carries its own configuration. `.amiss-root.state` fixes the record cap and the replay
window and preserves a high-water clock, so reopening a root with different limits, with
capacity missing, or with damage that was never marked, fails closed instead of proceeding on
assumptions. Two processes cannot disagree about the shape of the record, because the record
states its shape.

Capacity lives in a separate checksummed frame, `.amiss-capacity.state`, holding a slot count
that never understates use plus one exact pending key. The asymmetry is deliberate: a count
that is too high refuses a row that would have fit, which costs a retry, while a count that is
too low overfills the root, which costs the bound. The pending key means an addition
interrupted by a crash can be settled from that one row rather than by rebuilding the count
from the directory.

Deletion is batched. One recovery marker and one final count write replace syncing the
bookkeeping for every row, so removing a thousand ended rows is one durable decision rather
than a thousand.

Reading a root that a previous version wrote is a real case rather than a hypothetical, so v0.9
metadata migrates in place.

Implemented in [#103](https://github.com/hardmax71/amiss/pull/103) and finished in [#105](https://github.com/hardmax71/amiss/pull/105), which added the adversarial
replay, cleanup, capacity, corruption, and cross-process tests. The store is
[`controller/src/file_ledger/`](https://github.com/hardmax71/amiss/tree/main/controller/src/file_ledger) and the operator view is
[The file ledger](../file-ledger.md).
