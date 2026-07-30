# Delivery record

Closed July 2026. A controller that publishes provider verdicts has to survive crashes, retries,
and clock movement without losing a verdict or writing two. The lifecycle arrived in
[#98](https://github.com/hardmax71/amiss/pull/98),
[#100](https://github.com/hardmax71/amiss/pull/100), and
[#103](https://github.com/hardmax71/amiss/pull/103), was finished in
[#105](https://github.com/hardmax71/amiss/pull/105), and was bounded in
[#118](https://github.com/hardmax71/amiss/pull/118).

## Claim, lease, result, and completion are one contract

The controller before this could preserve an evaluation ID across retries and little else. It had
no way to represent two workers owning the same delivery, and no way to represent the window
between finishing an evaluation and publishing it, which is exactly where a crash costs you
either a lost verdict or a second one. A stale worker could not be rejected, because there was
nothing to reject it with, and a retry had no immutable result to resume from.

`DeliveryLedger` replaced that with one atomic claim whose answer is the whole coordination
contract:

```rust
pub enum DeliveryClaim {
    Execute(DeliveryLease),
    Publish(StagedPublication),
    Busy {
        evaluation_id: ControllerEvaluationId,
        retry_at_unix_millis: i64,
    },
    Duplicate {
        evaluation_id: ControllerEvaluationId,
    },
    BindingConflict,
}
```

`Execute` grants ownership. `Publish` hands back a result a previous owner already froze, so the
retry publishes rather than recomputes. `Busy` says someone else holds it and when to come back.
`Duplicate` is reserved for terminal, durably completed work, and `BindingConflict` for a
delivery whose identity does not match what the record already holds under that key.

Ownership is fenced rather than timed. The lease carries a monotonic `fence`, and the deadline in
it is advisory for a reason worth repeating:

```rust
/// Advisory deadline; only the ledger transaction decides ownership.
pub expires_at_unix_millis: i64,
```

A worker that believes its lease is live is not the authority on that. The transaction is. That
one decision removes the class of bug where two processes disagree about a clock and both
publish.

Three rules follow and are pinned by test. An owner whose lease has expired cannot save a new
result. A result saved before expiry stays publishable on retry, because the work was real and
the clock running out later does not make it wrong. A retained completion marker is repeatable
without granting new work, so a redelivery is answered from the record instead of evaluated
again.

The contract is
[`controller/src/orchestration/ledger.rs`](https://github.com/hardmax71/amiss/blob/main/controller/src/orchestration/ledger.rs)
and the operator view is [Controller delivery](../controller.md).

## The record and the runner share one lease

Two components with two ideas of who owns a delivery will eventually both act on it, and the
result is either a duplicate verdict or a stale one overwriting a current one. The usual fix is a
longer timeout, which converts a common bug into a rare one and makes it harder to reproduce.

The controller record and the runner share a single lease contract instead. The runner renews
before its relative lease window closes rather than after, and losing ownership stops the run
rather than letting it continue to publication. Ownership loss is a stop condition, not a race to
finish first. A worker that was paused long enough to lose its lease finds out at the next
transaction, and the work it was doing is dropped rather than published late.

## Every accepted delivery carries a replay lifetime

Replay suppression needs an end. Keep every delivery identity forever and the record grows
without bound. Forget one too early and a signed request that is still valid becomes replayable,
which is a security hole wearing the costume of a cleanup job. The question is who decides when
forgetting is safe, and the answer cannot be whoever is asking.

Trusted ingress stamps each accepted delivery with a lifetime at admission, derived from what the
request itself can prove. A delivery authenticated by exact body, or by a scheme that only proves
replay identity, is permanent, because nothing in it says when it stops being valid and guessing
an end would be inventing a fact. A delivery carrying an authenticated ID and issue time gets a
fixed end computed from the controller's signed-age and queue ceilings, so the end comes from
configuration the operator set rather than from the sender.

A route may narrow freshness beyond that. A route may not extend the lifetime already stored.
That asymmetry is the whole point: the strict direction is always available, the permissive one
never is, so no per-route setting can quietly reopen a replay window the controller closed.

The Gitea family is the concrete case. Its native webhook signature covers the body and nothing
else, with no timestamp anywhere in the delivery, so its replay markers are permanent and the
provider page says so rather than implying a window that does not exist. Ingress is
[`controller/src/ingress.rs`](https://github.com/hardmax71/amiss/blob/main/controller/src/ingress.rs)
and the signature schemes are
[`controller/src/webhook/`](https://github.com/hardmax71/amiss/tree/main/controller/src/webhook).

## The record is ordinary files, not a database

A controller that needs SQL to hold its own delivery record inherits a service to run, back up,
patch, and secure, and it inherits it inside the trust boundary. For a component whose whole job
is to be harder to lie to than a CI job, that is a poor trade. `FileLedger` implements the entire
contract with ordinary files, cross-process advisory locks, and atomic replacement.

The root carries its own configuration. `.amiss-root.state` fixes the record cap and the replay
window and preserves a high-water clock, so reopening a root with different limits, with capacity
missing, or with damage that was never marked, fails closed instead of proceeding on assumptions.
Two processes cannot disagree about the shape of the record, because the record states its shape.

Capacity lives in a separate checksummed frame, `.amiss-capacity.state`, holding a slot count
that never understates use plus one exact pending key. The asymmetry is deliberate: a count that
is too high refuses a row that would have fit, which costs a retry, while a count that is too low
overfills the root, which costs the bound. The pending key means an addition interrupted by a
crash can be settled from that one row rather than by rebuilding the count from the directory.

Deletion is batched. One recovery marker and one final count write replace syncing the
bookkeeping for every row, so removing a thousand ended rows is one durable decision rather than
a thousand. Reading a root a previous version wrote is a real case rather than a hypothetical, so
v0.9 metadata migrates in place. The store is
[`controller/src/file_ledger/`](https://github.com/hardmax71/amiss/tree/main/controller/src/file_ledger)
and the operator view is [The file ledger](../file-ledger.md).

## Lock growth is fixed and admission does not scan

A record that takes a lock per row, or counts the directory on every insert, gets slower exactly
as it gets busier. On v0.9.0 that was measurable: admitting one new row into a root holding
100,000 retained entries took about 57.5 milliseconds, because admission counted what was already
there.

The lock set is fixed and small. One maintenance lock, one admission lock, one clock lock, and at
most 256 lazily created row-lock shards:

```text
.amiss-root.state        .amiss-maintenance.lock
.amiss-capacity.state    .amiss-admission.lock
.amiss-clock.lock        .amiss-row-7a.lock
```

Row locks are named by one hex byte of the row key, so the count is bounded by the shard space
rather than by the number of rows, and a busy root creates the same 256 files a quiet one does.

Admission stops scanning. The checksummed capacity frame answers "is there room" without reading
the directory, and the same measurement fell from about 57.5 milliseconds to about 0.25
milliseconds, with a full-capacity rejection at about 0.085 milliseconds. New identities are
admitted under the configured cap while work already inside the cap is allowed to finish, so
filling up refuses new arrivals rather than stalling what is already running.

`FileLedgerRoot` moved preparation and cleanup out of the request path. A service prepares and
cleans the root once, then creates independent fenced owner sessions without repeating startup
maintenance per request. Each new row also takes a fresh random evaluation suffix, so an old
retry cannot match a later row that happens to reuse a key after a safe deletion.

The measurement is not a merge gate, because a machine-specific timing threshold is a flaky test
wearing a stopwatch. A weekly release-mode run records admission, full rejection, and cleanup
separately at 1,000, 10,000, 50,000, and 100,000 retained entries, and the numbers are kept as
evidence rather than asserted. The run is
[`.github/workflows/bench.yml`](https://github.com/hardmax71/amiss/blob/main/.github/workflows/bench.yml).

## A row is one bounded state file and, briefly, a report

Unbounded per-row storage is how a broken or hostile provider fills a disk, and a partial write
is how a restart resumes into a state that never existed. Both are ordinary failures, so the row
format assumes them.

Each row is one bounded state file, plus one bounded report file for as long as the report is
needed and no longer. Neither can grow past its ceiling, so a row's cost is known before it is
written rather than discovered afterwards.

Write order carries the invariant. Saving a result writes the report before the state that names
it, so no state file ever points at a report that is not there. Completion writes `done` before
removing the report, so nothing removes the evidence while a claim on it could still be made. A
crash between any two steps leaves a state the next open can recognize, which is the property
that matters: not "this cannot be interrupted", but "an interruption is legible".

Both opening the root and explicit cleanup remove dead reports and known atomic-write leftovers,
so the debris of an interrupted write is collected by ordinary operation rather than by an
administrator noticing. The format is
[`controller/src/file_ledger/format/`](https://github.com/hardmax71/amiss/tree/main/controller/src/file_ledger/format).

## Cleanup removes only what is safe to forget

Cleanup is where a durable record usually gets quietly wrong. Too eager and it reopens a replay
window or deletes work in progress. Too shy and the root grows until admission starts refusing
real deliveries.

The rule is narrow on purpose: only completed rows whose authenticated replay lifetime has ended
are removed. Permanent completion markers stay, because a delivery with no provable issue time
has no safe end. Running work stays. Saved results stay, because a result frozen before its owner
expired is still the answer for that delivery.

Clock movement is the subtle case. A local clock that jumps backwards would make expired work
look live again, so the persisted high-water clock in the root metadata refuses to go backwards
and a rollback cannot reopen anything.

The pinned cases are the honest measure of the rule: the inclusive end of the window, clock
rollback, permanent retention, preservation of running and saved work, fixed lock growth,
behavior on a full root, recovery from an interrupted capacity update, and cleanup's own
fail-closed root scan. Cleanup that cannot read the root does nothing rather than assuming the
root is empty, which is the difference between a maintenance job and a data-loss incident. The
transitions are
[`controller/src/file_ledger/transitions/`](https://github.com/hardmax71/amiss/tree/main/controller/src/file_ledger/transitions).
