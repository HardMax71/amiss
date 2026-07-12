# Failure modes of a documentation-to-code drift gate

Status (2026-07-11): enduring risk analysis, not the executable contract. Concrete mitigations,
rejections, and still-closed gates are tracked in
[issue-closure-matrix.md](./issue-closure-matrix.md) and
[implementation-readiness.md](./implementation-readiness.md).

## Bottom line

The useful version of this idea is a **change-impact and re-attestation system**, not a general
documentation-correctness oracle.

A hash, timestamp, or commit ID can prove that an artifact changed after somebody last reviewed a
relationship. It cannot prove that the change invalidated the prose. Conversely, an unchanged
artifact cannot prove that the prose is correct. The documentation may have been wrong at its
baseline, an indirect dependency may have changed, or production behavior may have changed through
configuration, data, permissions, a feature flag, or an external service.

Therefore, a gate that fails whenever the raw count of changed pairs is greater than zero will be
noisy and easy to game. A defensible gate has three outcomes:

1. **Block on mechanically proved violations**: a referenced symbol disappeared, a local target is
   missing, a generated artifact does not reproduce, an executable example fails, or a declared
   schema/contract rule is broken.
2. **Require an explicit impact decision** when a relevant target changed: update the document or
   attest that it remains correct, with an owner and reviewable reason.
3. **Warn on heuristic suspicion**: text similarity, inferred links, transitive impact guesses, and
   LLM judgments should not be hard blockers.

This distinction is not academic. A study of more than 3,000 projects deliberately used a narrow
definition: code-element text that remained in documentation after all matching code instances were
deleted. Even then, some projects took more than a day to analyze, and maintainers classified four
of the eight responses to reported cases as false positives. The authors also found cases where the
literal identifier disappeared while the functionality remained
([Tan, Wagner, and Treude](https://arxiv.org/abs/2212.01479)). FreshDoc, another deliberately narrow
Java API-name detector, reported an F-measure of 60%, despite outperforming earlier approaches
([Lee et al.](https://s-cube-xmu.github.io/uploads/Automatic%20Detection%20and%20Update%20Suggestion%20for%20Outdated%20API%20Names%20in%20Documentation.pdf)).

## What the checker can and cannot know

Let an edge say that document unit `D` depends on code artifact `C`, and let `B` be the version of
`C` last reviewed for `D`.

| Observation at the proposed head | Mechanically supportable conclusion | Unsupported conclusion |
|---|---|---|
| `hash(C_head) == hash(C_B)` | The selected bytes did not change | The document is true or complete |
| `hash(C_head) != hash(C_B)` | The selected bytes changed | The document is now false |
| Target path or symbol no longer resolves | The reference is structurally broken | The documented capability no longer exists |
| Public signature changed | A declared interface changed syntactically | Existing users break, or this prose needs editing |
| Snippet or command fails in a pinned environment | That executable claim fails in that environment | The surrounding explanation is globally wrong |
| Document and baseline marker changed together | Someone changed both files | Someone actually reviewed or corrected the claim |
| No edge exists | The checker has nothing to evaluate | No relationship or drift exists |

There are consequently three different properties that should not share the word "drift":

- **Structural drift**: an endpoint cannot be found or an edge is malformed. This is decidable and
  can usually block.
- **Validation drift**: an executable or otherwise formalized claim no longer passes. This can block
  within the validator's stated scope.
- **Review drift**: a dependency changed after the last human attestation. This is an invalidated
  review, not proof of incorrect documentation.

Natural-language documentation also expresses rationale, intent, safety constraints, historical
decisions, tradeoffs, and operational practice. Those claims may not be derivable from the current
code at all. A large empirical taxonomy found 162 kinds of documentation issues across information,
presentation, process, and tooling; up-to-dateness is only part of the quality problem
([Aghajani et al.](https://csnagy.github.io/research/pdfs/2019/Aghajani2019-preprint.pdf)). A drift
gate can reduce one class of documentation debt while doing nothing about missing, ambiguous,
misleading, badly scoped, or internally contradictory content.

## The graph has its own decay problem

The proposal moves part of the documentation-maintenance problem into a traceability graph. That
graph is useful, but it becomes another maintained artifact.

- **Missing edges are silent false negatives.** If zero connections is valid, the least maintained
  documents can remain completely invisible. An all-green result says only that declared edges
  passed, not that the declared edge set is complete.
- **Wrong edges create permanent noise.** A broad link from an overview page to a package or whole
  repository makes every internal refactor look relevant.
- **Narrow edges miss transitive behavior.** A page linked only to a public method will not notice a
  behavior change in a helper, dependency, build flag, schema, policy, or deployment manifest.
- **Edge endpoints decay.** Paths, line ranges, symbol names, repository locations, ownership, and
  version scope all evolve.
- **Duplicate bidirectional records disagree.** Keeping one `docs -> code` record and another
  `code -> docs` record doubles the maintenance surface without adding truth.
- **Inferred links have uncertain completeness and precision.** Search, embeddings, co-change, and
  LLMs are useful for proposing candidates, but a proposed relationship is not an authoritative
  dependency.

This cost is well established in traceability research. A mapping study of 63 studies found change
management to be the most common benefit, while establishing and maintaining links was the main
cost; link quality and performance remained major challenges
([Tian et al.](https://arxiv.org/abs/2108.02133)). A review and evaluation of continuous
requirements-to-code traceability likewise treats maintenance, precision, recall, and human vetting
as first-class problems
([Hübner and Paech](https://link.springer.com/article/10.1007/s10664-020-09831-w)).

The safest topology is one **canonical, typed edge stored once and indexed from both endpoints**.
Direction is a property of the relationship, not a reason to duplicate it. These relationships need
different propagation rules:

| Relation | Meaning | Sensible reaction to change |
|---|---|---|
| `describes` | Prose makes a current behavioral claim about code | Invalidate review of the affected claim |
| `example-of` | Snippet demonstrates an API or command | Compile/run the example, then request review if needed |
| `generated-from` | Artifact is a deterministic output of inputs | Regenerate and compare; no human baseline bump |
| `constrained-by` | Code must respect a requirement or policy | Run a formal validator if one exists; otherwise require domain review |
| `rationale-for` | ADR explains why a decision was made | Preserve history; do not rewrite merely because implementation moved |
| `historical-at` | Document intentionally describes a past release | Compare only with the pinned release, not current main |

Treating all of these as an untyped "pair" is a foundational design error.

## Identity and freshness mechanisms under stress

| Mechanism | What it does well | Failure modes |
|---|---|---|
| Filesystem modification time | Cheap in one working tree | Not stable across clone, checkout, archive, generated output, or CI; says nothing about Git history or semantics |
| Git author/committer time | Human-readable chronology | Dates are user-settable through `GIT_AUTHOR_DATE` and `GIT_COMMITTER_DATE`; rebases, cherry-picks, squashes, merges, and clock skew make "newer than" unreliable ([Git commit documentation](https://git-scm.com/docs/git-commit.html#_commit_information)) |
| Whole commit SHA | Immutable repository snapshot and good historical permalink | Far too broad for a local claim; any unrelated commit appears newer. Embedding the SHA of the commit containing the marker is self-referential: changing the marker changes that commit's SHA. GitHub permalinks intentionally show an exact old version, not whether it remains applicable ([GitHub permanent links](https://docs.github.com/en/repositories/working-with-files/using-files/getting-permanent-links-to-files)) |
| File/blob content hash | Deterministic, cheap, independent of timestamps | Formatting, comments, license headers, or unrelated functions invalidate it; a move can preserve content while breaking a path; indirect dependencies are invisible |
| Line-range hash | Less noise than whole-file hashing | Insertions and formatting move ranges; relevant logic can move outside the range; ranges cannot model macros, partial classes, or dynamic dispatch reliably |
| Named-region hash | Human-controlled boundary | Region markers themselves drift or get moved to exclude relevant code; language-agnostic but easy to game |
| AST or symbol fingerprint | Can ignore formatting and focus on signatures/bodies | Parser- and language-version specific; overloads, macros, generated code, conditional compilation, reflection, aliases, and symbol moves complicate identity. A signature fingerprint misses behavior; a body fingerprint restores refactor noise |
| Git rename/copy detection | Helps migrate paths during ordinary refactors | It is a similarity heuristic, not recorded identity. Git's default rename threshold is 50%; exhaustive rename/copy detection can be `O(N^2)` and is explicitly described as expensive ([Git diff documentation](https://git-scm.com/docs/git-diff)) |
| API/schema compatibility diff | High-value signal for formal public contracts | Only covers the modeled contract; "compatible" changes can still require examples or explanations, and behavior can change without a signature change |
| Executable documentation | Strong evidence for the exercised examples | Environment setup is costly; examples cover particular paths and may become flaky or non-hermetic; passing examples do not establish prose completeness |
| LLM semantic judgment | Can rank review candidates and explain likely impact | Nondeterminism, model drift, prompt injection from repository text, privacy, cost, and uncalibrated confidence make it unsuitable as a hard gate |

Semantic Versioning makes the same separation at release level: internal bug fixes, backward-compatible
additions, and incompatible public API changes are different categories even though all modify code
([Semantic Versioning 2.0.0](https://semver.org/)). A raw content hash collapses those categories.

## False positives: valid documentation that the gate rejects

Likely sources include:

- whitespace, formatting, comments, imports, logging, metrics, performance work, or internal
  refactoring inside a linked file;
- moving or renaming a semantically identical symbol;
- a bug fix that makes implementation conform to prose that was already correct;
- a new optional argument or additional implementation that does not affect an existing claim;
- a code generator or formatter version changing output without changing the source contract;
- a central utility, configuration file, or schema linked by hundreds of pages, causing a fan-out
  storm from one change;
- an intentionally frozen release guide compared against `main`;
- an ADR, migration guide, release note, incident report, or deprecation history that is supposed to
  retain old names and behavior;
- code for another platform, feature flag, edition, tenant, or release branch changing while the
  documented variant does not;
- a literal identifier disappearing while an alias, wrapper, generated binding, or equivalent
  functionality remains;
- a reverted change whose final behavior again matches the attested document, while a simplistic
  "any commit since baseline" test still reports drift.

The DOCER study gives concrete versions of two of these errors: a removed CMake flag still reflected
relevant user behavior, and a deleted literal remained represented by program logic
([Tan, Wagner, and Treude](https://arxiv.org/abs/2212.01479)).

Metrics must distinguish two labels. "The document became false" measures semantic precision.
"A human review was reasonably warranted" measures impact-review actionability. Redefining every
requested re-attestation as a true positive can make dashboard precision look excellent while the
gate still wastes substantial developer time.

## False negatives: wrong documentation that the gate accepts

Likely sources include:

- no edge was declared, or an edge was declared against an irrelevant/narrow target;
- the document was already wrong when its baseline was recorded;
- a contributor updates only the stored digest or "reviewed" marker;
- a contributor makes a token documentation edit without checking the affected claims;
- the public symbol and signature stay constant while behavior, defaults, errors, ordering,
  concurrency, performance, or side effects change;
- the change occurs in a transitive helper, dependency, database migration, build definition,
  deployment manifest, IAM policy, feature-flag service, environment variable, or external API;
- generated code is in sync with its schema but incompatible with the selected runtime or generator
  version;
- docs live in a wiki, portal, issue tracker, support site, diagram tool, notebook, package registry,
  or another repository that the checkout cannot see;
- the relevant code is loaded dynamically, selected by runtime configuration, generated only in a
  release build, or absent from a sparse/shallow checkout;
- a normalized AST/signature fingerprint deliberately ignores the exact body change that altered
  behavior;
- the documentation has a missing caveat, unsupported guarantee, misleading screenshot, bad
  sequence, or omission, none of which requires a corresponding code delta.

The strongest bypass is also the easiest: change code and mechanically refresh every expected hash.
The gate then proves only that its metadata is current. Baseline changes must therefore be generated
by the tool, shown prominently in review, bound to a specific edge and validator, and protected by
appropriate ownership. Even that proves review occurred only organizationally, not mathematically.

## Organizational and incentive failure modes

### Ritual compliance

When the fastest way through CI is "refresh all markers," teams will optimize for that operation.
The result is green metadata and unchanged prose. The same happens if a meaningless punctuation
edit counts as a documentation update, or if "not affected" requires no reason.

Requirements engineering ran this experiment for two decades and the outcome is consistent:
suspect-link ecosystems exposed bulk absolution. Doorstop's own documentation recommends
`doorstop review all` followed by `doorstop clear all`; Jama and Codebeamer shipped Clear All and
mass processing because users demanded them. Vetting studies add that the behavior is cognitive,
not just lazy: analysts accept uncertain links with far less scrutiny than they apply to
rejections ([Niu et al., FSE 2016](https://homepages.uc.edu/~niunn/papers/FSE16.pdf)). This is a
ritual-compliance warning, not a product requirement. Do not ship bulk acceptance: one explicit
acceptance transaction covers one claim and shows that claim's evidence. Multi-claim changes are
legal only as typed split or merge lifecycle transactions with complete predecessor/successor
mappings; they are not acceptance shortcuts.

Mitigations:

- never accept document mtime or "document changed in this PR" as evidence;
- make the acknowledgement edge-specific and record `updated`, `verified-still-correct`, or
  `not-applicable`, plus a short reason for the latter two;
- show the target diff/fingerprint change next to the affected claim;
- require owner review for high-risk relations and audit baseline-only changes;
- expire suppressions and report repeated blanket acknowledgements by team and edge type.

### Coverage avoidance

If declaring an edge creates future work, rational teams may stop declaring edges. A metric such as
"zero drifted pairs" then rewards an empty graph. Do not turn edge coverage into a universal quota
either: that produces low-quality bulk links. Start with explicit high-risk surfaces (public APIs,
security behavior, operational runbooks, migrations, and user-visible configuration) and measure
their coverage against an owned inventory.

### Ownership mismatch

Code owners may not understand customer docs; technical writers may not be able to judge code;
platform teams may change a shared component used by hundreds of products. Without an owner who can
make the decision, a blocking result merely transfers queue time to the PR author.

Each enforced edge needs one accountable owner, a backup/escalation path, and an SLA. Aggregate
fan-out by owning domain so a single underlying change creates one impact-review task rather than
hundreds of identical failures.

### Removal as an optimization

A gate can make deleting a linked page cheaper than maintaining it. In the DOCER historical data,
deleting documentation was one observed way projects resolved stale references (13.3% of resolved
top-project cases), although the study does not claim those deletions were gaming
([Tan, Wagner, and Treude](https://arxiv.org/abs/2212.01479)). Track removed documentation and require
review when it is part of an owned surface.

### Trust collapse

A deterministic false positive is not technically a flaky test, but its organizational effect is
similar: developers learn that red does not mean a real defect. Google's testing guidance says tests
lose value as engineers lose confidence and reports that even around 1% flakiness is damaging
([Software Engineering at Google, testing overview](https://abseil.io/resources/swe-book/html/ch11.html)).
Its CI guidance is the right rule for this gate: if a signal is not actionable or does not violate
the asserted invariant, it should not be a failure
([Software Engineering at Google, CI](https://abseil.io/resources/swe-book/html/ch23.html)).

## Developer-experience and CI failure modes

- **Opaque output:** "17 pairs drifted" does not tell an author which sentence is at risk, why the
  code change matters, who owns the decision, or how to resolve it.
- **Alert multiplication:** one shared change creates hundreds of annotations, PR comments, and
  reviewer requests. Count affected root causes and owner groups, not raw edges.
- **Late feedback:** a checker available only after remote CI imposes a full round trip for a
  metadata decision. Supply an identical local command and stable machine-readable output.
- **Moving-base surprises:** rebasing or merging the target branch changes the PR comparison and can
  produce new failures unrelated to the author's diff. Stored target digests are more stable than a
  merge-base-only rule.
- **Permanent pending checks:** workflow path-filter mistakes can block merging even when the job did
  not run. GitHub notes that skipped required workflows can remain pending and that changed-file
  filtering considers only the first 300 files
  ([GitHub Actions workflow syntax](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax#onpushpull_requestpull_request_targetpathspaths-ignore)).
  A required lightweight dispatcher should run on every PR and compute affected artifacts itself.
- **Network flakes:** checking live external links, remote repositories, wikis, or an LLM makes CI
  depend on DNS, rate limits, credentials, service availability, and mutable remote state. Keep the
  blocking PR path hermetic; run remote audits separately.
- **Unbounded runtime:** scanning full history, parsing every language, or expanding transitive edges
  can turn every PR into a repository-wide analysis. In the DOCER experiment, eight projects did not
  finish within a day and one Google project was stopped after three days
  ([Tan, Wagner, and Treude](https://arxiv.org/abs/2212.01479)).

## Security and supply-chain failure modes

The checker processes attacker-controlled repository content on pull requests. Treat documents,
front matter, links, filenames, code fences, generated manifests, and graph metadata as untrusted
data.

- Do not execute examples, imported modules, build files, or MDX in a privileged workflow. MDX can
  contain JSX, JavaScript expressions, and ESM imports/exports
  ([MDX documentation](https://mdxjs.com/docs/)). Executable-doc checks for fork PRs need a sandboxed,
  unprivileged job with no secrets and a read-only token.
- Do not combine `pull_request_target`, checkout of fork code, and execution. GitHub documents this
  as a "pwn request" pattern that exposes the base repository token and secrets
  ([GitHub `pull_request_target` security](https://docs.github.com/en/actions/reference/security/securely-using-pull_request_target)).
- Never interpolate document content, paths, branch names, PR text, or link targets into a shell
  script. GitHub explicitly treats these as script-injection inputs
  ([GitHub script-injection guidance](https://docs.github.com/en/actions/concepts/security/script-injections)).
- Do not follow arbitrary repository-supplied URLs from a privileged runner. Besides nondeterminism,
  this creates SSRF and internal metadata/network discovery risk. If a separate link-audit job must
  fetch, enforce scheme/domain/IP allowlists, block private/link-local destinations after DNS
  resolution, disable redirects or revalidate every hop, bound response sizes and timeouts, and run
  without secrets. OWASP recommends allowlisting and warns that redirects and DNS behavior can
  bypass naive URL validation
  ([OWASP SSRF Prevention](https://cheatsheetseries.owasp.org/cheatsheets/Server_Side_Request_Forgery_Prevention_Cheat_Sheet.html)).
- Normalize paths and refuse traversal outside the checkout; do not follow symlinks by default.
- Bound document size, nesting, include depth, regex work, graph fan-out, and total findings to
  resist resource-exhaustion pull requests. Detect include and edge cycles.
- Do not send private documentation or code to an external semantic-analysis service without
  explicit policy and data handling approval. Repository prose can also contain prompt injection.
- Pin third-party Actions to reviewed full commit SHAs and grant the workflow minimum permissions.
  GitHub notes that tags can move, whereas full-SHA pinning runs the reviewed code
  ([GitHub workflow hardening](https://docs.github.com/en/code-security/tutorials/secure-your-organization/protect-against-threats#pin-third-party-actions-to-commit-shas)).
- Key caches by checker version, parser version, configuration, and input digests. Untrusted PR jobs
  should not populate a shared trusted cache.

## Scale, performance, and monorepo failure modes

The incremental happy path can be close to `O(changed artifacts + incident edges)`. Several tempting
features destroy that property:

- rename/copy recovery can require pairwise comparisons; Git documents an `O(N^2)` exhaustive
  fallback for unresolved rename/copy candidates
  ([Git diff documentation](https://git-scm.com/docs/git-diff));
- a high-fan-out node such as a root schema, shared CLI parser, auth policy, or common configuration
  can invalidate thousands of edges;
- transitive dependency expansion can approach the entire build graph;
- polyglot symbol resolution requires many toolchains and parser versions;
- historical comparison needs objects unavailable in shallow clones;
- submodules, Git LFS, generated sources, sparse checkouts, and vendored trees may not contain the
  bytes implied by an edge;
- cross-repository targets introduce authentication, rate limits, mutable branches, and ordering
  problems; there is no atomic commit spanning independent repositories;
- base-branch updates can race with a running check, producing results for a snapshot different from
  the eventual merge.

Mitigate this like a build system: declare inputs, use content-addressed results, make validators
hermetic, and compute only impacted graph components. Bazel's hermeticity guidance explains why
isolated, explicitly versioned inputs enable reproducibility, caching, and parallelism
([Bazel hermeticity](https://bazel.build/concepts/hermeticity)). Run a full graph-integrity scan on a
schedule, but keep the required PR check incremental and bounded. Fail closed only for an analysis
error on a protected, claimed coverage surface; report unsupported artifact types explicitly rather
than silently treating them as clean.

## Versioning and release-line failure modes

"Current code" is not one thing in a maintained product. Documentation can describe:

- an unreleased `next` branch;
- the latest stable release;
- several supported release branches;
- a frozen old release;
- a platform/edition-specific variant;
- a rolling service whose clients remain on older SDKs;
- a migration from one version to another;
- a future or deprecated capability behind a flag.

Docusaurus explicitly distinguishes `current`/`next` from `latest`, preserves copied versioned docs,
and warns that versioning increases build time and contributor complexity
([Docusaurus versioning](https://docusaurus.io/docs/versioning)). Comparing every versioned page with
the main branch would label correct historical documentation as stale.

Every enforced edge therefore needs a scope such as `(product, release line, audience, platform,
feature set)`. Frozen documentation should point to an immutable tag/commit and be checked for target
integrity, not freshness against main. Active release docs should compare with the matching branch or
release artifact. Backports must update only the affected release scopes.

Generated bindings add a second version dimension. Protobuf, for example, documents compatibility
windows between generated code and runtime versions and warns that unsupported skew can appear to
work while hiding serious runtime bugs
([Protobuf cross-version guarantees](https://protobuf.dev/support/cross-version-runtime-guarantee/)).
A source hash alone cannot establish a valid source-generator-runtime tuple.

## Generated code and generated documentation

Generated artifacts should not use the same human acknowledgement protocol as hand-written prose.
The proper invariant is derivation:

1. identify all authoritative inputs, including generator binary/version, configuration, templates,
   dependencies, and relevant environment;
2. regenerate in a hermetic environment;
3. compare deterministic outputs;
4. link human documentation to the highest-level source of truth, such as an OpenAPI/protobuf schema,
   rather than every language binding;
5. keep provenance for the output-input relationship.

SLSA describes provenance as verifiable information connecting an artifact to where, when, and how
it was produced, and distinguishes outputs, parameters, dependencies, and the build platform
([SLSA provenance](https://slsa.dev/spec/v1.2/provenance)). That model is a better fit than a pair of
file timestamps.

False drift appears when generators embed current timestamps, random ordering, host paths, locale,
or tool-specific formatting. False confidence appears when only the schema is hashed but a template,
plugin, runtime, or transitive input changes. If regeneration is nondeterministic or the complete
input set cannot be identified, generated-output equality cannot be a reliable blocking invariant.

## Refactor failure modes

Refactors attack every locator:

- file moves break paths;
- symbol renames break names;
- extraction turns one endpoint into many;
- inlining turns many endpoints into one;
- overload or namespace changes make names ambiguous;
- generated partial definitions spread one logical API across files;
- repository splits and package moves change origin and ownership;
- squash/rebase rewrites commit identities even when final content is equivalent.

A migration engine can offer candidate edge rewrites using exact blob matches, language-aware symbol
maps, and Git similarity, but uncertain migrations must be reviewed. Never silently retarget an edge
based only on a similar name: that converts a visible broken link into an invisible wrong link.

Stable logical IDs help only if their lifecycle is governed. If developers can casually reuse an ID
for a different concept, the checker reports continuity where none exists. IDs need uniqueness,
tombstones for deleted concepts, explicit split/merge operations, and review of reuse.

## A safer enforcement model

### Tier A: hard blockers

Use only high-precision, locally reproducible invariants:

- malformed edge schema, duplicate IDs, forbidden cycles, or missing required ownership;
- missing local file/symbol for an active, version-matched edge;
- deterministic generated output differs from a clean regeneration;
- executable example fails in its pinned, sandboxed environment;
- machine-readable contract/schema validation fails;
- an acknowledgement was changed without the expected reviewed target digest or with an expired
  suppression.

### Tier B: required impact attestation

For a changed region or symbol whose semantic impact is not decidable, block only on the absence of a
decision. The UI should show:

- the exact documentation claim or section;
- relation type and version scope;
- old and new target fingerprints plus a focused diff;
- why the selector matched;
- owner and resolution command;
- choices of `updated`, `verified-still-correct`, or `not-applicable`, with a reason and reviewer.

This makes the enforced invariant honest: "a qualified person reviewed the declared impact," not
"the documentation is mathematically correct."

### Tier C: advisory discovery

Use heuristic symbol mentions, co-change history, embeddings, text/code models, and LLM reasoning to
propose missing edges or likely stale claims. Record confidence and allow feedback. Promote a rule to
Tier A only after project-specific evidence shows sufficiently high precision and deterministic
behavior for a narrow claim type.

### Operational controls

- Keep a single canonical typed edge and build reverse indexes automatically.
- Store target content/symbol digests, not a "newer timestamp" relation.
- Make the PR check offline, read-only, deterministic, incremental, and locally runnable.
- Run cross-repository and external-link checks in a separate unprivileged scheduled workflow.
- Group findings by root change and owner; cap displayed findings without hiding the total.
- Require expiring, owned suppressions with a reason and audit trail.
- Maintain schema and parser migrations before making a new checker version blocking.
- Run periodic full integrity checks to find orphaned edges, unsupported selectors, and stale owners.
- Preserve an emergency override with named approval, visible audit logging, and automatic follow-up;
  an override must not silently rewrite baselines.

## Pilot, measurement, and kill criteria

The following thresholds are proposed starting hypotheses, not standards. Measure them separately by
edge type because an aggregate can hide a disastrous rule behind a precise one.

### Before blocking any pull request

1. Select one narrow, owned surface with known semantics, such as CLI flags plus their reference
   page, or OpenAPI operations plus examples. Do not start repository-wide.
2. Replay at least six months of history. Have domain owners label each alert as (a) document became
   wrong, (b) review was useful but no edit was needed, or (c) no useful action.
3. Seed mutation cases for every claimed detector: delete/rename/move a target, change a signature,
   change behavior without a signature change, alter a generator input, backport a change, and edit
   only the acknowledgement.
4. Run in shadow mode for at least four weeks or 200 representative PRs, whichever gives enough
   examples per enforced edge type.
5. Establish current documentation-defect incidence and time-to-fix so the project can test whether
   the checker improves outcomes rather than merely creating activity.

### Suggested go/no-go thresholds

| Metric | Threshold for a hard-blocking rule | Action if missed |
|---|---|---|
| Semantic precision | At least 98% for a rule claiming actual incorrectness | Demote to attestation or advisory |
| Review actionability | At least 95% of alerts judged worth the requested review | Narrow selectors or demote |
| Recall on seeded, explicitly claimed cases | At least 90%, with unsupported cases clearly reported | Do not claim coverage; fix detector |
| Nondeterministic/infrastructure failure rate | Below 0.1%; zero dependence on mutable remote state in the PR blocker | Remove network/non-hermetic inputs |
| Incremental latency | `p95 <= 30 s`, with a documented hard timeout | Optimize, shard, or move work to scheduled audit |
| Unjustified override/suppression rate | Below 5%, and no suppressions past expiry | Stop blocking and investigate noise/ownership |
| Baseline-only or token-edit resolutions | Below 10% of findings | Treat as gaming signal; redesign acknowledgement |
| Orphaned active edges | Below 1% in full audits | Repair identity/migration process before expansion |
| Triage burden | Median under 3 minutes per affected PR, excluding real doc-writing time | Improve diagnostics or reduce scope |
| Outcome | Material reduction in stale-doc incidents or time-to-fix, targeted initially at 30% | Do not expand solely because metadata is green |

### Immediate kill or demotion criteria

Stop using the checker as a required gate, while retaining useful audits, if any of these persists:

- teams routinely refresh digests or make token doc edits without reviewing claims;
- zero-edge areas are presented as "covered" or "in sync";
- the tool cannot distinguish current, release, generated, and historical documentation;
- high-fan-out changes regularly require blanket overrides;
- there is no accountable owner or timely override path for a blocking edge;
- required results depend on external networks, mutable branches, an unpinned model, or privileged
  execution of pull-request content;
- security review cannot establish read-only least privilege, sandboxing, bounded parsing, and safe
  path/URL handling;
- checker/parser upgrades create more maintenance work than the documentation defects they prevent;
- measured documentation outcomes do not improve after a representative pilot;
- developers stop trusting the result, suppress it by default, or route urgent work around CI.

## Research evidence and source notes

All sources cited in this document were accessed on **2026-07-10**.

- [Software Documentation Issues Unveiled](https://csnagy.github.io/research/pdfs/2019/Aghajani2019-preprint.pdf),
  ICSE 2019: an empirical taxonomy based on 878 documentation-related artifacts.
- [Detecting Outdated Code Element References in Software Repository Documentation](https://arxiv.org/abs/2212.01479),
  Empirical Software Engineering preprint: a deliberately narrow detector, repository-scale runtime
  data, maintainer feedback, and concrete false positives.
- [Automatic Detection and Update Suggestion for Outdated API Names in Documentation](https://s-cube-xmu.github.io/uploads/Automatic%20Detection%20and%20Update%20Suggestion%20for%20Outdated%20API%20Names%20in%20Documentation.pdf),
  IEEE Transactions on Software Engineering: FreshDoc's scope and measured precision/recall limits.
- [The Impact of Traceability on Software Maintenance and Evolution](https://arxiv.org/abs/2108.02133),
  mapping study: benefits, maintenance cost, link-quality problems, and limited industrial evidence.
- [Interaction-based creation and maintenance of continuously usable trace links](https://link.springer.com/article/10.1007/s10664-020-09831-w),
  Empirical Software Engineering: trace-link creation, maintenance, vetting, precision, and recall.
- [9.6 Million Links in Source Code Comments](https://arxiv.org/abs/1901.07440), ICSE 2019: links
  are rarely updated, targets evolve, and almost 10% of studied links were dead.
- [Software Engineering at Google: Testing Overview](https://abseil.io/resources/swe-book/html/ch11.html)
  and [CI](https://abseil.io/resources/swe-book/html/ch23.html): hermeticity, trust, flakiness, and
  actionable failure signals.
- [Git commit documentation](https://git-scm.com/docs/git-commit.html#_commit_information) and
  [Git diff documentation](https://git-scm.com/docs/git-diff):
  controllable commit dates and heuristic/expensive rename detection.
- [GitHub permanent links](https://docs.github.com/en/repositories/working-with-files/using-files/getting-permanent-links-to-files)
  and [workflow path filters](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax#onpushpull_requestpull_request_targetpathspaths-ignore):
  immutable snapshot links and changed-file filter limits.
- [Docusaurus versioning](https://docusaurus.io/docs/versioning) and
  [Semantic Versioning 2.0.0](https://semver.org/): documentation release scopes and categories of
  interface change.
- [Bazel hermeticity](https://bazel.build/concepts/hermeticity),
  [SLSA provenance](https://slsa.dev/spec/v1.2/provenance), and
  [Protobuf cross-version guarantees](https://protobuf.dev/support/cross-version-runtime-guarantee/):
  reproducible input/output relationships, provenance, and generator/runtime skew.
- [GitHub `pull_request_target` security](https://docs.github.com/en/actions/reference/security/securely-using-pull_request_target),
  [script injections](https://docs.github.com/en/actions/concepts/security/script-injections),
  [workflow hardening](https://docs.github.com/en/code-security/tutorials/secure-your-organization/protect-against-threats#pin-third-party-actions-to-commit-shas),
  and [OWASP SSRF Prevention](https://cheatsheetseries.owasp.org/cheatsheets/Server_Side_Request_Forgery_Prevention_Cheat_Sheet.html):
  threats created when CI processes or fetches attacker-controlled content.
