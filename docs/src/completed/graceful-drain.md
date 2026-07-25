# Shutdown finishes the work it already accepted

A service that exits on a signal drops the delivery it just acknowledged, and an acknowledged
delivery is one the provider will not send again.

Graceful drain finishes in-flight HTTP work, and webhook workers finish the current delivery
before stopping rather than abandoning it mid-flight. What is already owned gets completed;
what has not been accepted is simply not accepted. That turns a deploy from a source of lost
verdicts into an ordinary restart.

Drain is [`controller/service/src/shutdown.rs`](https://github.com/HardMax71/amiss/blob/main/controller/service/src/shutdown.rs) with the endpoint side in
[`controller/service/src/probe.rs`](https://github.com/HardMax71/amiss/blob/main/controller/service/src/probe.rs). Added in [#122](https://github.com/HardMax71/amiss/pull/122).
