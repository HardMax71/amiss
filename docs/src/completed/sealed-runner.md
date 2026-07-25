# The runner seals the job it supervises

Between acquiring objects and trusting a result there is a process to start, and a process is
where inherited environment, inherited handles, and leftover children turn into a trust
problem.

The provider-neutral runner rechecks the acquired repository and action roots, derives a
sealed job, checks the pinned bootstrap, prepares private inputs, clears inherited environment
and streams, and supervises one cross-platform process tree. The controller owns the output
handles, applies both wall-clock and lease limits, proves the output tree empty before
reading, bounds the report, and rejects incomplete or malformed results. Focused tests cover
wrong roots, bootstrap tampering, bad or missing output, oversize, timeout, heartbeat loss,
and live descendants.

The runner is [`controller/src/bootstrap_runner.rs`](https://github.com/HardMax71/amiss/blob/main/controller/src/bootstrap_runner.rs) with acquisition in
[`controller/src/acquiring_runner.rs`](https://github.com/HardMax71/amiss/blob/main/controller/src/acquiring_runner.rs), pinned by
[`controller/tests/`](https://github.com/HardMax71/amiss/tree/main/controller/tests). Sealed in
[#106](https://github.com/HardMax71/amiss/pull/106), with acquisition wired in
[#107](https://github.com/HardMax71/amiss/pull/107).
