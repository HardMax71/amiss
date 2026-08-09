# A settled wire

Closed August 2026. A machine consumer of the report could not build against a contract
that reserved the right to reshape under it, and pinning an engine release only moved the
problem. This phase gave the wire its own version, decoupled from engine releases, ran a
public quiet period under a mechanical tripwire, and froze the contract at `1` when the
last condition closed. [The report](../report.md) stays the live chapter; this page
records how the freeze was earned.

The mechanism shipped first, in v0.18.0. `compatibility` is the payload's own version and
travels inside every report, so a consumer reads stability from the report rather than
from the version of the binary that wrote it. Engine releases were declared to mean
engine behavior only. The clock got a tripwire in the tree: the example the last release
shipped is retained beside the rolling one, refreshed by the release workflow, and a
contract test fails the build the moment it stops clearing the current schema and reader,
which is exactly a reshape.

Three conditions had to hold at once, and each has its own exit evidence. Every supported
lane's trust story closed on retained live evidence, recorded in
[Live provider evidence](live-provider-evidence.md). The release carried no half-built
trust path: the launcher placeholder was cut, and the verified-consumption lane is the
attestation recipe in [Security model](../security.md), checked against every release the
workflow publishes. And two consecutive minor series shipped without reshaping the
payload: v0.19.0 and v0.20.0 carried the fix, claim, and adopt verbs, two report
projections, and a policy grammar binding, all additive, with the tripwire green through
both.

The freeze itself is additive law made mechanical. The writers emit `1` from one wire
constant, the schema pins the same value by a contract test, and the first frozen example
is retained permanently at
[`spec/examples/scanner-report.frozen-1.json`](https://github.com/HardMax71/amiss/blob/main/spec/examples/scanner-report.frozen-1.json),
byte-pinned against edits, with every later schema in the major required to keep
validating it. A `1` report may gain optional fields as `1` rolls forward; nothing a
`1.0` consumer parsed ever changes meaning or disappears. Reshaping past that promise
mints `2`, and since a new contract breaks consumers whatever the binary is called, that
release is a major one. The engine's own `1.0` remains a maturity statement about the
engine, made on its own grounds; the wire did not wait for it and does not follow it.
