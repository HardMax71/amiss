# Market and build-vs-extend reassessment

Date: 2026-07-11.

Status: corrective supplement to [market.md](./market.md) and
[comparison.md](./comparison.md). Product and repository facts are current to the access date;
vendor performance claims are not independent validation.

## Decision

Do not begin by building a generic stateful “documentation drift” product. Begin with the
repository-local, discard-state experiment specified in
[pre-implementation-review.md](./pre-implementation-review.md), and run a build-vs-extend
evaluation against the two active OSS projects below before creating a standalone product
codebase.

The original dossier's “empty quadrant” claim is no longer supportable. The market now contains:

- an active language-independent anchor/fingerprint/lock/CI tool that is extremely close to the
  proposed stateful core;
- an active TypeScript documentation-drift tool with structural and example checks, coverage
  ratchets, machine output, and CI integration;
- an active enterprise vendor that continues to sell code-coupled documentation, automatic
  updates, generation, CI blocking, and on-premises deployment.

The possible differentiation is narrower and harder: typed cross-artifact claims, separation of
observation/verification/attestation, stable governed lifecycle, policy-bypass resistance,
coverage-safe output, and provider-verifiable review. Those are hypotheses, not demonstrated moat.

## Current direct alternatives

| Project | Current evidence | What it already covers | Material limitation or difference |
| --- | --- | --- | --- |
| Fiberplane `drift` | MIT, v0.10.1 published 2026-06-22, 119 GitHub stars on 2026-07-11 | Markdown anchors to files or AST symbols, tree-sitter normalization, `drift.lock`, reverse lookup, CI failure, inline references, cross-origin skip metadata | Explicit anchors only; one untyped binding model; its authors explicitly say relinking can be done without reviewing prose; governed deletion, trust, policy weakening, and typed validators are not its stated contract |
| `ryanwaits/drift` | MIT, website labels v0.42.0; 423 repository commits and three stars on 2026-07-11 | Fifteen TypeScript/JSDoc/Markdown drift rules, API extraction, example validation, coverage ratchet, JSON, agent workflow, monorepo and GitHub Action surfaces | TypeScript API-documentation focus; marketing says it finds every wrong doc, a stronger claim than the exposed rules establish; not a general narrative-attestation protocol |
| Swimm | Active enterprise pages crawled in July 2026 | Code-coupled docs, CI alert/block, automatic synchronization and generation, IDE/platform surface, 40+ claimed languages, cloud/on-premises/air-gapped options | Proprietary platform and vendor-authored formats/workflow; vendor assurance and scale claims require customer validation; still proves the commercial category is occupied |

Primary sources:

- Fiberplane's [March 2026 announcement](https://fiberplane.com/blog/drift-documentation-linter/)
  explains anchors, AST fingerprints, provenance, CI, and the fact that the review itself remains
  a human assumption. The current repository README was also inspected through the GitHub API.
- Fiberplane's May 2026 engineering note reports that randomized merge testing reduced its
  lockfile's spurious conflict rate from about 44% to about 25% after changing serialization; see
  the [Fiberplane blog index](https://blog.fiberplane.com/blog/). This is vendor-reported but
  directly relevant evidence that a committed binding ledger remains a conflict surface even
  after format work.
- The second project's [repository](https://github.com/ryanwaits/drift) and
  [product site](https://www.driftdev.sh/) describe its current commands and rules.
- Swimm's current [enterprise documentation page](https://swimm.io/enterprise-documentation-platform)
  still advertises automatic documentation generation plus merge alert/block behavior. These are
  vendor claims.

## Consequences for the design

### The zero-config scanner is acquisition, not moat

Same-repository link resolution, path extraction, changed-file heuristics, and source-positioned
JSON are useful but straightforward. They should be fast, safe, and open. They are not a defensible
reason to create a new platform.

The scanner's job is to produce evidence for three decisions:

1. Which relationship classes can be inferred precisely enough to save setup work?
2. Which code-change impacts are actionable enough to request review?
3. Which repository-specific deterministic checks deserve promotion into reusable adapters?

If it cannot answer those questions better than a small repository script, stop at the script.

### The stateful core already has a close OSS implementation

Fiberplane `drift` now implements the central mechanical loop proposed by the second-pass dossier:
explicit bindings, normalized content signatures, a committed lock, CI failure, relinking, and a
reverse index. Reimplementing that loop is justified only if at least one of these proves necessary
and cannot be added upstream or wrapped cleanly:

- typed relation semantics beyond “this doc binds to this code”;
- formal validators over values, sets, trees, graphs, generation, or equivalence;
- immutable governed claim identity and audited lifecycle;
- observation/attestation/verification/trust fact separation;
- final-tree compare-and-swap acceptance;
- unsuppressible policy, coverage, claim-removal, and migration findings;
- provider-verified reviewer evidence or a service-signed event log;
- format-neutral declarations with stable machine interoperability.

Until a pilot needs one of those capabilities, the stateful work is speculative infrastructure.

### Lockfile conflict is a product cost, not a serialization bug

The reported Fiberplane experiment is especially important. Even after testing eleven formats and
switching to the best reported candidate, its simulated spurious conflict rate remained roughly
one in four. The exact harness and workload need independent reproduction before applying the
number to another schema, but the direction is decisive: line orientation, JSONL, or TOML does not
eliminate concurrent edits to a shared committed state surface.

This strengthens three decisions:

- use a stateless base-versus-candidate scanner before a ledger;
- if a ledger is introduced, partition logical ownership without changing IDs or transaction
  semantics, and measure real conflict workload before choosing physical sharding;
- never add a post-merge bot merely to keep advisory observations current unless carrying those
  observations across merges has measured value greater than its commit and conflict burden.

## Build-vs-extend evaluation

Before a standalone product repository is authorized, evaluate the same frozen corpus with:

1. the repository-local discard-state scanner;
2. Fiberplane `drift` using explicit anchors on a representative subset;
3. `ryanwaits/drift` on a representative TypeScript repository, not on this Scala corpus;
4. existing deterministic mechanisms such as the current link checker, OpenAPI equality, and
   executable snippets.

Score mechanisms separately; do not blend them into one “accuracy” number:

| Dimension | Required evidence |
| --- | --- |
| First value | Setup time, authored lines, dependencies, and first confirmed defects |
| Structural precision | Maintainer labels for explicit missing targets and malformed selectors |
| Impact actionability | Whether the requested review was worth doing, distinct from whether prose was actually false |
| Escaped drift | Seeded and naturally observed defects missed within the tool's claimed coverage |
| Maintenance | Anchor/declaration churn, migration ambiguity, lock diffs, conflicts, and upgrade work |
| CI behavior | Cold/warm time, peak memory, output volume, error behavior, forks, and merge groups |
| Governance | Deletion, retarget, policy weakening, rubber-stamp, concurrent acceptance, and reviewer trust behavior |
| Portability | Markdown/MDX and language coverage actually exercised, not advertised |

Decision rule:

- extend or wrap an existing project when it meets the required semantics without creating a
  permanently divergent state model;
- build a separate core only when the pilot demonstrates a high-value requirement that would be a
  breaking architectural change upstream;
- stop at repository-specific deterministic checks when neither a generic inference product nor a
  governed ledger improves outcomes enough to justify maintenance.

This is a technical and product decision, not a preference for greenfield ownership.

## Demand evidence is not willingness-to-pay evidence

The existing surveys show that stale and inconsistent documentation is common and costly. They do
not establish that teams will maintain explicit claim IDs, review impact findings, operate a
ledger, or buy a separate assurance product. The current dossier also draws conclusions from one
unusually documentation-heavy repository selected because it contains drift; that is a valuable
requirements corpus and a biased market sample.

Keep these hypotheses separate:

| Hypothesis | Minimum falsifiable test |
| --- | --- |
| Explicit structural checking creates immediate value | Unaffiliated repositories confirm defects that their existing checks missed and retain the gate |
| Inferred impact saves review time | Shadow findings meet pre-registered actionability and triage-cost thresholds by reference class |
| Teams accept governed-claim ceremony | Multiple contributors create, migrate, accept, and retire claims without a researcher doing the maintenance for them |
| Durable obligations need a committed ledger | X-08 records whether pilot users choose, service, and retain carried review obligations over stateless per-change enforcement despite measured conflict/commit cost and within pre-registered burden budgets |
| Agent-instruction files are a strong wedge | X-02 shadow teams or design partners record concrete stale-instruction incidents or behavior degradation, evaluate the resulting findings, and retain the enabled check afterward |
| A buyer exists | A platform/DX owner agrees to a paid pilot or signs a design-partner commitment tied to measured outcomes |
| The product is differentiated | Users choose its governed evidence model after trying the relevant OSS alternatives |

A June 2026 research preprint on agent configuration artifacts reports stale code-element
references in 23% of a statistically representative sample of 356 repositories; see
[Treude and Baltes, “Context Rot in AI-Assisted Software Development”](https://arxiv.org/abs/2606.09090).
That strengthens the problem hypothesis, not the willingness-to-maintain or willingness-to-pay
hypotheses.

These routes are not interchangeable. X-02/design-partner evidence tests the agent-instruction
incident and retained-check hypothesis without authorizing governed state. Only the later X-08
user-behavior study can test whether teams actually prefer and service durable carried obligations;
synthetic serializer results or stated enthusiasm do not satisfy that hypothesis.

## Positioning that remains defensible

Do not say:

- “nobody does this”;
- “all existing docs, automatically kept up to date”;
- “attested prose is reliable for agents”;
- “audit-grade” without provider-verified or signed events;
- “any text file” when governed parsing is supported only for named adapters;
- “the lockfile is machine-owned” as though that authenticates decisions.

The defensible description is:

> A repository-local evidence protocol that resolves explicit documentation references, reports
> code-change impact, runs narrow deterministic consistency checks, and—where teams opt in—records
> governed review decisions without presenting them as proof that prose is true.

This is less magical than the original pitch and more difficult to misuse.

## Commercial and legal gate

The relevant U.S. publication describes stored links between requirements-management and
configuration-management objects, automatic suspect marking after changes, and user-driven
clearing. See [application 20080059977](https://patents.justia.com/patent/20080059977). The USPTO
requires current maintenance/status lookup through its current systems and explains that utility
patent maintenance fees are due at 3.5, 7.5, and 11.5 years; see
[Maintain your patent](https://www.uspto.gov/patents/maintain).

No engineering document can resolve claim construction, family members, assignment, maintenance,
term adjustment, terminal disclaimer, jurisdiction, validity, or infringement. Before any public
commercial pilot of change-impact/suspect-link behavior:

1. counsel retrieves the official current file and complete maintenance history;
2. counsel charts the independent claims against both stateless and stateful proposed workflows;
3. counsel checks continuations, foreign family, assignment, expiration, and term adjustment;
4. the resulting product decision and any design-around are recorded outside this technical
   dossier.

Until then, legal status is `external-gate-open`. Internal read-only research may continue, but no
document here calls it legally safe.

## Market readiness conclusion

The problem is real. The general mechanic is occupied. The differentiated protocol is plausible
and unvalidated. The correct next investment is evidence:

- compare rather than assume superiority;
- measure user behavior rather than count generated relationships;
- prove a need for persistent state before accepting its operational cost;
- obtain an actual design partner before treating survey demand as a market;
- complete professional legal review before commercialization.

This changes “build the standalone tool” into “earn the right to build the governed layer.”
