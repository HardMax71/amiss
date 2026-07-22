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

<details>
<summary>Done: delivery record</summary>

- [`DeliveryLedger`](controller.md) fixes the four-state claim, lease, saved-result, and
  completion contract. An expired owner cannot save a new result, an exact result saved before
  expiry remains publishable on retry, and a retained exact completion marker is repeatable without
  granting new work.
- Trusted ingress gives every accepted delivery a replay lifetime. Exact-body and replay-only
  requests are permanent. A request with an authenticated ID and issue time gets a fixed end from
  the controller's signed-age and queue ceilings; a route may narrow freshness but cannot extend
  that stored lifetime.
- `FileLedger` implements the contract with ordinary files, cross-process file locks, and atomic
  replacement, without SQL or a database. Root metadata fixes the record cap and replay window and
  preserves a high-water clock. Reopening the root with different limits or damaged data fails
  closed.
- The root has one maintenance lock, one new-record lock, one clock lock, and at most 256 lazily
  created row-lock shards. New identities are admitted under the configured record cap, while work
  already inside the cap can finish. A fresh random evaluation suffix prevents an old retry from
  matching a later row after safe deletion.
- A row has one bounded state file and, only while needed, one bounded report file. Saving the
  result writes the report before the state that names it; completion saves `done` before removing
  the report. Opening the root and explicit cleanup remove dead reports and known atomic-write
  leftovers.
- Cleanup removes only completed rows whose authenticated replay lifetime has ended. Permanent
  completion markers, running work, and saved results remain. The persisted high-water clock keeps
  a local clock rollback from reopening expired work. Focused tests pin the inclusive end, rollback,
  permanent retention, preservation of running and saved work, fixed lock growth, full-root
  behavior, and cleanup's fail-closed root scan.

</details>

## Now: provider-verified controls

The validation and delivery-record phases are closed, and the provider-neutral foundation has
landed. The rolling evaluation contract now distinguishes the candidate or source ref used for URL
resolution from the protected target ref used by branch-scoped policy controls. For commit-pair
materialization, the bootstrap accepts a canonical evaluation/snapshot/controls request triplet,
validates its required constraint and trusted-time bindings, and carries the exact bytes to its
verified engine in a closed stdin frame. The separate, unpublished Rust workspace under
[`controller/`](https://github.com/HardMax71/amiss/tree/main/controller) defines opaque provider
identities and the provider-neutral contracts documented in
[Controller delivery](controller.md). That page defines the full delivery, ownership, retry, and
publication contract. The durable record is complete as described in the Done section above. The
workspace also contains the traits, orchestrator, bounded ingress gate, rotating key ring, and
GitHub, GitLab Standard Webhooks, and Gitea-family signature verifiers. The wire library can
produce canonical execution constraints and trusted-time statements instead of only parsing them.

That is foundation, not a supported provider lane. The controller has no HTTP server, concrete
GitHub, GitLab, or Gitea-family adapter, authenticated payload decoder, provider API client or
credential source, repository or action-tree acquisition worker, bootstrap runner, deployable
service, publication transport, or provider check publisher. The signature implementations are
not wired to a listener or authoritative provider refresh. The GitHub composite Action remains a
convenience event wrapper that launches the engine directly. No current path produces a provider-
verified sandbox or turns an engine report into independently authenticated evidence.

What remains is to put that library boundary behind a bounded HTTP receiver; build each provider
adapter against capabilities the provider actually offers; obtain API credentials independently
of the repository; connect exact repository and action-tree acquisition to `amiss-bootstrap` with
bounded polling, heartbeat cadence, whole-process-group cancellation, and a stable machine failure
channel; bound provider refreshes below the lease window; and make repeated publication of the
same source-bound required check safe. End-to-end negative tests must still cover wrong provider,
repository, change, ref, commit, tree, expiry, replay, revocation, missing output, timeout, and
tampered runtime closure. A route loader must select the replay-only policy for GitHub and
Gitea-family signatures, which expose no authenticated attempt time, and a bounded freshness rule
for GitLab Standard Webhooks, which authenticates its timestamp. Replay-only would be invalid
GitLab configuration.
Link dialect support in the engine's `forge` field is not evidence that an authenticated adapter
exists.

The lane is ready only when the verifier and authorization are acquired independently of the
repository and action tree under review, every authorization and published result bind the
exact provider instance, integration, repository, URL dialect, source, target and default-branch
refs, commits, and trees, and a consumer can distinguish provider-authenticated evidence from
the engine report's local assertions without trusting repository-controlled input. This phase
stays in Now until at least one provider satisfies that complete path.

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
