# Documentation-assurance CI: investigation and implementation handoff

Date: 2026-07-12.

This folder is the complete investigation package for a CI system that connects documentation to
selected repository evidence and detects structural breakage or change impact. It contains market
research, source attribution, repository measurements, adversarial review, normative contracts,
wire schemas, and explicit go/no-go gates. No checker or production workflow has been installed.

## Current verdict

The problem is real; the original stateful solution was unsafe; one narrow experiment is specified,
but only its scaffold and conformance harness are ready to implement.

Build the CLI/schema/Git-acquisition scaffold and the complete parser-profile conformance corpus
first. Parser integration and the read-only base-versus-candidate evaluator begin only after that
corpus closes Gate A. Do not yet build a committed ledger, governed claim writer, automatic
refresh, narrative acceptance gate, provider service, or standalone commercial product.

The key correction is epistemic and operational: a hash can prove selected evidence changed; it
cannot prove prose became false or that an editor reviewed it. The product model therefore keeps
structural resolution, impact observation, deterministic verification, narrative acceptance,
trust, lifecycle, coverage, and policy as separate facts.

Product naming is now a pre-E0 gate. [naming-clearance.md](./naming-clearance.md) rejects the
provisional `Assure` identity and finds no screened replacement safe enough to freeze. `DocWake`
passed the technical namespace checks but failed the final legal knockout against active
`DOCSWAVE`. A new coined mark, professional clearance, and an atomic dossier namespace migration
are required before code. A screened candidate slate for that decision, forty-nine names across
six waves, is in [naming-candidates.md](./naming-candidates.md); after the owner's shift to a
modern naming register, the standing finalists are Klopt (the campaign's only clean verdict),
Amiss, and Tarkka, with Trueup conditional and the Latin-register survivors benched by taste.

The complete decision and every unresolved gate are in
[implementation-readiness.md](./implementation-readiness.md) and
[issue-closure-matrix.md](./issue-closure-matrix.md).

## What the evidence changed

User-zero experiments produced these concrete results:

- conservative structural discovery finds 109 tracked Markdown/MDX documents; a broader rule adds
  eleven templates, prompt programs, golden outputs, and a scalar file that are not one coherent
  documentation class;
- 55 same-repository GitHub links contain exactly two broken targets; the other measured explicit
  local/fence references resolve under their actual adapter semantics;
- only 5 of 16 missing repository-rooted inline occurrences were actionable, and none of a
  deterministic 20-item ambiguous sample was actionable;
- all five replayed structural cases broke when their target disappeared, so exact
  base-versus-candidate checking could have caught them;
- three of those cases later had their containing block edited while remaining broken, directly
  rejecting block-level trust-on-edit;
- the current surviving reference graph projects 773 target-impact events over 393 first-parent
  commits, 735 without a document-file co-change; that is workload, not 735 defects;
- a single sorted JSONL state file conflicted in 0%, 18%, and 99% of semantically disjoint trials
  with 1, 5, and 20 updates per branch; per-claim files avoid that particular conflict but still
  require filesystem/scale testing;
- the disposable Node scan ran at 4.875 seconds local p95, about 176 MiB maximum RSS, and 1.95 MiB
  verbose JSON—adequate for calibration, not a production promise;
- the repository has no `merge_group` workflow, so final merge-queue behavior is untested.

Methods, limitations, scripts, and raw data are in
[preimpl-experiments.md](./preimpl-experiments.md) and
[experiments/](./experiments/README.md).

## Frozen scanner v0

The target experiment below is frozen, but current authorization is staged: only its
CLI/schema/Git-acquisition scaffold and conformance harness may be implemented before the complete
parser-profile corpus is checked in. Parser integration and evaluator work follow that Gate-A
closure. The target scanner:

- reads exact commit or staged-index snapshots without writing; worktree overlay is explicitly
  blocked pending a separate filesystem-semantics RFC;
- discovers a conservative Markdown/MDX/Markdown-named set and reports every denominator;
- structurally resolves native Markdown links and same-repository GitHub links;
- compares raw target bytes/mode and containing source-block projections across base/candidate;
- reports raw change impact as advisory, never as semantic falsity or review;
- treats raw HTML, heading anchors, site routes, fence semantics, inline paths, symbols,
  similarity, foreign repositories, and live URLs as explicit boundaries, not guessed successes;
- performs no source evaluation, repository process execution, network access, secret access, or
  workspace write;
- emits strict deterministic JSON/human projections and fails closed on incomplete analysis.

Its contracts are:

- [scanner-v0-spec.md](./scanner-v0-spec.md): discovery, extraction, resolution, correlation,
  findings, output, and tests;
- [machine-contracts.md](./machine-contracts.md): strict report, repository policy, organization
  floor, debt, waiver, digest, ordering, and time inputs;
- [ci-security-spec.md](./ci-security-spec.md): exact Git/provider snapshots, policy composition,
  threats, object kinds, resource ceilings, supply chain, and operations.

## Future governed model

The design work does settle the dangerous compatibility questions, without authorizing their
implementation:

- governed identity is an explicit stable `ClaimId`; content-derived `ObservationId` is diagnostic;
- RFC A-001 has two complete simple forms: structural `reference/path-exists` and narrative
  `describes/file-content`; named checks and complex relations are explicitly unsupported;
- subject/dependency snapshots are independent and one acceptance seal is atomic;
- a narrative subject projection change requests review; a raw-only representation change remains
  a forensic fact, and neither kind of edit attests itself;
- local acceptance is self-asserted and report-only; a blocking narrative requires provider-
  verified eligible review;
- claim creation, move/retarget migration, two-stage retirement, split, merge, engine migration,
  tombstones, and both CAS predecessors have explicit transitions;
- SHA-256 plus restricted RFC 8785/JCS and domain separation define identity/seals;
- no global writable lock or automatic refresh exists; per-claim files are only the X-06 physical
  test candidate until state gates pass;
- candidate policy/coverage/claim/control-plane weakening produces unsuppressible findings.

See [normative-core-spec.md](./normative-core-spec.md) and
[directive-rfc.md](./directive-rfc.md). Their adapters, state writer, and public compatibility are
closed by Gate B/C and experiments X-03/X-06/X-08.

## Market correction

The original “empty quadrant” claim is false. Fiberplane `drift` already implements explicit
Markdown-to-code anchors, normalized signatures, committed state, CI failure, and reverse lookup;
`ryanwaits/drift` implements a broad TypeScript documentation-rule suite; Swimm occupies the
enterprise code-coupled documentation category.

The possible differentiation is narrower: typed cross-artifact relations, deterministic evidence
adapters, honest observation/acceptance/trust separation, stable lifecycle, bypass-resistant
policy, final-candidate CAS, coverage-safe output, and provider-verifiable review. Those remain
hypotheses.

[market-reassessment.md](./market-reassessment.md) therefore requires a build-vs-extend comparison,
external design partners, and willingness-to-maintain/pay evidence before a standalone product.
Professional patent status/family/FTO review is an external gate before a public commercial pilot.
Counsel must retrieve the official file and maintenance history; check continuations, foreign
family, assignment, expiration, and term adjustment; chart the claims against the proposed
workflows; and record the product decision and any design-around outside this dossier. This dossier
makes no legal conclusion.

## Authoritative reading order

1. [implementation-readiness.md](./implementation-readiness.md): what may be built now and the
   phase entry/exit rules.
2. [issue-closure-matrix.md](./issue-closure-matrix.md): every P0/P1/P2, C decision, consistency
   hole, final FCA machine-contract finding, Gate A–D item, X experiment, red-team scenario,
   invariant family, and old OP disposition.
3. [preimpl-experiments.md](./preimpl-experiments.md): measured results, limitations, raw artifacts,
   and X-01 through X-08 pass/fail gates.
4. [scanner-v0-spec.md](./scanner-v0-spec.md), [machine-contracts.md](./machine-contracts.md), and
   [ci-security-spec.md](./ci-security-spec.md): the implementation contract for the experiment.
5. [normative-core-spec.md](./normative-core-spec.md) and
   [directive-rfc.md](./directive-rfc.md): future governed semantics, currently disabled.
6. [market-reassessment.md](./market-reassessment.md): competitive/build/legal correction.

Supporting analysis remains useful as rationale:

- [pre-implementation-review.md](./pre-implementation-review.md) and
  [preimpl-red-team.md](./preimpl-red-team.md): the original synthesis and blockers;
- [implementation-feasibility.md](./implementation-feasibility.md) and
  [v0-contract-review.md](./v0-contract-review.md): repository/runtime and rejected candidate
  contracts;
- [repo-audit.md](./repo-audit.md), [use-cases.md](./use-cases.md), and
  [edge-cases.md](./edge-cases.md): user-zero evidence generalized into requirements;
- [design.md](./design.md), [failure-modes.md](./failure-modes.md), and
  [open-problems.md](./open-problems.md): product rationale and historical alternatives;
- [prior-art.md](./prior-art.md), [market.md](./market.md), and
  [comparison.md](./comparison.md): mechanisms, competitors, and trade-offs;
- [sources.md](./sources.md): annotated source ledger.

## Next action

Translate the frozen scanner contract into hostile fixtures, the complete parser-profile corpus,
and a conformance harness alongside the CLI/schema/Git scaffold. After those goldens pass, build the
read-only evaluator and run it report-only on user zero, then use it to close the scanner portions
of X-02, X-04, and X-05. Do not
install a required workflow. Only after external shadow evidence justifies E3 should a separate
provider request-wire plus control-epoch/provider-freshness RFC be written and X-07 run against an
active ruleset workflow pinned to an immutable source commit/blob and dependency closure. Let those
results earn, narrow, or kill required enforcement and every later stateful layer.
