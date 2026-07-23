# The file ledger

`FileLedger` is the shipped implementation of the delivery-record contract in
[Controller delivery](controller.md). This page is its storage: what one root contains, which
locks serialize it, and what cleanup may remove. The logical guarantees stay with the contract.

## Layout and locks

`FileLedger` maps the authenticated delivery identity to a fixed lowercase digest. Provider text
never becomes a path. One controller-owned root contains fixed metadata and locks plus bounded row
files:

```text
.amiss-root.state
.amiss-maintenance.lock
.amiss-admission.lock
.amiss-clock.lock
.amiss-row-00.lock ... .amiss-row-ff.lock  (created only when used)
<delivery-key>.state
<delivery-key>.report                     (only while a result needs it)
```

The maintenance lock is shared by ordinary row work and exclusive during cleanup. The admission
lock serializes the count-and-create step for a new identity, and the clock lock serializes durable
high-water updates. The first byte of the delivery digest selects one of 256 stable row-lock files;
a shard collision may serialize unrelated rows but cannot let two processes win one transition.
These fixed names avoid one permanent lock file per delivery.

## Frames and replacement

Root metadata is itself a versioned, checksummed frame. It fixes the lease duration, maximum record
count, and signed-age and queue ceilings for every process using that root, and stores the highest
trusted controller time the ledger has seen. Opening the same root with a different lease, record
cap, or replay window fails. New identities are counted and admitted atomically; once the cap is
full they fail before a state file is created, while an existing row can still renew, save, publish,
and complete.
Operators must size the cap to include permanent replay markers.

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
fails closed, as does a missing report named by a saved state. Opening a root runs cleanup, and the
same operation is public for later maintenance. Under the exclusive maintenance lock it advances
and saves the high-water clock, validates the complete root, then removes unreferenced reports,
recognized atomic-write leftovers, and bounded `done` rows strictly after their inclusive
replay end. It never removes running or saved work, even after that time, and never
ages out a permanent `done` row. Unknown root entries and unsafe temporary-directory shapes fail
closed instead of being deleted.

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
