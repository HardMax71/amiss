# The delivery record has four states and no fifth

A controller that publishes provider verdicts needs to know, after any crash or retry,
whether a delivery is unclaimed, owned by someone, already evaluated, or finished. Guessing
means either a lost verdict or a second one.

`DeliveryLedger` fixes that as a four-state contract over claim, lease, saved result, and
completion. An owner whose lease has expired cannot save a new result. A result saved before
expiry stays publishable on retry, because the work was real and the clock running out does
not make it wrong. A retained completion marker is repeatable without granting new work, so a
duplicate delivery is answered rather than re-evaluated.

The contract is [`controller/src/orchestration/ledger.rs`](https://github.com/HardMax71/amiss/blob/main/controller/src/orchestration/ledger.rs) and the rules it serves
are on [Controller delivery](../controller.md). Introduced with the controller foundation in
[#98](https://github.com/HardMax71/amiss/pull/98), given its lease and publication lifecycle
in [#100](https://github.com/HardMax71/amiss/pull/100), and finished in
[#105](https://github.com/HardMax71/amiss/pull/105).
