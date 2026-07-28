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
condition is met. Two have left this list by meeting theirs. The slugging rules of ten
renderers are pinned against the renderers themselves, so an anchor no rule publishes is now
an ordinary missing target. Router spellings are pinned the same way, harvested from three
routers rather than transcribed, so a destination the tree does not hold is asked again
under the spellings a router serves: it moved 247 of the 516 missing references across the
ten trees, starship's 241 and mdBook's 6, and moved nothing else. Both are described in
[Resolution](resolution.md).

- Declared untracked targets. What is left after that split is ruff-shaped: a page the docs
  build writes and the tree never holds, 104 reference occurrences, 63 into `docs/settings.md`,
  and 60 of those carrying a fragment into a page whose headings exist only after generation.
  Enforce mode cannot be adopted there, because nothing can declare a generated target and
  policy only tightens. What changed is that the declaration no longer has to be invented.
  Both members of the class already publish the list, in a file written for Git rather than for
  this engine: ruff's `docs/.gitignore` names `/settings.md`, `/rules.md`, `/rules/` and three
  more, one exact path per line, and uv's root `.gitignore` names its four generated reference
  pages under a comment saying what regenerates them. So the rule sits where router spellings
  already sit. A relative destination the tree does not hold, and no spelling reaches, is asked
  once more against those declarations: an anchored literal line, no wildcard and no negation,
  resolved against its own ignore file's directory. A match is `target-declared-untracked`, a
  record under both profiles, never `explicit-target-missing`.

  It reclassifies rather than clears, which is what separates it from the bulk clearing
  [the evidence base](evidence.md) names as a gate's cheapest bypass. The reference stays in the
  report as a counted row, so a repository that stops tracking half its documentation shows a
  number instead of silence. Three properties carry the rest. The declaration is not authored
  for the gate, and adding a line costs Git tracking that path. Git ignores its own rules for a
  file already tracked, so no declaration can make a reference to a present file pass. And the
  cost stays one reviewed line per path, which holds only while the engine never asks whether a
  path is ignored and asks instead whether a tracked ignore file names exactly that path. That
  is set membership, not pattern matching, and honouring a single wildcard would rebuild the
  bulk clearing by hand. Measured before any code: seven literal lines cover 94 of ruff's 104
  missing occurrences and fourteen cover 54 of uv's 55. The ruff remainder names six paths and
  one anchor, and uv's is one anchor, all of them real.

  It does not answer whether the generated page publishes `#lint-select`, and it should not.
  Neither tree knows, ruff already runs `mkdocs build --strict` with anchor validation on every
  pull request, and [What Amiss is not](non-goals.md) leaves a permalink scheme with the
  generator that owns it. An inventory of published identities, bound to the digest of the
  source it was generated from, was designed and rejected: it reproduces user zero's railroad
  diagrams, where regeneration succeeded forever against a stale input. The rest of the class is
  three problems rather than one. Anchors an API generator publishes on a one-line page, and
  anchors a repository's own hook publishes from a comment marker, are identities on a file the
  tree does hold. Definition-list terms are an eleventh rule in
  [What ten renderers call a heading](anchor-rules.md), where the union only grows. Transclusion
  is tree-answerable, because the included file is in the repository. The entry condition is
  met; the engine change is specified and not yet built.
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
