# Roadmap

This page tracks the work ahead: what is being done now, what could enter the roadmap
later, and what stays research. It is not release notes or a promise that every candidate
will ship. The factual boundary of the current product is in
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

## Reference-coverage candidates

Candidates, not scheduled milestones. Each enters the roadmap only when its entry
condition is met. Three have left this list by meeting theirs. The slugging rules of ten
renderers are pinned against the renderers themselves, so an anchor no rule publishes is now
an ordinary missing target. Router spellings are pinned the same way, harvested from three
routers rather than transcribed, so a destination the tree does not hold is asked again
under the spellings a router serves: it moved 247 of the 516 missing references across the
ten trees, starship's 241 and mdBook's 6, and moved nothing else. Both are described in
[Resolution](resolution.md).

- reStructuredText or AsciiDoc. Entry condition: a pinned grammar, a conformance corpus,
  extraction goldens, resource accounting, and honest opaque regions, the same set the
  Markdown adapters carry.

## Research, not committed work

Typed snippet, value, inventory, tree, graph, transcript, narrative, and external claims
remain research. Persistent acceptance records and governed review state reopen the
storage, concurrency, ownership, expiry, and cheapest-bypass problems the stateless
scanner avoids, the same problems that killed the ledger design in
[Provenance](provenance.md).

No claim kind becomes a milestone without design-partner demand, a proof-strength model,
evidence that reviewers find it useful, and experiments covering persistence and concurrent branches.
Until then these are design vocabulary, not advertised capability.

The permanent boundaries stay in [What Amiss is not](non-goals.md): no semantic truth
verdicts about prose, no repository-executed hooks, no live-network validation inside the
engine, no automatic prose rewriting, and no repository-controlled weakening of a
required policy.
