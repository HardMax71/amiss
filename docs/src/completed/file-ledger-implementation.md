# The record is ordinary files, not a database

A controller that requires SQL to hold its own delivery record inherits a service to run,
back up, and patch, and inherits it in the trust boundary. `FileLedger` implements the whole
contract with ordinary files, cross-process locks, and atomic replacement.

Root metadata fixes the record cap and replay window and preserves a high-water clock. A
separate checksummed capacity frame keeps a slot count that never understates use, plus one
exact pending key, so an addition interrupted by a crash can be settled from that row alone.
Batch deletion writes one recovery marker and one final count rather than syncing bookkeeping
per row. The v0.9 root metadata migrates in place, and reopening with different limits,
missing current capacity, or unmarked damaged data fails closed rather than guessing.

The implementation is [`controller/src/file_ledger/`](https://github.com/HardMax71/amiss/tree/main/controller/src/file_ledger) and the operator view is
[The file ledger](../file-ledger.md). Implemented in
[#103](https://github.com/HardMax71/amiss/pull/103) and finished in
[#105](https://github.com/HardMax71/amiss/pull/105).
