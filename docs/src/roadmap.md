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
condition is met.

- Declared generated targets. The July 2026 shadow scans made this the largest measured
  adoption blocker: documentation that links pages the docs build generates (ruff's
  `settings.md`, 59 references) or clean URLs the site router resolves (starship's
  preset pages, most of its 242 missing rows). Enforce mode cannot be adopted there,
  because nothing can declare a generated target and policy only tightens. The candidate
  contract is a declared, digested list of generated targets, visible in every report.
  Entry condition: a design that keeps "no suppression" true, plus two design-partner
  repositories from this class.
- Heading anchors. Entry condition: a pinned slugging corpus for each supported
  renderer, because checking the file while guessing the anchor would create false
  passes.
- reStructuredText or AsciiDoc. Entry condition: a pinned grammar, a conformance corpus,
  extraction goldens, resource accounting, and honest opaque regions, the same set the
  Markdown adapters carry.
- Bare-path inference. Entry condition: precision measured against a hand-labeled corpus
  of path-like prose, high enough to justify the ambiguity and reviewer load it
  introduces. Until measured, it stays advisory research.

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
