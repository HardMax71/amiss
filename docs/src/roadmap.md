# Roadmap

This page tracks the work ahead: what is being done now, and what stays research. It is
not release notes or a promise that anything listed here will ship. Coverage that has
landed is described where it works rather than here. The factual boundary of the current
product is in
[Project status](status.md), the exit evidence for phases already closed is in
[Completed phases](completed-phases.md), and version history is in the
[changelog](https://github.com/HardMax71/amiss/blob/main/CHANGELOG.md).

## Now: retain live provider evidence

The provider code chapter is closed. What remains cannot be produced by a local fixture: it
requires accounts and protected test projects on the providers themselves.

GitHub, Gitea, and Forgejo are done. [Retained provider runs](provider-evidence.md) holds a
positive and a revoked-control run for each, against github.com, Gitea 1.27.0, and Forgejo
16.0.1. Running them found four defects that every fixture had agreed with, so the lanes
themselves changed on the way.

- Retain positive and revoked-control runs from a GitLab project. Record the provider version,
  controller commit, and provider evidence. The lane's floor is 19.3 with Ultimate and 19.2.0 is
  the newest release, so this waits on GitLab. Local HTTP fixtures remain regression tests, not
  live-provider evidence.

## Toward a settled wire

`compatibility` stays `experimental` while the contract rolls, and the [status
page](status.md) says so wherever the report is described. It leaves experimental when
three things hold at once. The GitLab lane holds retained live evidence like the other
three, so every supported lane's trust story is closed. Two consecutive minor series ship
without reshaping the report payload, so the contract has shown it can hold still under
feature work. And the release carries no half-built trust path: the launcher placeholder
is cut, since a verifier the artifact supplies can never vouch for the artifact, and the
verified-consumption lane is the attestation recipe in [Security model](security.md),
checked against every release the workflow publishes. When all three hold, the next release freezes
the schema as 1.0, and from then on a payload reshape is a major version rather than a
rolling change. None of these carries a date; each is checkable from the repository and
its retained evidence.

## Research, not committed work

Typed snippet, value, inventory, tree, graph, transcript, narrative, and external claims
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
