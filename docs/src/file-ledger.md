# The file ledger

`FileLedgerRoot` prepares the ordinary-file store, and each `FileLedger` is one independently
fenced owner session over that root. Together they implement the delivery-record contract in
[Controller delivery](controller.md). This page is their storage: what one root contains, which
locks serialize it, and what cleanup may remove. The logical guarantees stay with the contract.

## Layout and locks

`FileLedger` maps the authenticated delivery identity to a fixed lowercase digest. Provider text
never becomes a path. One controller-owned root contains fixed metadata and locks plus bounded row
files:

```text
.amiss-root.state
.amiss-capacity.state
.amiss-maintenance.lock
.amiss-admission.lock
.amiss-clock.lock
.amiss-row-00.lock ... .amiss-row-ff.lock  (created only when used)
<delivery-key>.state
<delivery-key>.report                     (only while a result needs it)
```

The maintenance lock is shared by ordinary row work and exclusive during cleanup. The admission
lock serializes capacity recovery, reservation, and creation of a new row. The clock lock
serializes durable high-water updates. The first byte of the delivery digest selects one of 256
stable row-lock files; a shard collision may serialize unrelated rows but cannot let two processes
win one transition. These fixed names avoid one permanent lock file per delivery.

## Frames and replacement

Root metadata is itself a versioned, checksummed frame. It fixes the lease duration, maximum record
count, and signed-age and queue ceilings for every process using that root, and stores the highest
trusted controller time the ledger has seen. Opening the same root with a different lease, record
cap, or replay window fails.

A separate checksummed capacity frame holds the record limit, a slot count that never understates
use, and at most one pending row key. Before a new row is written, its slot and key are saved; after
the row is written, the pending key is cleared. If the sequence is interrupted, the next new-row
admission or full cleanup checks that exact row path and finishes the update. Before cleanup
deletes a batch of ended rows, it saves one cleanup marker; after deletion it saves the exact count
once. An interrupted batch leaves a safe upper bound and is reconciled by the next root open or
explicit cleanup. Ordinary admission reads the bounded capacity frame and requested row; it does
not walk the root directory. Once the cap is full, a new identity fails before its state file is
created, while an existing row can still renew, save, publish, and complete. Operators must size
the cap to include permanent replay markers.

Opening `FileLedgerRoot` validates the complete root, runs cleanup, and prepares the store once.
Creating a session chooses a fresh owner identity without scanning or cleaning the root.
`FileLedger::open` remains the convenience form of opening a root and immediately creating one
session, so it still performs the startup scan.

The root metadata written by v0.9 is validated and upgraded in place, and its existing rows seed
the first capacity frame. The migration changes root-level bookkeeping only; row and report bytes
are unchanged, and the older row schema remains rejected. After the upgrade, a missing capacity
frame or an unmarked count disagreement with the decoded rows is corruption. Stop every v0.9
controller process before the first upgraded open; the metadata upgrade is one-way.

The state file is a versioned, length-delimited, checksummed frame containing canonical JSON and is
capped at 128 KiB. The reader accepts only its current row schema. The older v2 schema contains no
check-plan binding, so it is rejected instead of attaching a caller-supplied policy to old work; a
future schema change needs an explicit migration that preserves every stored authorization field.
A report is kept separately at one fixed path, bounded by the machine-report byte ceiling, while
its digest and length remain in the saved state. Saving removes any dead report, writes and syncs
the new report, then atomically replaces the state that names it. Completion first saves `done`,
then removes the report. A stop between those steps can leave an unreferenced report, but cannot
expose a saved state whose report was never written. Retrying completion and cleanup both remove
that dead file.

The implementation uses Rust's standard `File::lock` and the `atomicwrites` crate, leaving the
operating-system calls behind those maintained boundaries. Replacement first syncs the new file.
On Unix the crate replaces the destination and syncs its parent directory; on Windows it uses
`MoveFileExW` with replace-existing and write-through flags. `FileLedger` therefore has one
cross-platform contract on supported local filesystems: the current path contains either the old
complete bytes or the new complete bytes. A stopped write may leave a temporary file, but cannot
make partial bytes current.

The root must already exist as a real, private local directory outside the repository and action
tree. `FileLedger` rejects a missing root or a root symlink. The service operator must own the
directory and set its permissions or access-control list. Anyone who can read or change that
directory is inside the controller trust boundary. The checksums detect damage, not a malicious
writer. Shared and network filesystems are not supported.

## Cleanup and replay

Malformed, oversized, non-regular, unknown-field, non-canonical, or digest-mismatched saved data
fails closed, as does a missing report named by a saved state. Opening a root runs cleanup;
creating an owner session does not. The same cleanup operation is public for later maintenance.
Under the exclusive maintenance lock and the admission lock it validates the complete root and
saved reports, settles a pending addition or marked batch cleanup, and otherwise requires the saved
slot count to match the decoded rows. It then advances and saves the high-water clock before
removing unreferenced reports, recognized atomic-write leftovers, and bounded `done` rows strictly
after their inclusive replay end. It never removes running or saved work, even after that time, and
never ages out a permanent `done` row. Unknown root entries and unsafe temporary-directory shapes
fail closed instead of being deleted.

| Saved state | Cleanup rule |
| --- | --- |
| `running` | Keep it, even after a bounded replay end, because a worker may still own or reclaim it. |
| `staged` (result saved) | Keep the state and its valid report until publication can finish. |
| `done`, permanent | Keep the small state marker; it is the replay defense. |
| `done`, bounded | Keep it through the inclusive replay end, then remove it. |

Persisting the high-water clock before deletion means a local clock rollback cannot make an ended
delivery look fresh. A claim for a bounded delivery whose row is gone but lifetime has ended returns
`Expired`. Completion after deletion returns `Lost`, because the exact saved digest is gone; only a
retained exact `done` marker can return repeat-safe `Completed`. A new record receives a fresh
random evaluation suffix, so deletion cannot make a stale publication retry match a later row.
Together, the record cap, fixed lock set, per-file ceilings, and one report path per row bound the
named durable state. Known crash leftovers are removed on the next open or cleanup. Permanent
replay rows deliberately consume capacity until an operator changes trust policy outside this
record; cleanup must not guess an age for signatures that contain no trusted time.

Focused tests cover v0.9 migration, interrupted additions and batch cleanup, missing capacity or
row state, and exact capacity after cleanup. The weekly non-gating run measures admission with
1,000, 10,000, 50,000, and 100,000 retained root entries, then records full-capacity rejection and
full-cleanup cost separately.
