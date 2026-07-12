# Pre-implementation red-team: decisions the design must freeze before coding

Date: 2026-07-11.

Closure status (2026-07-11): all findings below now have a normative resolution, typed deferral,
evidence gate, or external owner in
[issue-closure-matrix.md](./issue-closure-matrix.md). The fact that a transition is specified does
not mean its adversarial test has passed; governed work remains closed by
[implementation-readiness.md](./implementation-readiness.md).

Current-contract correction: descriptions of refresh, trust-on-edit, state-writing
initialization, inferred comparison bases, bulk acceptance, and SARIF below are historical attack
inputs, not supported operations. Scanner v0 is read-only and evaluates provider-supplied immutable
base and candidate snapshots. External adoption debt carries only an exactly equal finding fact.
Governed acceptance, if later unlocked, handles one claim at a time; split and merge use their own
closed lifecycle transactions.

## Verdict

The dossier has a sound product thesis and an unsafe executable contract. It is ready for a
report-only spike, but not for a stable lock format, an attestation API, or a blocking CI workflow.
The main problem is not a missing feature. Several current requirements are mutually exclusive:

- [design.md](./design.md#what-stale-means-exactly) says an edited block is a new identity and is
  fresh by construction;
- [design.md](./design.md#relationship-states) says `clean-attested` means both sides equal their
  last reviewed projections;
- [design.md](./design.md#the-lock-and-the-refresh-lane) lets an automatic refresh create baselines
  for new identities;
- [failure-modes.md](./failure-modes.md#ritual-compliance) says a document edit must never itself be
  accepted as evidence of review;
- [prior-art.md](./prior-art.md#9-implications-for-a-new-ci-checker) requires an explicit human
  acceptance event.

As written, a typo can retire a stale identity, the refresh bot can baseline its replacement, and
the output can call the result attested. That is exactly the meaningless green the product promises
never to emit.

The corrective move is to separate **automatic observation**, **explicit attestation**, **formal
verification**, and **policy disposition**. They are independent facts and must not be collapsed
into one `fresh`/`stale` state. Once that separation is made, identity, lifecycle, concurrency,
suppressions, and first-run behavior become implementable without overclaiming.

## Non-negotiable invariants

These should become a short normative specification before implementation starts. Other dossier
documents can remain design rationale, but code and tests need one source of truth.

1. A scan never mutates repository state.
2. Scanner v0 has no state-writing initialization or refresh operation; automatic activity can
   neither create nor advance an acceptance.
3. Only an explicit acceptance transition can make an attestation current.
4. Any change to the governed subject, dependency set, selected projections, scope, relation type,
   or selector-engine meaning invalidates the previous attestation.
5. A document edit does not silently clear a review obligation.
6. A governed claim has an immutable logical ID distinct from its locator and content digest.
7. Removing, splitting, merging, or retargeting a governed claim is an auditable lifecycle
   transition, not orphan garbage collection.
8. A hard gate evaluates the actual candidate tree that may merge. Attribution improves reporting;
   it never excuses an unhealthy protected claim.
9. Candidate changes to policy, declarations, scopes, suppressions, and the lock are evaluated as
   security-sensitive transitions, not merely as inputs to the new policy.
10. No result says or implies that arbitrary prose is true, complete, fresh, or globally in sync.
11. Coverage and evaluation are separate dimensions. Zero findings over zero governed claims is not
    coverage.
12. A selector is never silently retargeted to a merely similar artifact.

## Priority summary

| Rank | Meaning | Count | Ship consequence |
|---|---|---:|---|
| P0 | Must be decided before freezing the data model, directive syntax, lock format, or public CLI | 14 | Coding these ambiguities creates migrations or unsound greens |
| P1 | A read-only report prototype can precede the decision, but no blocking or assurance claim can | 5 | Resolve before a required CI beta |
| P2 | Explicitly out of the initial product; safe to defer if reported as unsupported | 8 | Do not leave half-implemented placeholder semantics |

## P0: implementation blockers

### P0-01: Automatic baselines are being called attestations

**Contradiction.** New identities are “fresh by construction,” initialization trusts the present,
and refresh records new baselines. Elsewhere, first attestation is a review and a fingerprint must
cover both sides. Those cannot share one state.

**Resolution.** Model four orthogonal dimensions:

| Dimension | Required states | What creates or changes it |
|---|---|---|
| Resolution | `resolved`, `missing`, `ambiguous`, `unsupported`, `error` | Deterministic selector evaluation |
| Observation | `unchanged`, `changed`, `deleted`, `unavailable` | Recomputed from the explicit base and candidate snapshots; never persisted by scanner v0 |
| Attestation | `unattested`, `attestation-current`, `review-required` | Only `assure accept` may advance it |
| Verification | `not-run`, `passed`, `failed`, `expired` | A named deterministic validator or scheduled probe |

Policy then maps the resulting facts to `fail`, `warn`, `record`, or a separately represented
waiver. A relation can validly be `resolved`, `changed-since-observation`, `unattested`, and
`verification-not-run` simultaneously. Do not force that into one enum.

An edited governed subject remains `review-required` until an explicit, one-claim acceptance
records the new subject and evidence fingerprints. Scanner observations are report facts for one
evaluation, not stored baselines. There is no refresh lane and no trust-on-edit policy: an edit may
be reported as a co-change, but it cannot clear either a structural failure or a governed review
obligation.

### P0-02: Content-derived identity cannot support a stable audit trail

**Contradiction.** The proposed “stable relationship ID” is variously:

- normalized block content plus document path;
- document path, section anchor, claim kind, and ordinal;
- a heading-independent block identity with an exact-duplicate ordinal.

Content-derived identity changes on every substantive edit. Anchor-derived identity changes on a
heading rename. Duplicate ordinals shift when identical blocks are inserted or removed. None can
support stable suppressions, ownership, retirement, split/merge history, or SARIF fingerprints.
Exact duplicates are not harmless once their sections have different lifecycle, owner, scope, or
policy.

**Resolution.** Separate three concepts:

- `ClaimId`: immutable and opaque to the matching algorithm. Governed claims carry a unique,
  human-readable ID in their declaration, such as `expr-precedence`.
- `SubjectLocator`: mutable path plus robust quote/context and structural hints used to rediscover
  the subject.
- `SubjectProjection`: the current normalized content whose digest participates in attestation.

Existing zero-touch references may use churn-prone generated `ObservationId` values because they
are observations, not governed claims with an audit promise. Promotion to a blocking narrative
claim requires a stable `ClaimId`. A content edit changes the projection, not the ID. A move changes
the locator, not the ID. Ambiguous re-anchoring produces `identity-conflict` and requires a
migration decision.

### P0-03: The ledger currently contains hidden authored intent

**Contradiction.** The document is called the single source of truth for what is claimed, yet the
attestation flow can prune inferred references and freeze the resulting selector set only in the
ledger. The zero-touch lane also stores whole claims only in the ledger. At that point the ledger is
not machine state; it is a second, invisible authoring surface.

**Resolution.** Enforce this boundary:

- Native links and path references are self-declaring structural observations.
- Bare-token, symbol, co-change, and model inference produces candidates only.
- A governed typed relationship, including a pruned dependency set, must be represented in a
  document declaration or a named root check referenced by that declaration.
- The lock stores resolved selectors, fingerprints, lifecycle state, and acceptance metadata; it
  never becomes the sole source of relation type, authority, dependency intent, or scope.

Drop ledger-only governed claims from the first implementation. An editor extension can make them
convenient later, but an invisible command-side claim and a claim “authored in the document” are
different product choices.

### P0-04: The proposed `[assure]` syntax cannot represent the model

**Contradiction.** Multiple identical `[assure]: ...` reference definitions in one Markdown file are
not independent declarations; reference labels are document-wide and duplicate definitions have
parser-defined winner semantics. The same label is also proposed for `[assure]: skip`. One path-like
destination cannot encode a stable claim ID, relation type, multiple selectors, scope, or versioned
grammar. `path#Symbol` is ambiguous with an ordinary URI fragment.

**Resolution.** Write and test a directive RFC before a parser:

- Every governed declaration uses a unique label, for example `[assure:expr-precedence]`.
- Its destination uses a versioned `assure:` URI grammar with percent-encoded path and symbol
  components.
- The label suffix is the stable `ClaimId`.
- Simple declarations may name one selector directly; multi-selector and parameterized relations
  reference a named check in the root configuration.
- Relation type is explicit or has one documented default (`describes`), never inferred from a
  selector string.
- Local skip syntax is not overloaded onto the declaration label.
- Each format adapter has conformance fixtures proving identical extracted semantics. Unsupported
  formats cannot silently fall back to a different scope rule.

Until that RFC exists, implement only native link/reference extraction. Do not ship the illustrative
syntax in [design.md](./design.md#authoring-surface) as though it were already a schema.

### P0-05: Relation types are labels without transition semantics

**Gap.** A typed hypergraph is proposed, but no normative rule defines which side is authoritative,
which changes invalidate what, whether a validator is mandatory, or how multiple assurance lanes
compose. “Forbidden cycle” is listed as a hard error even though equivalence and mutual constraint
relations can legitimately form cycles.

**Resolution.** Start with a closed relation ADT:

| Type | Authority and change rule | Valid completion |
|---|---|---|
| `reference` | Document names a target; only structural resolution is asserted | Target resolves in the relation's scope |
| `describes` | Document subject depends on evidence; a change on either side requires attestation | Explicit current attestation, optionally plus validators |
| `generated-from` | Output depends on declared inputs and generator identity | Sandboxed regeneration/equality passes |
| `constrains` | Normative document constrains implementation | Declared conformance validator passes, or a separately named implementation attestation is current |
| `equivalent` | Neither side alone is authoritative | A two-input deterministic consistency check passes |
| `historical-at` | Document intentionally describes an immutable revision | Pinned revision resolves; current-tree changes do not propagate |

Each type must declare allowed selector kinds, whether zero dependencies is legal, change
propagation, legal lifecycle transitions, and aggregation. Multiple assurance mechanisms retain
separate results; `verification-passed` must never erase `review-required`. Enforce acyclicity only
for derivation relations whose semantics require a DAG. Group other cycles as strongly connected
components for reporting.

### P0-06: Fingerprint and lock formats are not actually chosen

**Contradiction.** The canonical example says SHA-256, the implementation sketch says BLAKE3 or
xxHash3, and prior art mentions Git OIDs. A non-cryptographic hash lets an adversarial contributor
target collisions in a security-sensitive gate. The canonical JSON rules and lock transition
schema are also unspecified.

**Resolution.** Freeze a versioned, domain-separated format before producing real lockfiles:

- SHA-256 is the portable initial algorithm; optimize only after measurement. Do not use xxHash as
  the acceptance integrity digest.
- Canonicalize with a named standard or fully specified byte encoding, including Unicode, newline,
  set ordering, path case, and binary handling.
- Store raw and projected digests separately.
- Compute the attestation digest over schema version, immutable `ClaimId`, declaration digest,
  scope digest, subject projection, sorted dependency projections, and selector-engine versions.
- Store the predecessor attestation digest so CI can perform compare-and-swap validation.
- Use deterministic JSON Lines sorted by `ClaimId`, including tombstones; a later sharding scheme
  must preserve the same logical transaction rules.

Do not record the SHA of the commit containing its own lock update: that is self-referential. The
lock can record the input tree/projection digests. A hosted service may associate the accepted
record with the final merge commit after the fact.

### P0-07: A valid lockfile is not an authenticated attestation

**Unsafe assumption.** “Machine-owned” is a convention. A contributor can edit the lock, run the
same open-source acceptance algorithm, fabricate a reason or actor field, delete an entry, or
downgrade policy. `verify-lock` can prove internal consistency, not that somebody reviewed the
prose. Git history identifies a committer, not the person who made a claimed decision.

**Resolution.** Define the trust levels honestly:

- A local acceptance record is `self-asserted`; its actor field is untrusted metadata.
- CI recomputes every changed record and validates its transition from the base lock, but cannot
  prove attention or reviewer identity.
- Repository review and protected ownership are the independent control for ordinary mode.
- A provider integration may add `provider-verified` reviewer evidence obtained from the review API.
- A commercial/tamper-evident audit trail requires an append-only, service-signed event outside the
  contributor-controlled tree. Git history alone must not be sold as that guarantee.

The base-to-candidate transition verifier must reject unexplained entry deletion, ID reuse,
predecessor mismatch, fingerprint mismatch, selector mutation disguised as acceptance, and
retirement without a lifecycle record. A contributor can still intentionally run a valid
acceptance; only review policy or signed service approval can distinguish that from careful review.

### P0-08: A refresh writer would perform lifecycle and security-sensitive writes

**Unsafe assumption.** The refresh lane currently adds baselines, retires orphans, migrates exact
matches, and may push with branch-protection bypass. Even with the “never update existing entries”
invariant, retirement deletes outstanding obligations, exact-match migration contradicts “never
silent,” and bypass lets a bot alter the graph without normal review.

**Resolution.** Do not implement a refresh actor or operation. Read-only evaluation may report an
orphan or migration candidate, but it never writes observations, selectors, suppressions,
acceptances, or lifecycle state and never opens a bot pull request. Governed creation, migration,
one-claim acceptance, split, merge, and retirement remain explicit closed transactions reviewed in
the candidate. A blocking relationship and its authorization must be complete in that exact
candidate.

### P0-09: Claim deletion, split, merge, and retirement have no safe lifecycle

**Gap.** Current orphan collection can erase a claim when its block or whole document disappears.
That makes deletion the cheapest way to clear a failure. Splits and merges are described as edge
cases but have no ledger transition.

**Resolution.** Governed claims use an explicit lifecycle:

- `active` can transition to `retirement-requested`, then `retired` with a reason and approval.
- `split` creates successor IDs and records the predecessor-to-successor mapping.
- `merge` creates or selects one successor and tombstones all predecessors with the mapping.
- `migrate` changes a locator or selector under the same `ClaimId` and records old/new resolution.
- Tombstones are permanent for ID reuse prevention and audit.
- Deleting a document or declaration containing active claims emits `governed-claim-removed`; it
  does not silently make the candidate healthier.

Ephemeral inferred observations may disappear without this ceremony because they never claimed
governed coverage. The output must keep that distinction visible.

### P0-10: Version scope is a field name, not an evaluation model

**Gap.** `main`, a release line, a frozen revision, and a deployed environment are listed, but the
design does not say how a selector resolves, where its ledger lives, how tags moving are handled,
or what a pull request should do when a page's scope is unknown. Automatic day-zero scanning risks
comparing frozen docs with current code.

**Resolution.** Make scope a closed ADT and include its resolved identity in every digest:

- `candidate-tree`: the exact tree being checked;
- `immutable-revision`: repository identity plus immutable commit digest;
- `environment-observation`: named probe plus immutable observation result and expiry;
- `external-observation`: named source plus immutable observation result and expiry.

For v0, support only `candidate-tree`. Detect conventional versioned/historical directories and
report `scope-unresolved`; do not compare them with current code or block on inferred impact. A
release branch runs the checker with its own branch-local lock. Frozen documents later use a commit
digest, not a mutable tag or `latest`. Environment and external scopes remain scheduled evidence
and cannot block a pull request until their time and trust semantics are separately specified.

Changing a governed claim's scope is a claim-definition transition requiring review. A policy rule
that relabels a live tree as historical is a policy weakening, not a free cleanup operation.

### P0-11: Merge-queue attribution is being used as a safety exception

**Impossible guarantee.** A generic merge-group candidate may contain changes from more than one
pull request. Evaluating base and candidate can identify a delta, but cannot always prove which
human pull request caused every relation state. More importantly, allowing queue-introduced
staleness to report instead of fail permits a protected attestation to merge against evidence it
was never accepted against.

**Resolution.** Separate safety from blame:

- A hard gate evaluates the exact final candidate tree and trusted policy. Every protected
  relationship must satisfy its invariant at that tree.
- Attribution is diagnostic only. If provider metadata permits a supported fact attribution, show
  it; otherwise report attribution as `unknown`. The run's event kind already records that it is a
  merge-group evaluation; do not invent a finding-attribution category for that event.
- Pre-existing structural debt is represented by an external record of the exact finding key and
  fact digest. It applies only while that candidate fact is exactly equal, not merely “no worse,”
  and never follows a changed fact.
- An attestation record contains its predecessor digest. Two concurrent PRs accepting the same
  claim create a compare-and-swap conflict; the second must rebase and re-attest.
- The merge-queue status reruns after every relevant candidate change. If an earlier queued change
  alters a selected dependency, a later acceptance legitimately becomes `review-required`.

Do not rely on checkout depth or a locally inferred merge base to discover the comparison commit.
Pass or explicitly fetch the provider-supplied immutable base and candidate object IDs, verify
their types, and record the exact trees evaluated. A default-branch job that turns red after merge
is monitoring, not a substitute for a pre-merge hard invariant.

### P0-12: Policy and suppression changes can erase their own findings

**Unsafe assumption.** A candidate can set `stale = ignore`, add `exclude = true`, relabel a path
historical, alter a named check, remove a declaration, or add `[assure]: skip`. If the checker uses
only candidate policy, the bypass removes the evidence of its own weakening. Current `ignore`,
`exclude`, `exempt`, `not-applicable`, local skip, and emergency override semantics also disagree on
reason and expiry requirements.

**Resolution.** Separate four concepts:

- **Severity policy** maps a finding kind to `fail`, `warn`, or `record`.
- **Scan exclusion** removes a path from evaluation and is always counted in coverage.
- **Lifecycle classification** says current, planned, or historical; it is not a suppression.
- **Waiver** suppresses one stable finding/claim under an explicit authority.

Every waiver needs a stable target, reason, owner, creation evidence, and absolute UTC expiry for a
live claim. A permanent historical exclusion is a reviewed classification, not an infinite waiver.
Emergency waivers are commit/claim scoped, expire, and never update a baseline.

Remove adjacent one-level skip from v0. Candidate policy/config/declaration changes produce
unsuppressible meta-findings such as `policy-weakened`, `coverage-reduced`, `claim-removed`, and
`validator-changed`, computed by comparing base and candidate configuration. Organizational floors
are trustworthy only when supplied by an externally protected required workflow, ruleset, or App;
an input in a repository-owned workflow is not an organizational control.

Wall-clock expiry is permitted for waiver governance, not content identity. CI's trusted clock is
authoritative; local evaluation must disclose when it cannot establish the same time context.

### P0-13: “Green” has no coverage-safe meaning and first-run claims are impossible

**Contradiction.** The product says it reads every code reference and flags text changed since last
verification, yet first run has no prior observation or verification. History is optional and the
checkout is shallow. Zero-link documents are legal. The scanner's scope is also both “every non-code
text file” and the much narrower allowlist in
[open-problems.md](./open-problems.md#product-honesty). A first run cannot detect pre-existing
semantic staleness without a formal validator or a trusted historical baseline.

**Resolution.** Make first-run and result semantics explicit:

- Use the document allowlist and built-in exclusions from OP-17 as the v0 scope. Provide
  `assure scope --explain` so every included/excluded file has a reason.
- Native local link targets and explicit paths are deterministic structural checks.
- Bare names, probable-broken paths, historical co-change, and inferred symbols are advisory
  candidates until promoted.
- Full Git history is an optional enrichment for “previously resolved” and rename suggestions. It
  is never required for the core and never silently assumed.
- Scanner v0 has no `init` command. Adoption debt is prepared and authorized outside the candidate
  repository, then supplied as an immutable input; the scanner writes nothing.
- A first run can find broken explicit references and formal validator failures. It can report
  unattested or historically suspicious prose. It cannot honestly call existing prose stale.

Every summary and machine result must include at least: documents discovered, documents scanned,
excluded documents, explicit structural references, inferred candidates, governed claims,
unattested claims, unsupported/error counts, waivers, and blocking findings. The success sentence
is “no blocking findings in the evaluated scope,” never “docs are fresh/in sync.”

Coverage gates require a declared denominator, such as an owned inventory of public commands or
required reference pages. “At least one edge per page” is gameable and must remain an advisory
metric. Deleting a governed or inventory-required page is a lifecycle/coverage finding.
Agent-readable exports must carry evidence kind, scope, and attestation trust level; they must not
instruct an agent to treat attested prose as true.

### P0-14: Selector migration and engine upgrades need separate causes

The dossier simultaneously says a path is part of `file-content` identity and a rename requests
review, while refresh silently migrates unique exact-hash moves. Resolve it as follows:

- Every selector has immutable `SelectorId`, versioned locator, projection kind, and a declaration
  of whether path is semantic identity or merely location.
- A path-naming `reference` treats rename as breakage until the document changes.
- A path-insensitive symbol selector may follow a proven unique relocation, but records a migration
  event; similarity only proposes.
- Exact hash is evidence for a candidate, not universal permission to retarget.
- Selector-engine upgrades emit `engine-migration-required`, not “documentation changed.” A
  disposable migration analysis may dual-run old and new engines and propose a transition, but no
  automatic process changes governed state. The explicit migration must record the engine change
  and exact before/after resolution.

## P1: required before any blocking beta

### P1-01: Ownership and reviewer identity are provider capabilities

Each blocking governed claim needs an accountable owner and approval rule, but neither the one-line
declaration nor the current ledger schema contains a trustworthy owner. Define owner resolution from
protected policy/CODEOWNERS and keep document owner distinct from evidence owner. The CLI can report
required approvers; only provider integration can prove an eligible reviewer approved. Ordinary
offline mode remains review-by-repository-policy, not verified human identity.

### P1-02: Adoption debt needs a first-class ratchet

“Pre-existing findings fail the default branch” leaves adopters permanently red. A separate
adoption process must classify each eligible current structural finding as fixed now, external debt
with owner and expiry, or excluded under protected scope policy. A debt entry is keyed to the exact
finding and accepted fact digest; an unequal candidate fact is not covered. Pull requests fail on
every candidate structural failure without that exact debt match and on invalid or expired debt.
No initialization command writes repository state or mass-attests prose merely to make a dashboard
green.

### P1-03: Privileged automation exceeds the initial trust model

The example Action uses mutable `@v1`/`@v4` tags despite the security section requiring full-SHA
pinning. The comment-command workflow parses attacker-controlled trees with a write token, and the
historically proposed refresh App could bypass branch protection.

The blocking beta should contain only a read-only, networkless PR check and local acceptance updates
reviewed in the PR. Pin the action and binary by full digest. Defer issue-comment writes and accept
buttons until parsing is resource-isolated and the service computes an attestation without executing
repository code. Never execute examples, MDX, named shell, or generators in that privileged lane.

### P1-04: Validator provenance overclaims complete derivation

A `generated-from` relation cannot know that authors declared every input unless the validator
sandboxes filesystem/network access. Without that enforcement it proves only reproducibility from
the observed invocation, not complete provenance. Validator descriptors need executable digest,
declared inputs, environment digest, network/secrets policy, timeout/resource limits, and a cost
class. Regeneration and probes belong in unprivileged jobs. Use “declared-input reproducibility”
unless sandboxing proves undeclared inputs were inaccessible.

### P1-05: Error, timeout, unsupported, and skipped are not interchangeable

Define stable process exit codes and policy before CI adoption: success with no blocking findings,
blocking finding, configuration/analysis error, and optionally partial evaluation. Parser crash,
resource limit, unsupported selector, skipped validator, expired evidence, and absent submodule must
remain distinct in JSON and human output. A protected surface may fail closed on analysis error;
unprotected discovery mode may warn. Neither may report the relation clean.

Set and test hard limits for file size, parser work, glob cardinality, regex complexity, include
depth, graph fan-out, report size, and total runtime. The proposed `p95 <= 30 s` is a pilot target,
not an implementation guarantee; benchmark before making the complete-repository pass required.

## P2: safe to defer explicitly

| Deferred feature | Concrete initial resolution |
|---|---|
| Cross-repository relationships | Return `unsupported-scope`; do not fetch from the PR job |
| Deployed-environment and external semantic claims | Scheduled advisory observations only; no PR gate |
| TTL-based external checks | Keep out of the pure v0 core; specify trusted-clock and snapshot semantics first |
| Executable transcripts, browser workflows, and service probes | Wrap later as unprivileged evidence lanes with environment fingerprints |
| LLM discovery or semantic judgment | Offline/scheduled advisory only after calibration; never affects v0 exit status |
| Similarity/refactoring-aware automatic migration | Suggestions only; explicit `assure migrate` remains authoritative |
| Monorepo lock sharding | Benchmark one canonical logical JSONL ledger first; shard without changing IDs or CAS semantics |
| App UI, SARIF polish, editor extension, and agent ranking | JSON evidence contract first; no consumer may translate attestation into truth |

Historical-at-revision link checking, translated-tree lag, bitmap-diagram interpretation, and
tamper-evident external audit storage also fit P2. They should remain named unsupported capabilities,
not partially evaluated states that happen to look green.

## Minimum safe implementation slice

The following is smaller than the current day-zero pitch but preserves its valuable wedge without
locking in unsafe semantics.

1. **Read-only scanner.** Scan the explicit OP-17 document set, resolve native same-repository links
   and path references, emit advisory inferred candidates, and explain scope.
2. **No initialization writes.** Adoption debt, when needed, is an externally authorized immutable
   input keyed to exact finding facts; the scanner has no state-initialization command.
3. **Pure comparison.** `assure check` recomputes candidate observations and validates every
   base-to-candidate lock/config/declaration transition without writing.
4. **Stable governed claims.** Add one versioned unique directive and only the `describes` relation.
   `assure accept` explicitly records both sides, reason, predecessor, scope, and self-asserted trust
   level. Standardize on `accept`; do not alternate among `ok`, `link`, and `accept` in the public CLI.
5. **Conservative enforcement.** Hard-fail every candidate broken native link unless its exact
   finding fact matches externally registered adoption debt; also fail malformed declarations,
   invalid state transitions, and deterministic configured checks. Report observed target changes.
   Do not hard-gate narrative impact until ownership and merge-candidate semantics are live.
6. **Final-tree CI.** Run on pull request and merge-group candidate SHAs with read-only permissions,
   provider-supplied immutable base and candidate IDs, no network, no repository code execution,
   and full-SHA-pinned dependencies.
7. **No post-merge writer.** There is no refresh actor or automatic state transition; governed
   state changes only through explicit reviewed acceptance or lifecycle transactions.

This slice still catches the two known hard-broken references without authored claims, establishes
the observation mechanism, and supplies the data needed to measure irrelevant-trigger rates. The
typed deterministic checks needed to catch user zero's seven semantic calibration drifts can then
arrive one at a time without pretending the zero-touch pass found them.

## Required adversarial tests before the schema is declared stable

| Scenario | Required outcome |
|---|---|
| Target changes, contributor fixes a typo elsewhere in the claim | Existing governed claim remains `review-required` |
| New inferred block appears | It is observed and unattested, never `attestation-current` |
| Contributor hand-edits a digest to the wrong value | Transition verification fails |
| Contributor runs a structurally valid local acceptance | Record is current but trust is `self-asserted`; review policy remains the control |
| Contributor deletes a failing governed declaration or document | `governed-claim-removed` or coverage reduction fails under protected policy |
| Exact-content target moves to a new path | Migration candidate appears; no silent governed retarget |
| One claim splits into two | Successors retain an explicit predecessor mapping and old ID tombstone |
| Two PRs accept the same predecessor | Second candidate fails compare-and-swap after queue/rebase |
| An earlier queued PR changes a dependency | Final candidate invalidates the later stale attestation and reruns review |
| Candidate downgrades policy or adds exclusion | Unsuppressible policy/coverage meta-finding is emitted |
| Candidate marks live docs historical | Scope-policy weakening is emitted |
| Selector engine changes with identical resolution | Engine migration is recorded without claiming a doc edit |
| Selector engine changes resolution | `engine-migration-required` or review is emitted; no auto-baseline |
| Versioned page has no scope mapping | `scope-unresolved`; it is not compared with current main |
| Scanner lacks parser/support or hits a limit | `unsupported`/`error`, never clean |
| Repository has zero governed claims | Summary says zero governed coverage, not “everything in sync” |
| Scanner sees an orphaned governed claim | It reports an orphan candidate and leaves all governed state untouched |
| Lock update tries to name its own enclosing commit | Schema rejects the field as validity input |

## Product-language corrections required now

These are documentation fixes for the eventual normative spec, not cosmetic wording:

- Replace “fresh by construction” with `new/unattested` or `newly observed`.
- Replace “editing clears the flag” with “editing changes the subject; explicit acceptance clears a
  governed review obligation.”
- Replace “every code reference” with the exact supported reference classes and scanned denominator.
- Replace first-run “stale” with `historically suspicious`, `unattested`, or
  `changed-since-observation`, depending on actual evidence.
- Replace “audit trail of who attested” with “unproven self-asserted record” for local acceptance;
  only authenticated provider or signed-service evidence can establish reviewer identity.
- Replace a generic green/fresh badge with “no blocking findings in evaluated scope,” accompanied by
  coverage, unsupported, exclusion, waiver, and trust counts.
- Do not tell agents to prefer attested prose as though attestation established truth. Export the
  evidence class and let consumers make an explicit policy decision.

With these resolutions, the system remains valuable and differentiated. It just stops deriving
assurance from identity churn, proposed bot refreshes, and the absence of declared edges—the three places
where the current design is most likely to manufacture confidence rather than evidence.
