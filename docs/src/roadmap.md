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

<details>
<summary>Done: provider-verified controls</summary>

- The rolling evaluation contract separates the source ref used for URL resolution from the
  protected target ref used for branch controls. A frozen controller evaluation binds the
  provider, integration, repository, URL dialect, change, refs, commits, trees, provider gate,
  check plan, execution limits, and trusted time without enumerating provider-specific identity
  types.
- The bootstrap accepts the canonical evaluation, snapshot, and controls documents, checks their
  required bindings, and passes their exact bytes to the verified engine in one closed input frame.
  The wire library also produces canonical execution limits and trusted-time statements.
- The unpublished [`controller/`](https://github.com/HardMax71/amiss/tree/main/controller) workspace
  supplies the provider-neutral traits, orchestrator, bounded ingress gate, rotating key ring,
  signed-webhook checks, GitLab OIDC checks, and separate provider adapters. Provider differences
  stay in small crates rather than a closed provider enum.
- A bounded HTTP receiver authenticates before admission and saves the exact raw delivery before
  acknowledging it. Its ordinary-file inbox survives restart, enforces row and byte capacity,
  renews ownership during controller work, retries temporary provider failures, and removes raw
  bytes only after the delivery ledger has completed. It uses no SQL or database.
- The controller record and runner share one lease contract. The runner renews before its relative
  lease window closes, and loss of ownership stops the run instead of allowing stale work to
  publish. [Controller delivery](controller.md) defines the ownership, retry, and publication
  rules; the durable record is covered by the Done section above.
- The provider-neutral runner rechecks acquired repository and action roots, derives a sealed job,
  checks the pinned bootstrap, prepares private inputs, clears inherited environment and streams,
  and supervises one cross-platform process tree. The controller owns the output handles, applies
  wall and lease limits, proves the tree empty before reading, bounds the report, and rejects
  incomplete or malformed results. Focused tests cover wrong roots, bootstrap tampering, bad or
  missing output, oversize, timeout, heartbeat loss, and live descendants.
- The source-built GitHub service completes one lane for one repository, App installation, and
  protected target branch on GitHub.com or a compatible GHES release. Strict JSON loads the App
  key, rotating webhook secrets, external controls, execution constraint, bootstrap, and separate
  private state roots. The plaintext listener is deployed behind an operator-owned TLS and
  connection-limit boundary.
- The GitHub source accepts signed `opened`, `reopened`, and `synchronize` pull-request events,
  plus `edited` only for a signed base-branch change. Admission binds the configured repository and
  target. The App client refreshes exact repository, pull-request, ref, commit, tree, and
  test-merge facts and requires a strict active status rule whose context is bound to that App.
  It refreshes again before saving the result.
- Acquisition uses Git protocol v2 with exact authenticated SHA-1 wants for the repository and
  pinned action commit. One deadline covers network receipt and validation. Fixed fail-closed
  limits cap the pack at 2 GiB, objects at 2,000,000, each inflated stream or resolved object at
  128 MiB, aggregate inflated and resolved bytes at 4 GiB each, and delta depth at 128.
  `REF_DELTA` is rejected and pack indexing uses one thread.
- Publication attaches `success`, `failure`, or `cancelled` to GitHub's authoritative test-merge
  commit. Its summary binds the gate, provider run, refs, commits, trees, plan, execution
  constraint, report digest, and stable unavailable reason. The evaluation ID reconciles one
  exact visible retry; an accepted create with a lost reply can still leave a duplicate because
  GitHub and the local ledger do not share a transaction. A final pull-request refresh turns an
  out-of-order publication into a no-op when its staged head, base, refs, or gate is no longer
  current, so stale work cannot write to the newer gate.
- The source-built GitLab service completes the policy-job lane for one project and protected
  target branch on GitLab 19.3 or newer with Ultimate. A pipeline execution policy owned outside
  the checked project injects the job into every enforced merge train. The service authenticates
  the job's short-lived OIDC token and binds its policy project and commit, job and pipeline,
  runner, merge request, repository, and exact train-result commit before any provider state is
  trusted.
- GitLab refresh requires the configured merge method, the exact two train parents, an active
  policy job, a protected target branch with no push or bypass path, and merge-train enforcement
  for all users. The synchronous endpoint lets only the exact saved pass return success; block,
  unavailable, duplicate, expired, replayed, or changed state keeps the policy job failed.
- The source-built Gitea-family service completes one lane for Gitea 1.27 or newer and Forgejo 16
  or newer. It authenticates the native exact-body HMAC, refreshes the pull request, commits,
  trees, effective branch rule, and reviewer identity, then publishes an approval or request for
  changes through one dedicated reviewer account.
- The Gitea-family gate requires one approval restricted to that reviewer, closed direct-push and
  bypass paths, stale and rejected review blocking, an up-to-date pull request, and administrator
  enforcement. The adapter checks the distinct Gitea and Forgejo capability shapes without
  guessing one forge from headers. A wildcard protection rule is supported through effective-rule
  lookup; one exact rule remains the easier setup to audit.
- End-to-end and focused tests carry a signed delivery through authentication, durable admission,
  provider refresh, runner, provider gate, completion, and replay suppression. Negative cases
  cover wrong provider, repository, target, runner, policy, reviewer, commit, and tree; changed
  bootstrap or merge rule; expiry and replay; missing output and timeout; malformed or tampered
  input and state; capacity and restart; lost ownership; ref or gate drift; oversized and
  malformed packs; `REF_DELTA`; excessive delta depth; and conflicting provider evidence.
- Provider evidence lives in the App-owned Check Run, protected GitLab policy job, or dedicated
  Gitea-family review and the matching merge rule. The engine report remains unchanged and
  self-asserted; it has no provider signature or `provider_verified` field. Each provider page
  records its commit or tree freshness limit, retry behavior, rotation rules, and full trust
  boundary.

</details>

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
