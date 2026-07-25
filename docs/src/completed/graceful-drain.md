# Shutdown finishes the work it already accepted

A service that exits on a signal drops the delivery it just acknowledged, and an acknowledged
webhook is one the provider will not send again. Restarts are ordinary. Losing a verdict per
deploy is not.

On a termination signal, in-flight HTTP work finishes. A webhook worker finishes its current
delivery and leaves the durable backlog for the next process, which is the right split: the
backlog is already in the inbox, so the next process will find it, while the delivery in hand
has state only this process holds. The GitLab lane also finishes admitted evaluations
and any running ledger maintenance, since maintenance interrupted halfway is what leaves a root
needing recovery.

A second termination signal aborts the process rather than waiting on a stuck drain. That is
the escape hatch an operator needs at three in the morning, and it is documented so nobody has
to discover it by holding a key down.

Added in [#122](https://github.com/hardmax71/amiss/pull/122). Drain is
[`controller/service/src/shutdown.rs`](https://github.com/hardmax71/amiss/blob/main/controller/service/src/shutdown.rs) with the endpoint
side in [`controller/service/src/probe.rs`](https://github.com/hardmax71/amiss/blob/main/controller/service/src/probe.rs).
