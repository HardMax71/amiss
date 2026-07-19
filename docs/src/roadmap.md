# Roadmap

This page tracks future work and the evidence for completed phases; it is not release
notes or a promise that every candidate will ship. The factual boundary of the current
product is in [Project status](status.md).
Completed phase evidence stays in collapsed Done sections so the exit decision remains
inspectable; version-specific history stays in the
[changelog](https://github.com/HardMax71/amiss/blob/main/CHANGELOG.md).

<details>
<summary>Done: validation and hardening</summary>

- Book alignment is mechanical: documentation contract tests generate the disposition
  and ceiling tables, compare the meaning sentences, grammar, and llms index, and execute
  every schema-backed example. A claim that can be generated gets generated; the rest
  links its implementation.
- The pinned MDX lexer's quadratic unterminated-region case recorded in the
  [corpus notes](https://github.com/HardMax71/amiss/blob/main/corpus/README.md) is bounded
  in-parse. Every candidate close charges the accumulated region against the
  `aggregate-embedded-code-evaluation-bytes-per-snapshot` ceiling, a crossing is an
  ordinary resource row, and the trip is pinned by test. The convenience Action also
  carries a configurable wall-clock watchdog whose default is the bootstrap lane's
  120-second window.
- The [scan ledger](ledger.md) holds ten public repositories from July 2026: four
  spotless, three carrying only real breaks (helix's one introduced, bat's twelve and
  alacritty's one pre-existing), and three mapping systematic non-adoption classes. Every
  row records reference and missing counts, advisory rows, changed documentation lines,
  and the class of any finding a maintainer would reject.
- A false `explicit-target-missing` on a supported reference is a resolver bug, not a
  statistic. It gets a pinned test, and the accepted count of such bugs is zero.
- PR feedback is engine-owned and focused. Related findings are grouped by target, Fixes
  precede Checks, Existing findings never use PR annotations, scan failures stay
  separate, and the CLI and Action show at most ten items while retaining overflow in the
  full report.
- Hosted self-scans have recorded push, same-repository pull-request, depth-two shallow
  checkout, and staged-index paths. Separate fork and merge-group runs are not retained
  as phase gates. The fork path uses the same unprivileged pull-request workflow. As of
  July 2026 GitHub does not offer a merge queue to this public, user-owned repository. The
  `merge_group` trigger and event mapping remain in place for repositories where a queue
  is available.
- The weekly non-gating mutation run and nightly coverage-guided fuzz run are installed.
  The first mutation run recorded 2,728 mutants and 664 missed on 2026-07-18; after
  excluding the fixtures crate, the comparable baseline is 2,672 and 616. These runs
  continue independently, and no property is called stable until two months pass without
  an unexplained regression.

</details>

## Now: provider-verified controls

The validation phase is closed. The parsers, control semantics, and bootstrap supervision
already exist as library surfaces; [Project status](status.md) records their exact
maturity. What remains is integration and an independent trust boundary:

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
evidence that reviewers find it useful, and experiments covering persistence and concurrent branches.
Until then these are design vocabulary, not advertised capability.

The permanent boundaries stay in [What Amiss is not](non-goals.md): no semantic truth
verdicts about prose, no repository-executed hooks, no live-network validation inside the
engine, no automatic prose rewriting, and no repository-controlled weakening of a
required policy.
