# Roadmap

This page tracks the work ahead: what is being done now, and what stays research. It is
not release notes or a promise that anything listed here will ship. Coverage that has
landed is described where it works rather than here. The factual boundary of the current
product is in
[Project status](status.md), the exit evidence for phases already closed is in
[Completed phases](completed-phases.md), and version history is in the
[changelog](https://github.com/HardMax71/amiss/blob/main/CHANGELOG.md).

## Toward a settled wire

`compatibility` stays `experimental` while the contract rolls, and the [status
page](status.md) says so wherever the report is described. It leaves experimental when
three things hold at once. The first holds today: the GitLab lane retains live evidence
like the other three, so every supported lane's trust story is closed. Two consecutive minor series ship
without reshaping the report payload, so the contract has shown it can hold still under
feature work. And the release carries no half-built trust path: the launcher placeholder
is cut, since a verifier the artifact supplies can never vouch for the artifact, and the
verified-consumption lane is the attestation recipe in [Security model](security.md),
checked against every release the workflow publishes. When all three hold, the next release freezes
the schema as 1.0, and from then on a payload reshape is a major version rather than a
rolling change. None of these carries a date; each is checkable from the repository and
its retained evidence.

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
gate, with one drifted example that reference checking cannot catch. That register is what
this section reads before anything here becomes work.

The permanent boundaries stay in [What Amiss is not](non-goals.md): no semantic truth
verdicts about prose, no repository-executed hooks, no live-network validation inside the
engine, no automatic prose rewriting, and no repository-controlled weakening of a
required policy.
