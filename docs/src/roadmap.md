# Roadmap

This page contains future work, not release notes or a promise that every candidate will
ship. The factual boundary of the current product is in [Project status](status.md), and
completed changes move to the
[changelog](https://github.com/HardMax71/amiss/blob/main/CHANGELOG.md) instead of being
repeated here.

## Now: validate and harden

The scanner's claims are exact, so validation is about counting error classes, not
collecting impressions. The work in this phase, with its instruments:

- Book alignment is already mechanical and stays that way: the documentation contract
  tests generate the disposition and ceiling tables, compare the meaning sentences, the
  grammar, and the llms index, and execute every schema-backed example. A claim that can
  be generated gets generated; the rest links its implementation.
- Parser CPU had one named hole: the pinned MDX lexer's quadratic unterminated-region
  case, recorded in the
  [corpus notes](https://github.com/HardMax71/amiss/blob/main/corpus/README.md). An
  in-parse bound closed it in July 2026: every candidate close charges the accumulated
  region against the `aggregate-embedded-code-evaluation-bytes-per-snapshot` ceiling, a
  crossing is an ordinary reported resource row, and the trip is pinned by test. The
  convenience Action now carries a wall-clock watchdog on top; its default is the
  120-second window bootstrap enforces on its lane, and a workflow input can move it.
- Shadow scans have started. Six public repositories were scanned in July 2026: helix,
  ripgrep, just, mdBook, starship, and ruff. Two were spotless (ripgrep at 766 references
  and just at 3,101, zero missing in both). One carried a single real introduced break.
  Three mapped systematic non-adoption classes: clean URLs a site router resolves,
  deliberately broken test fixtures, and targets a docs build generates. The ledger
  going forward records, per repository, the reference counts, the missing counts, and
  the class of every finding a maintainer would reject.
- A false `explicit-target-missing` on a supported reference is a resolver bug, not a
  statistic: it gets a pinned test, and the accepted count of such bugs is zero.
- Reviewer burden gets a defined metric before it gets a threshold: advisory rows per
  hundred changed documentation lines, recorded per scan. The gate threshold is chosen
  after ten recorded scans on repositories that are not this one.
- The event matrix needs recorded runs, not assumptions. The self-scan exercises push
  and pull-request paths today; merge groups, fork pull requests, shallow checkouts, and
  staged-index runs in hosted CI each need an end-to-end fixture or a recorded run.
- Trend instruments accumulate on their own schedules: the weekly non-gating mutation
  run and the nightly coverage-guided fuzz run. The bar for calling a property stable is
  two months of those runs without an unexplained regression.

This phase exits when every bullet above is either closed or has its recorded numbers:
the MDX bound decided, the ledger at ten or more repositories, zero open false-missing
bugs, the reviewer-burden threshold chosen from data, and the event matrix covered.

## Next: provider-verified controls

Conditional on the validation phase. The parsers, control semantics, and bootstrap
supervision already exist as library surfaces; [Project status](status.md) records their
exact maturity. What remains is integration and an independent trust boundary:

- acquire and authenticate provider-created evaluation and control requests;
- implement provider adapters that translate independently authenticated run context
  into the forge-neutral request contract; the GitHub composite Action is a convenience
  adapter, and GitLab, Gitea-family, and other authenticated adapters are unsupported;
- feed the authenticated request into the engine instead of the all-absent control shell
  the CLI constructs today;
- include and invoke `amiss-bootstrap` in the protected required-check path;
- define trust anchors, freshness, revocation, and replay behavior;
- cover wrong identity, wrong tree, expiry, replay, missing output, timeout, and
  tampered runtime closure in end-to-end negative tests.

The lane is ready when the verifier is acquired independently of the action tree it
checks, every authorization binds the exact repository and tree, and a report
distinguishes verified provenance from local self-assertion without relying on
repository-controlled input.

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
measured review burden, and experiments covering persistence and concurrent branches.
Until then these are design vocabulary, not advertised capability.

The permanent boundaries stay in [What Amiss is not](non-goals.md): no semantic truth
verdicts about prose, no repository-executed hooks, no live-network validation inside the
engine, no automatic prose rewriting, and no repository-controlled weakening of a
required policy.
