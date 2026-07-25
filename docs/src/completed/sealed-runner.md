# The runner seals the job it supervises

Between acquiring objects and trusting a result there is a process to start. A process is where
inherited environment variables, inherited file descriptors, and leftover child processes turn
into a trust problem, and none of those show up in a happy-path test.

The runner rechecks the acquired repository and action roots rather than trusting that
acquisition put the right things there, derives a sealed job, and checks the pinned bootstrap
against its expected digest. It prepares private inputs, clears inherited environment and
streams, and supervises one cross-platform process tree. The controller owns the output
handles, so the child writes into handles it did not open and cannot redirect.

Reading the result is equally distrustful. Wall-clock and lease limits both apply, whichever
ends first. The output tree is proven empty before the run, so a leftover file cannot be read
as this run's answer. The report is bounded, and an incomplete or malformed result is rejected
rather than parsed optimistically.

The pinned failures are the shape of the guarantee: wrong roots, bootstrap tampering, bad
output, missing output, oversize output, timeout, heartbeat loss, and live descendants. That
last one matters most in practice, because a child that outlives its parent is how a runner
leaks work into the next job.

Sealed in [#106](https://github.com/hardmax71/amiss/pull/106), with acquisition wired in [#107](https://github.com/hardmax71/amiss/pull/107). The runner is
[`controller/src/bootstrap_runner.rs`](https://github.com/hardmax71/amiss/blob/main/controller/src/bootstrap_runner.rs), acquisition is
[`controller/src/acquiring_runner.rs`](https://github.com/hardmax71/amiss/blob/main/controller/src/acquiring_runner.rs), and the cases are
in [`controller/tests/`](https://github.com/hardmax71/amiss/tree/main/controller/tests).
