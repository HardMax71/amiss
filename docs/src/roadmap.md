# Roadmap

This page tracks the work ahead: what is being done now, and what stays research. It is
not release notes or a promise that anything listed here will ship. Coverage that has
landed is described where it works rather than here. The factual boundary of the current
product is in
[Project status](status.md), the exit evidence for phases already closed is in
[Completed phases](completed-phases.md), and version history is in the
[changelog](https://github.com/HardMax71/amiss/blob/main/CHANGELOG.md).

## Toward a settled wire

The wire keeps its own time. `compatibility` is the contract's version and travels inside
every report, `experimental` while the contract rolls and `1` once it freezes, so a
consumer reads the report's stability from the report rather than from the version of the
binary that wrote it. Engine releases mean engine behavior; they neither promise nor
threaten the wire.

The clock has a tripwire in the tree. The example the last release shipped is kept beside
the rolling one, refreshed by the release workflow, and a contract test fails the build
the moment it stops clearing the current schema and reader, which is exactly a reshape.

The contract freezes when three things hold at once. Two hold today: every supported
lane's trust story is closed on retained live evidence, and the release carries no
half-built trust path, since the launcher placeholder is cut and the verified-consumption
lane is the attestation recipe in [Security model](security.md), checked against every
release the workflow publishes. The last is quiet: two consecutive minor series ship
without reshaping the payload. A reshape changes or removes what an emitted field means; an
addition does not, so additions never reset the clock, because the frozen regime permits
exactly them.

Frozen means additive within the major. A `1` report may gain optional fields as `1`
rolls forward, and nothing a `1.0` consumer parsed ever changes meaning or disappears.
The promise gets its own fixture at the freeze: the first frozen example is retained
permanently, and every later schema in the major must still validate it. Reshaping past
that promise mints `2`, and since a new contract breaks consumers whatever the binary is
called, that release is a major one. The engine's own `1.0` is a maturity statement about
the engine, made on its own grounds; the wire does not wait for it and does not follow
it. None of these carries a date; each is checkable from the repository and its retained
evidence.

## Research, not committed work

Value claims shipped as the first evaluated kind: [Claims](claims.md) states the closed
grammar, and everything outside it keeps the unsupported-capability boundary. Typed
snippet, inventory, tree, graph, transcript, narrative, and external claims
remain research. Persistent acceptance records and governed review state reopen the
storage, concurrency, ownership, expiry, and cheapest-bypass problems the stateless
scanner avoids, the same problems that killed the ledger design in
[Provenance](provenance.md).

No claim kind becomes a milestone without design-partner demand, a proof-strength model,
evidence that reviewers find it useful, and experiments covering persistence and concurrent branches.
Until then these are design vocabulary, not advertised capability. Demand has a place to
land: open an issue on the repository naming the claim kind and the repository it would
gate, with one drifted example that reference checking cannot catch. The claim-demand
issue form asks for those three, and optionally what the claim should have
pinned. That register is what
this section reads before anything here becomes work.

The permanent boundaries stay in [What Amiss is not](non-goals.md): no semantic truth
verdicts about prose, no repository-executed hooks, no live-network validation inside the
engine, no automatic prose rewriting, and no repository-controlled weakening of a
required policy.
