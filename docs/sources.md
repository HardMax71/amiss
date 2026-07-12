# Annotated source ledger

Access date: 2026-07-12. The investigation favored official specifications, official tool
documentation, peer-reviewed papers, and author-hosted preprints. Vendor pages are labeled as vendor
claims; 2025-2026 preprints and demonstration papers are not treated as production validation.

The more detailed mechanism-by-mechanism review is in [prior-art.md](./prior-art.md).

## Problem evidence and traceability research

### Software Documentation: The Practitioners' Perspective

- Source: [ICSE 2020 conference page and abstract](https://2020.icse-conferences.org/details/icse-2020-papers/28/Software-Documentation-The-Practitioners-Perspective)
- DOI: [10.1145/3377811.3380405](https://doi.org/10.1145/3377811.3380405)
- Evidence: two surveys with 146 practitioners; obsolete, ambiguous, insufficient, and inadequate
  documentation are part of the practitioner-facing problem taxonomy.
- Relevance: supports the problem, not the efficacy of a hash-based solution.

### Using Traceability Links to Recommend Adaptive Changes for Documentation Evolution

- Source: [IEEE TSE DOI page](https://doi.org/10.1109/TSE.2014.2347969)
- Authors: Barthélémy Dagenais and Martin P. Robillard, 2014.
- Evidence: AdDoc discovers coherent sets of code elements documented together and reports pattern
  violations as artifacts evolve; its retrospective analysis covered four Java OSS projects and
  found at least half of documentation changes related to existing documentation patterns.
- Relevance: close academic precedent for graph-based documentation change-impact analysis. The
  "at least 50%" result is not detector precision or recall.

### Detecting Outdated Code Element References in Software Repository Documentation

- Source: [Empirical Software Engineering article](https://link.springer.com/article/10.1007/s10664-023-10397-6)
- CI follow-up: [DOCER GitHub Actions paper](https://arxiv.org/abs/2307.04291)
- Evidence: DOCER compares exact code-element presence when a document was updated with the current
  revision. In the top-project dataset, 28.9% of 918 successfully classified projects contained at
  least one currently outdated reference under the paper's narrow definition.
- Limits reported by the authors: identifiers can remain while behavior changes; changelogs and
  comments create false positives; images/video are invisible; exact matching loses recall; the
  history model is main-branch-only and parallel branch histories are problematic.
- Relevance: extremely close deterministic/history-based prior art and a strong warning against
  equating a missing identifier with semantically stale prose.

### Recent systematization of software artifact traceability

- Source: [SoK: Systematizing Software Artifacts Traceability via Associations, Techniques, and Applications](https://arxiv.org/abs/2603.16208)
- Status: March 2026 preprint; interpret cautiously.
- Evidence claimed by the review: 22 artifact types, 23 association types, only 37% of reviewed
  studies releasing code, and 95% of approaches evaluated in academic rather than specific
  industrial settings. It explicitly describes manual link maintenance as labor-intensive and prone
  to traceability debt.
- Relevance: supports typed relationships and highlights the industrial-adoption gap. It is not an
  independent validation of a proposed product.

### Architecture traceability and change impact

- Source: [The Supportive Effect of Traceability Links in Change Impact Analysis for Evolving Architectures](https://eprints.cs.univie.ac.at/4160/)
- Status: 2015 controlled-experiment paper metadata and abstract hosted by the University of Vienna.
- Evidence: reports that architecture traceability links reduced missing/incorrect assets and
  improved change-impact analysis quality in two controlled experiments.
- Relevance: evidence for navigation and impact analysis, not proof that linked documents remain
  correct.

## Closest product and OSS precedents

### Swimm

- Sources: [docs-as-code format](https://swimm.io/blog/docs-as-code-understanding-swimm-sw-md-markdown-format),
  [continuous-documentation CI](https://swimm.io/blog/continuous-documentation-through-continuous-integration-with-swimm),
  and [enterprise platform](https://swimm.io/enterprise-documentation-platform).
- Status: vendor claims.
- Claimed mechanism: code-coupled snippets/tokens/paths, Git-history-aware relocation or auto-sync,
  IDE backlinks, and CI escalation/blocking when a change needs human attention.
- Relevance: this is already very close to the proposed workflow. A new system needs differentiation
  such as an open, renderer-neutral relationship schema; arbitrary artifact selectors; explicit
  assurance semantics; or better self-hosting. Marketing language does not prove arbitrary prose is
  current.

### snippetdrift

- Source: [PyPI project](https://pypi.org/project/snippetdrift/)
- Status: small OSS package, version 0.1.0 released April 2026; no independent maturity evidence.
- Mechanism: Markdown sentinels select path/line regions, store SHA-256 fingerprints and review
  timestamps, synchronize snippets, and exit nonzero on content changes.
- Relevance: a direct implementation of the region-hash/acceptance concept. Fixed line ranges and
  snippet-only scope are major limitations.

### Doc Detective

- Sources: [official introduction](https://docs.doc-detective.com/docs/get-started/introduction)
  and [detected tests](https://docs.doc-detective.com/docs/test-docs/detected).
- Status: OSS official documentation.
- Mechanism: parses testable actions from documentation or specifications and executes browser,
  link, text, API, screenshot, and script checks; emits structured results.
- Relevance: stronger than hashes for documented procedures, but limited to testable paths and the
  configured environment.

## Prevention and executable-documentation mechanisms

### Rust documentation tests

- Source: [The rustdoc book](https://doc.rust-lang.org/rustdoc/documentation-tests.html)
- Mechanism: extracts documentation examples, compiles them, runs them unless configured otherwise,
  and supports assertions and expected compilation failures.
- Proof boundary: the example works under the tested configuration; surrounding prose and untested
  behavior remain unchecked.

### Python doctest

- Source: [Python standard library documentation](https://docs.python.org/3/library/doctest.html)
- Mechanism: executes interactive examples embedded in prose and compares output.
- Proof boundary: same as Rust, with additional sensitivity to output formatting and environment.

### Sphinx literal inclusion

- Source: [Sphinx directives documentation](https://www.sphinx-doc.org/en/master/usage/restructuredtext/directives.html)
- Mechanism: includes a whole file or selected Python object/line/text-delimited region.
- Proof boundary: prevents literal snippet copying from drifting; it cannot validate the explanation.

### OpenAPI and formal contract diffs

- Sources: [OpenAPI Specification](https://spec.openapis.org/oas/latest.html),
  [OpenAPI Generator](https://openapi-generator.tech/),
  [`openapi-diff`](https://github.com/OpenAPITools/openapi-diff), and
  [Buf breaking-change detection](https://buf.build/docs/breaking/).
- Mechanism: machine-readable API/schema contracts can generate reference material or be compared
  under explicit compatibility rules.
- Proof boundary: structural contract consistency is much stronger than a file timestamp; runtime
  behavior can still diverge from the contract.

## Emerging semantic detectors

### CASCADE

- Source: [FSE 2026 paper/preprint](https://arxiv.org/abs/2604.19400)
- Status: research system, stated as forthcoming in FSE 2026.
- Mechanism: generate tests from method docs, run them against existing code, and cross-check using
  code synthesized from the same documentation to reduce hallucinated-test false positives.
- Reported boundary: on the balanced Java benchmark, the full system traded recall for precision
  (prior-art.md records precision 0.88, recall 0.21, specificity 0.97). It found 13 previously unknown
  substantial inconsistencies across additional projects; the paper says 10 were fixed.
- Relevance: executable semantic evidence is promising for method behavior, not architecture prose
  or complete repo coverage.

### DocPrism

- Source: [2025 preprint](https://arxiv.org/abs/2511.00215)
- Status: preprint associated with ICSE 2026.
- Mechanism: LLM comparison of function docs and code with explicit filtering of benign
  "under-promises," where high-level docs intentionally omit implementation detail.
- Reported boundary: its ablation reduced flag rate from 98% to 14%; across 1,615 pairs in four
  languages it reports an average 15% flag rate and 0.62 precision. Recall is unavailable for the
  broad extension data because only flagged cases were manually reviewed.
- Relevance: direct evidence that a plain LLM consistency prompt is far too noisy for a zero-tolerance
  CI gate.

### ArtifactSync

- Source: [ICSE 2026 demonstration page](https://conf.researchr.org/details/icse-2026/icse-2026-demonstrations/20/ArtifactSync-Automated-Repository-Synchronization-through-Hierarchical-Change-Impact)
- Paper: [author-hosted PDF](https://das.encs.concordia.ca/pdf/ebube_ICSE2026.pdf)
- Status: four-page demonstration paper with preliminary evaluation.
- Mechanism: progressively inspect filenames, structure, then full content with an LLM to identify
  affected code/docs/tests/config and propose fixes.
- Reported boundary: designed individual scenarios performed much better than combined large commits;
  prior-art.md records combined-commit impact identification, recommendation, and fully correct fix
  rates of 80%, 75%, and 65%.
- Relevance: useful architecture for advisory link discovery, not deterministic blocking assurance.

## Versioning, identity, and selector engineering

### Git ignore and reference-name semantics

- Sources: versioned [`gitignore(5)` for Git 2.42.0](https://git-scm.com/docs/gitignore/2.42.0.html),
  versioned [`git-check-ref-format`](https://git-scm.com/docs/git-check-ref-format/2.42.0), and the
  upstream [`v2.42.0` Git source tag](https://github.com/git/git/tree/v2.42.0).
- Evidence: the ignore manual fixes source precedence, relative matching, last-match negation,
  directory rules, wildcard/escape/range behavior, and the special double-star forms; the ref
  manual enumerates the ten forbidden-shape rules for ordinary refs.
- Design consequence: `ref-format-v1` is defined directly. The Git 2.42.0 ignore oracle/vectors are
  retained only as worktree-RFC research; scanner v0 rejects worktree mode and therefore inherits no
  ambient ignore files, config, locale, or installed-Git matcher behavior.

### Git object data model

- Sources: [`git hash-object`](https://git-scm.com/docs/git-hash-object),
  [Git objects](https://git-scm.com/book/en/v2/Git-Internals-Git-Objects), and
  [Git data model](https://git-scm.com/docs/gitdatamodel.html).
- Evidence: Git blobs represent file contents independently of location; commits also contain the
  tree, parents, authorship, and time information.
- Design consequence: use a selected content projection as the validity key. Keep a commit ID only as
  provenance for showing what changed.

### Git hash-function transition

- Sources: [official transition design](https://git-scm.com/docs/hash-function-transition/2.49.0.html),
  Git v2.44.0's pinned
  [`sha1collisiondetection` submodule](https://github.com/git/git/tree/v2.44.0/sha1collisiondetection),
  and its exact upstream
  [source commit](https://github.com/cr-marcstevens/sha1collisiondetection/tree/855827c583bc30645ba427885caa40c5b81764d2).
- Evidence: repositories can use SHA-1 or SHA-256 object names during the transition; Git documents
  that hardened SHA-1 mitigates known attacks but SHA-1 remains weak. Git v2.44.0 binds the detector
  source to commit `855827c583bc30645ba427885caa40c5b81764d2`.
- Design consequence: store an explicit checker-owned algorithm name rather than assuming a Git OID
  is always a 40-character SHA-1; pin collision-detection behavior for SHA-1 object preimages, and
  do not mistake that mitigation for a cryptographically strong provider authorization identity.

### Timestamps and reproducibility

- Source: [Reproducible Builds: Timestamps](https://reproducible-builds.org/docs/timestamps/)
- Evidence: timestamps are identified as a major reproducibility problem; the project recommends
  controlled `SOURCE_DATE_EPOCH` values and content/source tracking more precise than time.
- Design consequence: timestamps can drive review-age service levels, never content validity.

### Tree-sitter code navigation

- Source: [Tree-sitter code navigation](https://tree-sitter.github.io/tree-sitter/4-code-navigation.html)
- Evidence: query-driven tags identify named definitions/references and kinds across languages.
- Design consequence: a plausible symbol-selector plugin layer exists, but each grammar/query set and
  overload policy remains versioned tool logic.

### Robust text anchors

- Source: [W3C Web Annotation selectors](https://www.w3.org/TR/selectors-states/)
- Evidence: distinguishes exact text plus prefix/suffix context (`TextQuoteSelector`) from brittle
  character positions (`TextPositionSelector`).
- Design consequence: explicit document IDs are preferable; quote plus context is a reasonable
  fallback; line/character positions are for display.

## CI usability, ownership, and diagnostics

### Tricorder and developer-perceived false positives

- Sources: [Google Research paper page](https://research.google/pubs/tricorder-building-a-program-analysis-ecosystem/)
  and [Building Secure and Reliable Systems, chapter 13](https://google.github.io/building-secure-and-reliable-systems/raw/ch13.html).
- Evidence: Google's workflow-integrated analysis system emphasizes low user-perceived false-positive
  rates, easy-to-understand/action findings, automatic fixes, and feedback. The book describes a
  target of at most 10% user-perceived false positives and disabling checks based on "Not useful"
  feedback.
- Relevance: documentation impact findings need the same actionability discipline. For deterministic
  change triggers, measure irrelevant-review rate rather than claiming the change signal is factually
  false.

### GitHub CODEOWNERS and required review

- Source: [GitHub documentation](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/about-code-owners)
- Evidence: changed owned paths request reviews; rules can require a code-owner approval. GitHub notes
  that one of several listed owners is sufficient and recommends protecting the CODEOWNERS file
  itself.
- Design consequence: ownership can make attestation independent of the author, but it cannot prove
  careful review and can create bottlenecks. Protect relationship policy and the attestation ledger too.

### SARIF fingerprint prior art (deferred)

- Source: [OASIS SARIF 2.1.0](https://docs.oasis-open.org/sarif/sarif/v2.1.0/os/sarif-v2.1.0-os.pdf)
- Evidence: SARIF defines result fingerprints intended to survive irrelevant location changes and
  supports baselining large result sets.
- Design consequence: bind stable finding identities in deterministic JSON instead of identifying a
  finding by its current line number. SARIF is prior art only and is deferred from scanner v0.

### Documentation classes

- Source: [Diátaxis primer](https://diataxis.fr/start-here/)
- Evidence: distinguishes tutorial, how-to, reference, and explanation by user need.
- Design consequence: assurance policy should vary by document purpose. Add explicit historical,
  normative-specification, and planned/research lifecycle classes for repository governance.

## Second pass: standalone-tool research (2026-07-10)

Five parallel research passes (product landscape, academic literature, requirements-engineering
mechanics, docs-as-code mechanisms, market evidence) added the sources below. The full
mechanism-level catalog for the first four lives in [prior-art.md](./prior-art.md); this ledger
keeps the entries that carry the dossier's new claims, with evidence-strength notes.

### fiberplane/drift

- Source: [repository](https://github.com/fiberplane/drift) and
  [announcement](https://fiberplane.com/blog/drift-documentation-linter/)
- Status: OSS, v0.10.1 in June 2026, around 119 stars.
- Evidence: path and symbol anchors, tree-sitter-normalized AST fingerprints (XxHash3) in a
  committed `drift.lock`, CI failure on divergence, `drift link` re-attestation, `drift refs`
  reverse lookup, a cross-repo `origin` field.
- Relevance: closest existing implementation; proves the fingerprint-lockfile mechanic needs no
  git history. Differentiation must come from the claim model, non-code selectors, and attestation
  semantics.

### Doorstop mechanics and user record

- Sources: item and validation reference docs, issues 173, 174, 178, and 564 (all linked in
  prior-art.md), and Browning and Adams, JSEA 2014.
- Evidence: a link stamp is SHA-256 over the parent's identity, text, references, and links;
  suspect means stored differs from recomputed; `clear` and `review` form the lifecycle;
  links-born-suspect was reverted after users revolted; hash-based code-reference review was
  requested, prototyped by a user, and never shipped; `review all` plus `clear all` is the
  documented catch-up workflow.
- Relevance: the attestation loop's production ancestor, with its fatigue failure modes on record.

### Gray links

- Source: [Niu et al., FSE 2016](https://homepages.uc.edu/~niunn/papers/FSE16.pdf)
- Evidence: analysts accept uncertain trace links with less scrutiny than they apply to
  rejections; the authors recommend structured vetting.
- Relevance: the empirical basis for leading the attestation UI with the target diff and for
  rejecting bulk acceptance as a ritual-compliance path. Acceptance remains one claim per explicit
  transaction; typed split/merge lifecycle transactions are the only multi-claim operations and
  do not accept their successors.

### The api-report pattern

- Sources: API Extractor configuration docs, rushstack issue 1856, azure-sdk-for-js issue 4282
  (linked in prior-art.md).
- Evidence: CI fails until a regenerated public-API report is committed and reviewed; a formatter
  rewriting the report broke comparison; warning-level enforcement let changes slip through.
- Relevance: change-triggered attestation-by-commit is already an accepted mainstream workflow,
  and its two recorded failures (normalization, severity) transfer directly.

### Postman State of the API

- Sources: 2023 report PDF (about 37,000 respondents) partly via secondary summaries; 2024 press
  release (about 5,600).
- Evidence: 2023: 52% call lack of documentation the top obstacle to consuming APIs and 57% name
  up-to-date documentation the top improvement. 2024: 39% call inconsistent docs the biggest
  collaboration roadblock; 44% read source code instead.
- Status: 2023 percentages partly read through coverage of the report; the sample shrank sharply
  in 2024, so trend claims are weak.
- Relevance: the largest-sample statement of exactly this problem.

### DORA reports, 2021 through 2025

- Sources: dora.dev report and capability pages, the Google Cloud 2021 announcement, and press
  coverage for the one-in-four figure.
- Evidence: quality-docs teams 2.4 times likelier to reach top delivery performance (2021, with
  roughly one in four reporting docs that good); docs quality multiplying every measured practice
  (2022; continuous integration's predicted benefit 750% versus 34%); delivery stability down 7.2%
  as AI adoption rose while predicted docs-quality gains from AI top the factor list (2024); 90%
  AI usage and the amplifier framing (2025).
- Status: self-reported surveys and model predictions, not experiments.
- Relevance: the buyer-side budget argument.

### Machine-reader adoption

- Sources: Mintlify Series B post (April 2026), Context7 repository and PulseMCP statistics,
  agents.md and the Linux Foundation announcement, the llms.txt origin post, and Search Engine
  Journal on John Mueller's comment.
- Evidence: nearly half of traffic to Mintlify-hosted docs comes from AI agents (self-reported);
  $45M Series B at a $500M valuation on the docs-for-AI thesis; Context7 at about 57,000 stars and
  a million weekly uses purely to route around stale docs; AGENTS.md formalized in 2025 and later
  donated to the Linux Foundation. Counterpoint on record: Mueller reports AI services do not
  actually fetch llms.txt.
- Relevance: the why-now core, with its honest hole attached.

### Swimm funding and friction

- Sources: TechCrunch Series A coverage (2021), Swimm auto-sync and CI documentation, HN threads
  and third-party reviews.
- Evidence: $27.6M Series A, $33.3M total, no announced round since 2021; complaints center on
  workflow friction rather than detection quality; the full-clone requirement is structural to
  history-based re-anchoring.
- Relevance: the cautionary adoption tale, plus two direct design inputs (state in the repo, one
  gesture to resolve).

### Developer-experience surveys

- Sources: Atlassian State of Developer Experience 2024 (2,100+ respondents), Stack Overflow 2024
  survey insights page.
- Evidence: 69% of developers lose eight or more hours a week to inefficiencies, with insufficient
  documentation among the top self-reported causes; 68% hit a knowledge silo weekly and under half
  can easily surface up-to-date internal information.
- Relevance: time-cost evidence beyond API documentation.

### Technical-writer contraction

- Sources: ACS Information Age on Canva (ten of twelve technical writers laid off, March 2025); a
  practitioner estimate of roughly 30% contraction (idratherbewriting.com).
- Status: one confirmed company event plus one practitioner estimate; directional only.
- Relevance: fewer humans absorbing drift; it cuts both ways on willingness to pay.

## Local evidence

Repository facts and reproducible commands are in [repo-audit.md](./repo-audit.md). The key early
example is `docs/content/docs/design/architecture.mdx#ci-surface`: it says ten workflows exist and
names `docs.yml`, while the current tree has 22 workflow files and no `docs.yml`. The page's latest
commit predates two newly added workflows by about one day, but the count/name mismatch was already
present at that page commit. This cleanly demonstrates both sides of the proposal:

- A path-set fingerprint accepted at that page commit would have requested review after the two new
  workflows appeared.
- It could not prove that the already-incorrect count and nonexistent name were correct at initial
  acceptance.

## Pre-implementation contract and security sources

Accessed 2026-07-11.

### CommonMark link-reference definitions

- Source: [CommonMark 0.31.2, link-reference definitions](https://spec.commonmark.org/0.31.2/#link-reference-definitions).
- Evidence: reference labels have document-wide meaning; when several matching definitions exist,
  the first takes precedence. Definitions do not acquire native heading scope from their placement.
- Relevance: repeated `[assure]: ...` lines cannot be treated as independent governed declarations
  without a separate tool grammar. Unique claim labels and conformance fixtures are required.

### Frozen Markdown/MDX grammar profiles

- Sources: [CommonMark 0.31.2](https://spec.commonmark.org/0.31.2/), the
  [GFM 0.29-gfm specification](https://github.github.com/gfm/), and the official
  [MDX repository/release line](https://github.com/mdx-js/mdx), plus the official
  [`remark-gfm@4.0.1` API](https://github.com/remarkjs/remark-gfm/tree/4.0.1).
- Evidence: CommonMark and GFM publish executable examples; `remark-gfm` explicitly includes
  footnotes and defaults `singleTilde` to true even though it notes that single-tilde strike is
  outside formal GFM. The plugin does not render HTML. MDX 3 adds ESM, JSX, and expressions whose
  parsing changes Markdown surface spans.
- Relevance: `commonmark-gfm-v1` uses the exact pinned `remark-gfm@4.0.1` parse bundle with
  `{singleTilde: true}`, names footnotes/single-tilde as plugin extensions, and applies no tagfilter
  rendering transform because raw HTML is opaque. `mdx-source-v1` adds the exact
  `remark-mdx@3.1.1` syntax. The local
  reference oracle is pinned by `docs/package-lock.json` at commit `1e31df…` (blob `926269e5…`, raw
  SHA-256 `cfcf4f37…`), with exact unified/remark versions recorded in scanner-v0-spec. Parser
  provenance strings cannot select different grammar semantics; the nonshrinkable all-example
  conformance corpus is a Gate-A prerequisite.

### Git LFS pointer grammar

- Source: [Git LFS specification, Pointer section](https://github.com/git-lfs/git-lfs/blob/d72db1e533a1d6ee5543e02e9f8ccac97e0fcd34/docs/spec.md#the-pointer),
  rechecked 2026-07-12.
- Evidence: a pointer is UTF-8, uses sorted key/value lines with `version` first, contains required
  `oid` and `size` keys, may contain unknown extension lines, and is less than 1,024 bytes. The
  reference client also reads the documented pre-release `hawser.github.com` version.
- Relevance: recognizing only the common three-line example would misclassify valid extended or
  legacy pointers as repository content. Scanner v0 freezes a bounded conservative recognizer and
  never runs LFS filters or fetches the referenced object.

### URI generic syntax

- Source: [RFC 3986, Uniform Resource Identifier generic syntax](https://www.rfc-editor.org/rfc/rfc3986),
  Internet Standards Track.
- Evidence: the RFC defines scheme, authority, path, query, fragment, percent-encoding, and relative
  reference syntax, and treats scheme case as insensitive.
- Relevance: scanner `uri-reference-v1` uses the ASCII generic syntax without ambient URL-library
  normalization, then deliberately narrows HTTP(S) and same-repository GitHub recognition. IDNA,
  authority normalization, default-port folding, and repeated percent decoding are not implied.

### MDX execution surface

- Source: [What is MDX?](https://mdxjs.com/docs/what-is-mdx/), official MDX documentation.
- Evidence: MDX combines Markdown with JSX, JavaScript expressions, and ESM import/export syntax.
- Relevance: a privileged checker must analyze source without importing or evaluating a page or its
  repository-controlled plugins.

### GitHub Actions secure use

- Source: [GitHub Actions secure-use reference](https://docs.github.com/en/actions/reference/security/secure-use).
- Evidence: GitHub calls a full-length commit SHA the only immutable Action pin and warns against
  combining privileged workflow contexts with untrusted checkouts or pull-request content.
- Relevance: the first checker is read-only and digest-pinned. Comment-command writers and
  privileged refresh automation are excluded from the accepted contract, not latent v0 features.

### GitHub required-check identity and freshness

- Sources: GitHub's current
  [required-status troubleshooting](https://docs.github.com/en/pull-requests/collaborating-with-pull-requests/collaborating-on-repositories-with-code-quality-features/troubleshooting-required-status-checks),
  [ruleset troubleshooting](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/troubleshooting-rules),
  [available rules for rulesets](https://docs.github.com/en/enterprise-cloud@latest/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/available-rules-for-rulesets),
  and the [organization-ruleset REST API](https://docs.github.com/en/rest/orgs/rules),
  rechecked 2026-07-12.
- Evidence: GitHub documents a seven-day repository recency condition, evaluates required checks
  against the latest relevant commit SHA, and says required checks do not distinguish workflow,
  matrix, or event trigger type. Selecting an expected app constrains the status producer but still
  does not identify a workflow or event. Organization/enterprise ruleset workflows instead select
  a source repository and workflow; their REST shape exposes workflow `repository_id`, `path`,
  `ref`, and a `sha` described as the commit SHA of the workflow file. The gate requires that SHA
  to be non-null and additionally resolves the path to an exact blob/dependency closure, because a
  mutable ref/path alone is not content identity. Supported PR/merge-queue events and filter
  behavior are provider-defined. None of these native keys encode this design's base snapshot,
  ten-minute trusted-time window, exception expiry, or external-control epoch.
- Relevance: a status that was fresh at publication can outlive a debt/waiver or survive floor,
  constraint, or base changes. A stable lane therefore requires an externally owned exact-source
  content-pinned ruleset workflow (or proven provider equivalent), but that source protection is not sufficient:
  required enforcement remains blocked on an authenticated merge-time control-epoch check plus
  invalidation/rerun evidence. Ordinary candidate-SHA/context or expected-app success is
  insufficient.

### GitHub skipped and neutral required-check behavior

- Sources: GitHub's [job-condition documentation](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-jobs-with-conditions)
  and [required-status troubleshooting](https://docs.github.com/en/pull-requests/collaborating-with-pull-requests/collaborating-on-repositories-with-code-quality-features/troubleshooting-required-status-checks),
  rechecked 2026-07-12.
- Evidence: a job skipped by a conditional reports success, and required checks treat successful,
  skipped, and neutral conclusions as successful states.
- Relevance: path-filter absence alone is not fail-closed. The future protected job must be
  unconditional with no continue-on-error/dependency skip, and the controller may map only an
  authenticated complete passing envelope to provider success.

### GitHub punctuation-leading repository names

- Source: the live public repository [citypaul/.dotfiles](https://github.com/citypaul/.dotfiles),
  rechecked 2026-07-12.
- Evidence: a valid GitHub repository name can begin with `.`; the earlier identity regex accepted
  only the two `.github` special cases.
- Relevance: all duplicated RepositoryIdentity schemas now share a broader bounded lower-case
  punctuation grammar and a `.dotfiles` validation vector. Owner grammar remains separate, and
  provider-authenticated identity—not the regex alone—establishes repository existence.

### GitHub repository identity component casing

- Source: GitHub's [Get a repository REST endpoint](https://docs.github.com/en/rest/repos/repos#get-a-repository),
  rechecked 2026-07-12.
- Evidence: the endpoint documents both `owner` and `repo` path parameters as case-insensitive.
- Relevance: same-repository URL recognition ASCII-folds only literal unescaped owner/repository
  components before comparing the authenticated lowercase identity. It does not fold the host,
  percent-encoded/IDNA bytes, refs, or repository paths, and it preserves raw URL spelling in the
  raw-destination digest. This also makes the recorded mixed-case user-zero URLs reproducible.

### Git storage grammars

- Sources: Git's pinned [pack/index grammar 2.44.0](https://git-scm.com/docs/gitformat-pack/2.44.0)
  and [staged-index grammar 2.44.0](https://git-scm.com/docs/index-format/2.44.0).
- Evidence: the pack source defines SHA-1/SHA-256 checksum widths, loose-equivalent object IDs,
  pack versions 2/3, index versions 1/2, delta encodings, and trailers; the index source defines
  `DIRC`, versions 2–4, entry flags/path compression, extensions, and the final checksum.
- Relevance: scanner v0 uses an in-process, no-follow primary-object/index reader with closed
  duplicate-pack precedence and typed rejection of split/sparse index forms. An installed Git or
  mutable repository configuration cannot silently choose acquisition semantics.

### GitHub merge queues

- Source: [Managing a merge queue](https://docs.github.com/en/enterprise-cloud@latest/repositories/configuring-branches-and-merges-in-your-repository/configuring-pull-request-merges/managing-a-merge-queue).
- Evidence: a merge-group candidate can contain the base branch plus the changes from pull requests
  ahead in the queue, and required checks run on that candidate's head SHA.
- Relevance: per-pull-request attribution is diagnostic. A protected invariant must be evaluated on
  the exact final candidate rather than excused because another queued change contributed to it.

### JSON canonicalization

- Sources: [RFC 8785](https://www.rfc-editor.org/rfc/rfc8785) and its
  [verified errata](https://www.rfc-editor.org/errata/rfc8785).
- Evidence: JCS defines canonical JSON serialization; verified errata include a security-relevant
  recommendation concerning negative zero.
- Relevance: JSON Lines is a plausible ledger format only with a frozen numeric subset, strict
  parsing, domain separation, duplicate rejection, and published cross-platform hash vectors.

### Suspect-link patent family

- Sources: [U.S. patent application 20080059977 and claims](https://patents.justia.com/patent/20080059977)
  and the USPTO's [Patent Center entry point](https://www.uspto.gov/patents/apply).
- Evidence: the publication describes stored links between requirements-management and
  configuration-management objects, automatically marking related links suspect after object
  changes, and clearing suspect status after input. The USPTO directs users to Patent Center for
  application management and status information.
- Relevance: close enough to the proposed commercial change-impact and suspect-link workflow to
  warrant professional claim, family, jurisdiction, and current-status review. This ledger does
  not offer a legal conclusion.

## Market reassessment sources

Accessed 2026-07-11. These sources correct the earlier “empty quadrant” conclusion.

### Fiberplane `drift`, current implementation

- Sources: [project repository](https://github.com/fiberplane/drift),
  [March 2026 announcement](https://fiberplane.com/blog/drift-documentation-linter/), and
  [Fiberplane engineering blog](https://blog.fiberplane.com/blog/).
- Repository facts checked through the GitHub API: MIT; v0.10.1 published 2026-06-22; 119 stars on
  the access date.
- Evidence: explicit Markdown-to-file/symbol anchors, tree-sitter-normalized signatures,
  `drift.lock`, CI blocking, relinking, reverse lookup, and optional cross-origin metadata. The
  announcement explicitly says relinking does not prove that prose was reviewed.
- Vendor-reported engineering result: randomized serializer testing reduced spurious lock merge
  conflicts from roughly 44% to roughly 25% after a format change. The harness and workload were
  not independently reproduced in this pass.
- Relevance: direct implementation precedent, direct competition, support for the trust-boundary
  critique, and evidence that committed binding state has nontrivial merge cost.

### `ryanwaits/drift`

- Sources: [project repository](https://github.com/ryanwaits/drift) and
  [product site](https://www.driftdev.sh/).
- Repository/site facts on the access date: MIT, TypeScript, 423 commits, three stars, site version
  v0.42.0.
- Claimed/visible surface: fifteen TypeScript/JSDoc/Markdown rules, API extraction, example
  validation, a coverage ratchet, structured JSON, agent workflows, monorepo support, and a GitHub
  Action.
- Relevance: invalidates the claim that deterministic docs-drift tooling is an empty category. Its
  TypeScript focus leaves room for other scopes but removes many formerly claimed differentiators.

### Swimm, current enterprise positioning

- Sources: [enterprise documentation platform](https://swimm.io/enterprise-documentation-platform)
  and [current application-understanding page](https://swimm.io/home).
- Vendor claims: code-coupled documentation, CI alert/block, automatic generation, enterprise and
  on-premises deployment, large language/codebase coverage, and deterministic-analysis-plus-AI
  positioning.
- Relevance: confirms the commercial category remains occupied. Performance, completeness, and
  accuracy language is vendor-authored and not independent evidence.

### Context rot in agent configuration files

- Source: Treude and Baltes,
  [Context Rot in AI-Assisted Software Development](https://arxiv.org/abs/2606.09090), June 2026
  preprint.
- Evidence reported by the authors: an existing README/wiki consistency checker found stale code
  element references in 23.0% of a statistically representative sample of 356 repositories.
- Relevance: strengthens the problem hypothesis for CLAUDE.md, AGENTS.md, and related files. It
  does not validate the proposed state model, workflow, or willingness to pay.

### USPTO current-status process

- Sources: [Maintain your patent](https://www.uspto.gov/patents/maintain) and the Department of
  Commerce [Patent Maintenance Fee Events dataset](https://catalog.data.gov/dataset/patent-maintenance-fee-events-1981-present).
- Evidence: the USPTO says utility-patent maintenance fees are due at 3.5, 7.5, and 11.5 years and
  directs current status lookup through its maintenance systems; the public bulk dataset contains
  recorded maintenance events.
- Relevance: reinforces that publication metadata is not a current legal-status opinion. Counsel
  must retrieve and analyze the official file, family, and maintenance history before a commercial
  suspect-link launch.
