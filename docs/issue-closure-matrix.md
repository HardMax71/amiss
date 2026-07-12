# Pre-implementation issue closure matrix

Date: 2026-07-12.

Status: authoritative traceability record for the pre-implementation review. This matrix records
every issue as one of: resolved by a normative decision, rejected/superseded, deliberately
unsupported, blocked on reproducible evidence, or owned by an external authority. “Addressed” does
not mean “implemented” or “tested.” An evidence-blocked row remains a no-go gate.

The normative order is:

1. [scanner-v0-spec.md](./scanner-v0-spec.md) for the discard-state scanner;
2. [machine-contracts.md](./machine-contracts.md) for scanner wire inputs/output;
3. [ci-security-spec.md](./ci-security-spec.md) for snapshots, policy, security, and operations;
4. [normative-core-spec.md](./normative-core-spec.md) for the future governed model;
5. [directive-rfc.md](./directive-rfc.md) for the disabled governed authoring syntax;
6. [implementation-readiness.md](./implementation-readiness.md) for what work is authorized.

Earlier alternatives in [design.md](./design.md), [open-problems.md](./open-problems.md), and
[v0-contract-review.md](./v0-contract-review.md) are historical rationale when they conflict with
that order.

## Closure legend and top-level verdict

| Status | Meaning |
| --- | --- |
| `RESOLVED-SPEC` | The ambiguity has one normative answer; implementation and tests may still be pending |
| `REJECTED` | The proposed behavior was shown unsafe/noisy or superseded and is outside the product contract |
| `DEFERRED-TYPED` | The capability has an exact unsupported/non-green result and no fallback |
| `BLOCKED-EVIDENCE` | A falsifiable experiment or executable test must pass before the affected stage starts |
| `EXTERNAL-GATE` | Provider, pilot team, design partner, or counsel must supply evidence/authority |

| Product stage | Current decision |
| --- | --- |
| Disposable scanner scaffold/conformance harness | **Authorized** for CLI/schema/Git acquisition, hostile fixtures, and the complete parser corpus; parser integration/evaluator remain blocked until that corpus passes |
| Stable machine API or required CI scanner | **Not yet authorized**; output/control schemas exist, provider request/control-epoch roots do not, and X-02, X-04, X-05, and X-07 remain open |
| Persisted observations or governed state | **Not authorized**; Gate B and X-01/X-06 are open |
| Enabled governed directives/claims | **Not authorized**; Gate C, X-03, and X-08 are open |
| Required narrative-attestation gate | **Not authorized**; Gate D, ownership, provider verification, and shadow evidence are open |
| Standalone commercial product | **Not authorized**; build-vs-extend, buyer, and legal gates are open |

## Original P0 blockers

| ID | Resolution | Status and remaining evidence |
| --- | --- | --- |
| P0-01 automatic baselines called attestations | Observation, acceptance, validation, trust, review context, and policy are separate axes; initialization cannot accept anything | `RESOLVED-SPEC`; trust-on-edit is also empirically rejected by three containing-block edits while cases remained broken; invariant tests remain |
| P0-02 content-derived governed identity | Governed claims use authored immutable `ClaimId`; ungoverned extraction uses churn-permitted `ObservationId` | `RESOLVED-SPEC`; directive/lifecycle tests remain under X-03/X-08 |
| P0-03 ledger contains hidden authored intent | Relation, selector, endpoint, scope, and validator intent live in a declaration or future named-check schema; state stores no hidden claim | `RESOLVED-SPEC`; named checks are `DEFERRED-TYPED` |
| P0-04 repeated `[assure]` cannot represent claims | RFC A-001 requires unique `assure:<ClaimId>` labels and two exact simple expansions; repeated labels are invalid | `RESOLVED-SPEC` plus local parser evidence; enabled adapters remain `BLOCKED-EVIDENCE` on X-03 |
| P0-05 relation labels lack transition semantics | Core defines a closed relation ADT, authority, invalidation, arity, completion, and cycle rules | `RESOLVED-SPEC`; unimplemented validators remain unsupported |
| P0-06 digest and lock formats were contradictory | SHA-256, JCS restrictions, domains, per-endpoint snapshots, seals, and logical transition requirements are fixed; a global writable lock is rejected | Scanner digests are `RESOLVED-SPEC`; governed record/storage JSON Schema, full vectors, and X-06 physical evidence are explicitly `BLOCKED-EVIDENCE` before compatibility freeze |
| P0-07 a valid lock is not authenticated review | Local acceptance is `self-asserted`; structural validity is separate from trust; blocking narrative use requires provider-verified evidence | `RESOLVED-SPEC`; provider verification is an `EXTERNAL-GATE` |
| P0-08 refresh performs security-sensitive writes | Refresh is not a command or actor; v0 writes nothing; governed transitions occur only through explicit reviewed commands | `REJECTED`; v0 returns `INVALID_INVOCATION`, exit 2 |
| P0-09 deletion/split/merge/retirement unsafe | Two-stage retirement, permanent tombstones, split/merge closed transactions, ID non-reuse, and CAS are fixed | `RESOLVED-SPEC`; executable lifecycle suite and X-08 remain open |
| P0-10 scope was only a field name | `candidate-tree` is the only supported governed scope; every other known scope is explicitly unsupported with no fallback | `RESOLVED-SPEC` plus `DEFERRED-TYPED` for historical/release/environment/external scopes |
| P0-11 merge attribution used as safety exception | Exact final candidate safety is absolute; attribution is diagnostic; record and acceptance predecessors use CAS | `RESOLVED-SPEC`; real fork/merge-group/provider testing is `BLOCKED-EVIDENCE` X-07 |
| P0-12 policy/suppression can erase itself | Candidate control planes are compared; repository policy is raise-only; weakening/meta-findings are unsuppressible failures; external controls are digest-bound | `RESOLVED-SPEC`; attack tests and external deployment controls remain |
| P0-13 dishonest green and impossible first run | Result language says “no blocking findings in evaluated scope”; complete denominators and zero-coverage states are mandatory; no `init` exists in v0 | `RESOLVED-SPEC`; report schema is published, implementation tests remain |
| P0-14 selector migration conflated with engine change | Retarget, locator migration, projection-contract migration, and implementation provenance have distinct transitions/causes | `RESOLVED-SPEC`; migration tests remain under X-08 |

## Original P1 requirements

| ID | Resolution | Status and remaining evidence |
| --- | --- | --- |
| P1-01 owner/reviewer identity | Document/evidence/policy/waiver roles are distinct; offline metadata proves no reviewer; blocking narrative needs provider-verified eligible approval | `RESOLVED-SPEC` plus `EXTERNAL-GATE`; no provider/CODEOWNERS pilot exists |
| P1-02 adoption debt | No automatic debt; enforcement requires cleanup or an externally reviewed exact-fact debt snapshot with owner and expiry | `RESOLVED-SPEC`; actual rollout input is external |
| P1-03 privileged automation | V0 is read-only, credentialless, networkless, and never uses privileged PR parsing/writers | `RESOLVED-SPEC`; workflow/provider isolation tests remain X-07 |
| P1-04 validator provenance | A validator proves only its declared contract; hermetic complete-provenance language requires an enforced sandbox | `DEFERRED-TYPED`; executable validators are absent from v0 |
| P1-05 error/timeout/unsupported/skipped conflation | Closed facts, fail-closed completeness, strict limits, and exit classes 0/1/2 are fixed | `RESOLVED-SPEC`; fault/resource/platform tests remain X-04/X-05/X-07 |

## P2 deferrals

| Deferred feature | Exact disposition | Status |
| --- | --- | --- |
| Cross-repository relationships | Foreign URLs are `external-out-of-scope`; no configuration/request surface, fetch, or candidate-tree fallback exists | `DEFERRED-TYPED` |
| Deployed/external semantic claims | Unsupported scope in PR scanner; later scheduled observation only | `DEFERRED-TYPED` |
| TTL/live external checks | Absent from pure core; no time-based freshness claim | `DEFERRED-TYPED` |
| Transcripts/browser/service probes | `validation = unsupported`; repository code is never executed | `DEFERRED-TYPED` |
| LLM discovery/judgment | Absent from stable v0; any later model lane is advisory and cannot write state or affect exit | `DEFERRED-TYPED` |
| Similarity/refactor auto-migration | No v0 request surface; extra options/fields are invalid, and future governed migration must remain explicit | `DEFERRED-TYPED` |
| Monorepo physical-state scaling | Per-claim files are the X-06 candidate, not a stable storage promise | `BLOCKED-EVIDENCE` X-06 |
| App UI, SARIF, editor, comments, agent ranking | Absent; deterministic JSON is the first consumer contract | `DEFERRED-TYPED` |
| Historical-at-revision checking | `unsupported-scope`; current-tree fallback forbidden | `DEFERRED-TYPED` |
| Translated-tree/cross-locale lag | No v0 request/rule ID; independent structural checks remain possible | `DEFERRED-TYPED` |
| Bitmap diagram/OCR semantics | No v0 request/rule ID; only image path existence may resolve | `DEFERRED-TYPED` |
| Tamper-evident external audit | Repository state remains non-standalone; future receipts are external overlays | `DEFERRED-TYPED` plus `EXTERNAL-GATE` |

## C-01 through C-16 contract decisions

| ID | Final decision | Status |
| --- | --- | --- |
| C-01 artifact/selector/resolution/projection split | Four distinct types; current bytes never alter authored identity | `RESOLVED-SPEC` |
| C-02 arity and identity | Explicit stable governed ID; one subject plus relation-constrained dependencies; observation identity is separate | `RESOLVED-SPEC`, correcting the old content-derived default |
| C-03 baseline granularity | Per-endpoint snapshots plus one atomic acceptance seal | `RESOLVED-SPEC` |
| C-04 status lattice | Orthogonal facts; disposition is derived last | `RESOLVED-SPEC` |
| C-05 authority versus invalidation | Relation constructor defines both independently | `RESOLVED-SPEC` |
| C-06 acceptance transition | Explicit event, two predecessor chains, no implicit edit/refresh | Logical transition is `RESOLVED-SPEC`; strict governed record/storage wire and writer remain Gate B |
| C-07 selector/engine versioning | Selector intent, projection contract, engine contract, and implementation digest are separate | `RESOLVED-SPEC` |
| C-08 hash/encoding | SHA-256 plus domain-separated HB/HJ and restricted RFC 8785/JCS | `RESOLVED-SPEC`; full cross-platform state vectors are Gate B |
| C-09 policy composition | Facts, built-in disposition, raise-only repository policy, external floor, exact debt/waiver, unsuppressible clamp | `RESOLVED-SPEC` |
| C-10 version scope | Candidate tree only initially; all others explicit unsupported | `RESOLVED-SPEC`, correcting pinned-history v0 |
| C-11 machine result/exits | Strict scanner envelope/schema, payload digest, output ordering, and exits 0/1/2 | `RESOLVED-SPEC`; compatibility requires implementation evidence |
| C-12 discovery boundary | Conservative Markdown/MDX/Markdown-named set; exact exclusions; no broad text inference | `RESOLVED-SPEC`, measured on user zero |
| C-13 cross-repo/environment/TTL | Known unsupported scope/capability, no network | `DEFERRED-TYPED` |
| C-14 multi-subject/partial acceptance | Invalid/unsupported schema; no partial state | `DEFERRED-TYPED` |
| C-15 signatures/reviewer proof | External trust overlay only; never inferred from Git metadata | `DEFERRED-TYPED` plus `EXTERNAL-GATE` |
| C-16 plugins/LLM/probes | No repository execution; known unsupported capability | `DEFERRED-TYPED` |

## Cross-spec consistency audit

The independent post-freeze audit found 29 additional holes. They are indexed here so none is
hidden inside a prose correction.

| ID | Audit issue | Resolution | Status |
| --- | --- | --- | --- |
| CA-01 | Directive omitted core `reference` | RFC adds `reference` and restricts `path-exists` to it | `RESOLVED-SPEC` |
| CA-02 | `path-exists` lacked authored file/tree kind | `artifact=repository-file|repository-tree` is mandatory | `RESOLVED-SPEC` |
| CA-03 | Simple URI could not build a canonical selector | RFC defines the exact subject and `target` endpoint expansions, schemas, projection, cardinality, scope, and path semantics | `RESOLVED-SPEC` |
| CA-04 | Named checks had no root wire schema | `check=<id>` is recognized but emits `unsupported-named-check-schema` and no definition/state | `DEFERRED-TYPED` |
| CA-05 | `generated-from` bound prose as generated output | The simple shape is rejected; future schema must name the real output and hermetic validator | `REJECTED`/`DEFERRED-TYPED` |
| CA-06 | `constrains` lacked completion constructor | Simple shape rejected; future named schema must choose validator or acceptance | `DEFERRED-TYPED` |
| CA-07 | `historical-at` omitted acceptance | RFC records the full core requirement and rejects the unsupported simple shape | `DEFERRED-TYPED` |
| CA-08 | Simple directives lacked scope | Exact fixed `candidate-tree/self`; other scope syntax invalid in A-001 | `RESOLVED-SPEC` |
| CA-09 | Directive lifecycle disagreed with core | Migrate-before-accept, relation retire/create, and two-stage retirement now match core | `RESOLVED-SPEC` |
| CA-10 | RFC resurrected automatic refresh | Replaced by read-only scan/extraction; refresh remains unsupported | `REJECTED` |
| CA-11 | Canonical syntax was both mandatory and optional | A-001 makes canonical spelling mandatory and defines LF/CRLF behavior | `RESOLVED-SPEC` |
| CA-12 | RFC appeared to enable deferred relations | Only simple `reference` and `describes` may expand after adapter gates; others are typed unsupported | `RESOLVED-SPEC` |
| CA-13 | Scanner handling of directives was unspecified | Any reserved definition emits `unsupported-capability: governed-claim`; enforce exits 2; no RFC expansion | `RESOLVED-SPEC` |
| CA-14 | Scanner/security document scope disagreed | Security delegates the exact set to scanner; other formats are outside scope unless requested | `RESOLVED-SPEC` |
| CA-15 | Scanner said “no waiver” while consuming external waiver | Current disposable CLI has no waiver lane and always reports none; the strict digest-bound waiver schema is reserved for a future required-wrapper request contract | `RESOLVED-SPEC` / `DEFERRED-TYPED` |
| CA-16 | Attribution order was inconsistent | Core reserves ordered `improved`/`worsened` for future kinds, but scanner v0 defines no order and omits them; unequal same-key facts are `unknown`, while debt mismatch is separately `debt-worsened` | `RESOLVED-SPEC` |
| CA-17 | Observation/review/trust enums conflicted | `automatic` is observation provenance; `repository-reviewed` is context; neither is acceptance trust | `RESOLVED-SPEC` |
| CA-18 | Acceptance validity was conflated with trust | Structural `acceptance=current` derives first; trust sufficiency gates completion separately | `RESOLVED-SPEC` |
| CA-19 | Minimum trust for a blocking narrative was open | Blocking narrative requires at least `provider-verified`; self-asserted is report-only | `RESOLVED-SPEC` plus `EXTERNAL-GATE` |
| CA-20 | CAS terminology lost one predecessor | Both `previous_record_seal` and `predecessor_acceptance_seal` are mandatory and checked | `RESOLVED-SPEC` |
| CA-21 | Service introduced a second state authority | Receipt is a trust overlay on one committed acceptance; it never advances state/CAS | `RESOLVED-SPEC` |
| CA-22 | Retirement authority differed | Core two-stage lifecycle is structural; protected policy separately checks trust/authority | `RESOLVED-SPEC` |
| CA-23 | Storage was simultaneously frozen/gated | Logical state is normative; per-claim layout is the explicit X-06 candidate and not stable before Gate B | `RESOLVED-SPEC` plus `BLOCKED-EVIDENCE` |
| CA-24 | CLI usage could expose a fourth exit | All usage/capability errors map to exit 2 | `RESOLVED-SPEC` |
| CA-25 | Report digest was self-referential | Envelope digests only the strict `payload` under `assure/scanner-report-payload/v1` | `RESOLVED-SPEC` |
| CA-26 | Scanner/control digests lacked encodings/domains | Machine contracts name every strict schema, JCS/HB/HJ domain, and out-of-band binding | `RESOLVED-SPEC` |
| CA-27 | Duplicate occurrence key could transfer debt | Exact occurrence context is defined; indistinguishable duplicates are debt/waiver-ineligible | `RESOLVED-SPEC` |
| CA-28 | Debt worsening had no ordering | V0 debt tolerates exact accepted fact-digest equality or resolution only; any other value fails | `RESOLVED-SPEC` |
| CA-29 | Trusted time contradicted deterministic output | `evaluation_instant` is an explicit supplied validity input; determinism is over the full tuple | `RESOLVED-SPEC` |

### Missing implementation boundaries found by the same audit

| ID | Missing boundary | Resolution | Status |
| --- | --- | --- | --- |
| MB-01 | Stable scanner JSON Schema | Published under `spec/scanner-report-v1.schema.json` with strict envelope/payload | `RESOLVED-SPEC`; implementation compatibility untested |
| MB-02 | Repository policy filename/schema/law | Exact `.assure/scanner-policy.json`, raise-only schema, canonical digest, and comparison law | `RESOLVED-SPEC` |
| MB-03 | Floor/debt/waiver wire and trust binding | Separate schemas/digests avoid cycles; expected digests arrive out of candidate control | `RESOLVED-SPEC` |
| MB-04 | Heading/frontmatter/HTML parsing was vague | `frontmatter-v1` is exact; heading anchors and raw HTML are unsupported by default | `RESOLVED-SPEC`/`DEFERRED-TYPED` |
| MB-05 | GitHub URL ref splitting was ambiguous for `/` | Decode once, test both candidate/default trusted-ref segment sequences, require one distinct split, and resolve only a candidate-ref match; default-only/other versions are unsupported | `RESOLVED-SPEC` |
| MB-06 | Plain-text/probable-path lexer unspecified | Removed from stable v0; only dossier research artifacts retain non-contract inference data, with no product command | `REJECTED` |
| MB-07 | Finding names diverged | Machine contracts publish one closed v0 taxonomy; prose aliases are non-machine historical wording | `RESOLVED-SPEC` |
| MB-08 | Provider receipt signature/replay format missing | Provider/service trust remains unsupported until a separate receipt RFC fixes payload, verifier, revocation, and replay | `DEFERRED-TYPED`/`EXTERNAL-GATE` |

## Final machine-contract audit FCA-01 through FCA-120

| ID | Audit defect | Closure | Status |
| --- | --- | --- | --- |
| FCA-01 | Report example retained removed engine/adapter fields and stale digests | Example now uses action provenance and full adapter descriptors; adapter, sandbox, finding-fact, observation, and payload digests recompute in the smoke checker | `RESOLVED-SPEC`; production conformance open |
| FCA-02 | Synthetic snapshot digest had no canonical preimage | `IndexProjectionInput`/`IndexProjectionEntry` fix the complete ordered logical index and Git OIDs; the small `SyntheticSnapshotInput` binds base plus projection digest without a policy/parser selection cycle | `RESOLVED-SPEC`; X-04 executable fixtures open |
| FCA-03 | Finding key/fact digests were opaque | Report embeds discriminated key and base/candidate fact preimages; debt/waiver embed exact structural fact bodies | `RESOLVED-SPEC`; negative implementation fixtures open |
| FCA-04 | Observation IDs were opaque | Occurrences embed exact adapter/address/projection/intent `ObservationIdInput`; repeated fields and digest equality are mandatory | `RESOLVED-SPEC` |
| FCA-05 | Sandbox and trusted-time provenance were absent | Report carries strict sandbox descriptor/digest/assurance/enforcement source and time source/trust source | `RESOLVED-SPEC`; provider verification remains X-07 |
| FCA-06 | Kind/class/profile/default and policy traces were underconstrained | Closed built-in table, coverage variants, trace order/adjacency, configured/effective boundary, exception law, and record-only resolved projection are normative | `RESOLVED-SPEC` |
| FCA-07 | Adapter IDs/descriptors could disagree | Exact three-adapter ordered set, compatibility matrix, ID equality, and descriptor digest law are fixed | `RESOLVED-SPEC`; real parser compatibility open |
| FCA-08 | Action/build provenance digests had no value shapes or linkage | Release manifest, build namespace, lock domain, platform artifact, raw checksum, engine version/digest, and action tree bindings are fixed | `RESOLVED-SPEC`; release/provider tests open |
| FCA-09 | Source-construct enums diverged | Report uses the exact nine Markdown link/image variants; v0 has no runtime adapter-candidate extension; GitHub is target intent | `RESOLVED-SPEC` |
| FCA-10 | Multi-candidate waiver bundles poisoned unrelated runs | Validate the complete bundle, require top-level repository/ref/floor binding, and treat only items for other candidate trees as inactive; selected-item defects are explicit | `RESOLVED-SPEC` |
| FCA-11 | Global/nonrepresentable-path failures required a valid repository path | Finding locations permit explicit `global`/null path; every within-limit Git path outside `RepoPath` retains bounded raw byte hex in analysis evidence | `RESOLVED-SPEC` |
| FCA-12 | Summary could not count every attribution and exception application | Added `not_applicable`; debt/waiver totals derive from mutually exclusive application objects/steps; unsupported inference is absent from stable v0 | `RESOLVED-SPEC` |
| FCA-13 | CI invented merge-group attribution and gated only introduced failures | Ordinary exact-base attribution plus `event_kind` is used; every candidate structural failure blocks except exact active external debt/waiver | `RESOLVED-SPEC` |
| FCA-14 | Timestamp regex accepted impossible calendar values | Lexical ranges are narrowed; strict Gregorian parsing, temporal ordering, and negative fixtures are product requirements | `RESOLVED-SPEC`; implementation fixtures open |
| FCA-15 | Line/column units were unspecified | Raw spans are zero-based half-open bytes; display positions are one-based Unicode scalars after newline normalization | `RESOLVED-SPEC` |
| FCA-16 | Local validator overstated schema conformance | It is labeled a parsed-value smoke checker only; the canonical report-wire golden is checked, while Gate A evidence still requires strict JSON, Draft 2020-12, cross-field, ordering, and raw-byte negative suites | `RESOLVED-SPEC`; Gate A evidence open |
| FCA-17 | `RepoPath` lookaheads could be bypassed across a newline | All five schemas use newline-safe lookaheads; regression vectors cover newline-hidden `..`, backslash, NUL, duplicate slash, and absolute paths | `RESOLVED-SPEC` plus smoke regression |
| FCA-18 | Published report arrays and floor resource names did not encode documented ceilings | Report unions now have exact `maxItems`; the floor can tighten per-snapshot document and retained-error ceilings with matching resource IDs | `RESOLVED-SPEC`; limit fixtures open |
| FCA-19 | Public CLI and provider wrapper responsibilities were conflated | The only public surface is the exact in-process CLI; stable wrapper request roots/framing remain blocked on a separate RFC before E3 | `RESOLVED-SPEC` / `DEFERRED-TYPED` |
| FCA-20 | First-run/self-comparison and candidate-only evaluation could manufacture a baseline | Every run requires an event-authorized, distinct base and candidate; self-comparison and candidate-only success are invalid | `RESOLVED-SPEC` |
| FCA-21 | Analysis failures competed with finding taxonomy and aliases | One closed uppercase `AnalysisErrorCode` enum covers incomplete evaluation; errors never receive finding identity, policy, debt, or waiver | `RESOLVED-SPEC` |
| FCA-22 | Correlation, aggregation, coverage, and exact rename behavior left producer choices | Full counterpart arrays, one-per-key aggregation, complete observation IDs/multiplicities, exact unique raw/mode rename edges, and the closed impact table are fixed | `RESOLVED-SPEC`; mutation fixtures open |
| FCA-23 | A pinned action could execute unverified metadata/runtime helpers | Required workflows acquire the action tree as data; restricted JSON action metadata, closed runtime manifest, bootstrap digest, direct exec, and platform equality are mandatory | `RESOLVED-SPEC`; X-07 release/provider proof open |
| FCA-24 | Time/sandbox constraints confused deterministic facts with host kills | Trusted-time statements bind evaluation/run identity; sandbox mechanism/platform/descriptor/constraint are bound; 120 s/1 GiB are wrapper kill limits, not semantic errors | `RESOLVED-SPEC`; platform proof X-05/X-07 open |
| FCA-25 | External exception/control application lacked exact state and defect preimages | Control-state inputs, source multiplicities, complete debt/waiver diagnostics, overlap order, fail-to-warn-only waiver law, and application counts are closed | `RESOLVED-SPEC`; hostile fixtures open |
| FCA-26 | Acceptance provenance could be mistaken for current evaluation status | Receipt persistence distinguishes the candidate that introduced an acceptance from every later candidate whose status is independently rechecked | `RESOLVED-SPEC`; provider receipt RFC still closed |
| FCA-27 | X-06/X-08 and governed physical layout formed a circular prerequisite | A disposable in-memory X-08 harness requires X-01..05/X-07 but not X-06; only positive durable-obligation evidence opens physical-layout X-06 | `RESOLVED-SPEC` |
| FCA-28 | LFS pointers could be guessed or treated as resolved content | The bounded current/legacy grammar accepts sorted extension lines and defensive CRLF, has exact byte/integer vectors, preserves raw digest/availability, and never runs attributes/smudge/fetch | `RESOLVED-SPEC`; X-04 object fixtures open |
| FCA-29 | Frontmatter/MDX/HTML accounting could overlap or exceed document bytes | BOM, disjoint-region precedence, per-document sums, and exact region/byte totals are fixed | `RESOLVED-SPEC` |
| FCA-30 | Worktree alias, special-file, kind, and mode derivation depended on host guesses | Worktree mode is removed from the CLI/root report shapes and returns `INVALID_INVOCATION`; a separate RFC must close admin/nested-repo, D/F, mount/reparse, Windows-name, alias/hardlink, ignore-work, race, and failure-wire semantics | `REJECTED` / `DEFERRED-TYPED`; X-04 cannot reopen it without that RFC |
| FCA-31 | `gitignore-v1` was an unnamed future grammar whose bytes/resources were outside snapshot identity | The 26 vectors/60 Git 2.47.3 outcomes remain research inputs, but no matcher or ignore preimage is part of v0 after worktree rejection | `DEFERRED-TYPED`; exact oracle plus work-budget RFC required |
| FCA-32 | Valid Git names outside `RepoPath` made complete output impossible | Raw byte length is checked first; over-limit names use `raw-path-bytes`, while every within-limit invalid UTF-8/backslash/grammar name uses `UNREPRESENTABLE_PATH` plus full hex | `RESOLVED-SPEC` |
| FCA-33 | Failed snapshot acquisition could not inhabit `UnavailableSnapshot`, and reasons had no error law | The reason enum covers every commit/index acquisition family; a reason/anchor table, derivative-stage law, dedup rule, and path/resource cardinalities are normative; worktree reasons were removed with the mode | `RESOLVED-SPEC`; negative fixtures open |
| FCA-34 | Governed-definition aggregation could exceed its 4,096-source representation | Reserved definitions consume the ordinary reference budgets before state construction, proving each path-scoped source array is representable | `RESOLVED-SPEC` |
| FCA-35 | Ref validity inherited mutable installed-Git behavior | `ref-format-v1` directly freezes prefix, byte bound, and all ten ordinary-ref prohibitions; 19 boundary vectors are smoke-checked | `RESOLVED-SPEC` |
| FCA-36 | A valid external URI scheme longer than 32 characters had no result shape | `external_scheme` now inherits the already-bounded raw-destination maximum instead of an unrelated 32-character cap | `RESOLVED-SPEC` |
| FCA-37 | Floor/debt/waiver provenance allowed an action artifact to masquerade as external trust | `immutable-action` is removed; verified semantic controls permit only external-required-workflow or organization-ruleset sources | `RESOLVED-SPEC` |
| FCA-38 | Engine version and platform prose claimed more provenance than encoded | Action reports bind engine version to the reviewed manifest; local version is display-only; platform binds process target OS/ISA, while ABI/physical-host/emulation compatibility remains X-05 evidence | `RESOLVED-SPEC` / `BLOCKED-EVIDENCE` |
| FCA-39 | Canonical report output was required but had no checked byte fixture | The one-line JCS-plus-LF report golden exists and the smoke checker proves semantic equality and exact bytes distinct from the indented example | `RESOLVED-SPEC`; strict-wrapper rejection test remains E0 |
| FCA-40 | The post-review ledger stopped before later audit findings | FCA-19..40 record every subsequent contract, trust, worktree, path, failure-envelope, resource, and fixture closure without relabeling evidence gates as passes | `RESOLVED-SPEC` |
| FCA-41 | The report example counted no non-document path while resolving `src/example.scala` | `outside_document_set` is 1; payload/canonical digests were regenerated and the smoke checker asserts the partition | `RESOLVED-SPEC` |
| FCA-42 | Worktree complexity kept generating filesystem-dependent producer choices | The authorized CLI/schema were narrowed to commit-pair and staged-index; worktree returns `INVALID_INVOCATION` and every audit blocker is a named RFC/X-04 reopen condition | `REJECTED` / `DEFERRED-TYPED` |
| FCA-43 | Fatal incomplete reports could retain an implementation-selected safe prefix | Fatal-incomplete output clears all detail arrays/non-error counts; unsupported-only boundary-incomplete output finishes and retains the exact full bounded scan | `RESOLVED-SPEC`; negative fixtures open |
| FCA-44 | Error overflow made required reason anchors impossible and used an arbitrary lower bound | A logical error set is formed first; overflow keeps the lowest `E-1` plus a sentinel at exactly `E+1`, which explicitly substitutes for omitted anchors, including `E=1` | `RESOLVED-SPEC`; overflow fixtures open |
| FCA-45 | Resource `observed_lower_bound`, membership, path, and caching could vary by implementation | Count/per-value/aggregate/memory/output laws, complete logical-tree nodes, per-path document/control charging, per-object target charging, ordering, activation, and every resource path/null row are exact | `RESOLVED-SPEC`; limit fixtures open |
| FCA-46 | A 1 MiB floor could make the error envelope larger than its own output budget | The floor cannot tighten the 64 MiB machine-wire ceiling; E0 must prove a worst-case escaped release-manifest/error golden fits | `RESOLVED-SPEC`; maximal-shape evidence open |
| FCA-47 | Arbitrary externally protected control blobs had no hashing budget | Added synchronized per-blob 16 MiB and aggregate 64 MiB resources, checked from Git headers before raw hashing | `RESOLVED-SPEC`; boundary fixtures open |
| FCA-48 | Schemas allowed direct promotion of advisory `document-removed` | Removed it from repository/floor promotable enums; protected inventory instead creates the separate blocking `coverage-reduced` fact | `RESOLVED-SPEC` |
| FCA-49 | Reused debt IDs collided in control rule identity and defect construction order contradicted wire order | Debt IDs and finding keys are globally unique with debt-ID ordering; debt/waiver tables define construction order while wire/human output remains finding-key canonical | `RESOLVED-SPEC` |
| FCA-50 | Document and Resolution ADTs allowed optional bytes/fields for the same fact | Closed tables now fix every document unsupported reason and every resolution status/code field, digest, mode, path, and availability combination | `RESOLVED-SPEC`; mutation fixtures open |
| FCA-51 | Reference summary buckets lacked exact predicates | Candidate primary/alternative occurrence population, intent buckets, outcome buckets, overlap, and the complete outcome equation are normative | `RESOLVED-SPEC` |
| FCA-52 | Provider destination refs could be confused with merge-queue head refs or checkout state | PR, merge-group, push, and default-branch refs have exact authenticated constructors; merge groups use `base_ref`, never `head_ref`, and controls bind the result | `RESOLVED-SPEC`; X-07 events open |
| FCA-53 | V0 prose still implied an inference command and invented unsupported-capability subtypes | Only the exact check CLI exists; extra options are `INVALID_INVOCATION`, unknown policy fields are configuration errors, and `unsupported/governed-claim` is the sole v0 capability rule ID | `RESOLVED-SPEC` |
| FCA-54 | Future core claimed layout neutrality while mandating per-file paths/locks/history and separate records | ClaimKey is a logical key; report/storage/path/history and every physical locking/atomicity/recovery/sharding law are conditional on the X-06-selected storage RFC | `RESOLVED-SPEC` |
| FCA-55 | Local unavailable values could not compute request digests without the deferred wrapper framing | In-process CLI unavailable request digests are exactly null; non-null stream digests remain blocked with the future request-wire RFC | `RESOLVED-SPEC` / `DEFERRED-TYPED` |
| FCA-56 | Unavailable reason sets lacked the schema's ordering annotation | All three carry `enum-declaration-order`; prose names it as the explicit non-field ordering rule | `RESOLVED-SPEC` |
| FCA-57 | Staged-index object/content/materialization choices were not fully canonical | Every complete `IndexProjectionEntry` uses the exact Git OID/mode/kind/skip bit in a prefix-free logical surface; raw SHA-256 appears only in later evidence facts, and the small snapshot preimage binds the projection | `RESOLVED-SPEC`; X-04 fixtures open |
| FCA-58 | Configuration specifics and trusted-time errors were optional diagnostics | Fatal stage order is fixed; aggregate configuration anchors plus every safely established specific code are mandatory; trusted-time failure has one exact null-path trigger set | `RESOLVED-SPEC`; hostile fixtures open |
| FCA-59 | Floor hard-limit conditionals were not strict-type closed or exercised | Both conditional branches carry integer types; the example exercises 64 MiB/64 errors; negative mutations and strict Draft 2020-12 compilation pass | `RESOLVED-SPEC`; independent product parser evidence remains Gate A |
| FCA-60 | The issue ledger used statuses absent from its own legend | Every row now uses only the five declared closure states; explanatory qualifiers remain prose | `RESOLVED-SPEC` |
| FCA-61 | Index skip counts and fatal candidates were unrepresentable, and evidence selection made snapshot construction cyclic | Resolved index counts derive from the complete projection; unavailable index uses zero sentinels; snapshot identity is constructed before policy/parsing from the complete OID projection | `RESOLVED-SPEC`; X-04 fixtures open |
| FCA-62 | Scanner prose invented an already-existing three-stream provider API | Disposable CLI is in-process and all external controls are none; request domains are reservations only; stable interop waits for root schemas/framing RFC | `RESOLVED-SPEC` / `DEFERRED-TYPED` |
| FCA-63 | Candidate-first diagnostics contradicted fatal base-first order and empty fatal details | One base-before-candidate stage order applies; any fatal stops later stages and clears detail arrays, even for a repair PR | `RESOLVED-SPEC`; fault fixtures open |
| FCA-64 | Gitignore/history/generated/similarity hints and raw-HTML occurrences had no output model | V0 computes none of those hints/rankings, extracts no raw-HTML occurrence, and reports only opaque-region counts | `RESOLVED-SPEC` |
| FCA-65 | Valid extended/legacy LFS pointers were misclassified as ordinary content | Current and documented legacy versions, sorted unknown extensions, LF/defensive CRLF, 1,023-byte and signed-size bounds are frozen in 15 ordered cases | `RESOLVED-SPEC` |
| FCA-66 | A tree intent aimed at an LFS-pointer blob had no legal Resolution | Exact incompatible-pointer type-mismatch row retains pointer digest/availability and emits only structural mismatch; compatible pointers retain the separate content boundary | `RESOLVED-SPEC` |
| FCA-67 | Digest preimages were unreachable schema defs and candidate identity lacked a schema | Three fragment URIs are advertised roots; index/snapshot/commit/index-candidate examples and hardcoded HJ goldens are smoke-checked | `RESOLVED-SPEC` |
| FCA-68 | `--repo` left bare/linked/nested/admin/alternate discovery choices open in authorized modes | V0 accepts only a directly named primary non-bare root with no-follow `.git` handles and primary objects/index; every rejected shape has one Git error family | `RESOLVED-SPEC` / `BLOCKED-EVIDENCE`; X-04 platform fixtures open |
| FCA-69 | Document classification, side presence, base discovery, exclusion override, and binary sniffing were producer choices | Exact five-value precedence, per-side selection/status, base/candidate union, candidate-only denominator, no content sniff, and include-over-exclusion law are fixed | `RESOLVED-SPEC` |
| FCA-70 | Native target kinds, component digest bytes, CommonMark preprocessing, empty destinations, and Resolution precedence were open | Exact source-token/semantic transforms, raw versus pre-percent component digests, target-kind constructors, empty-self rule, and one terminal precedence are fixed | `RESOLVED-SPEC`; parser fixtures open |
| FCA-71 | GitHub URLs without identity and Unicode/slashed refs could classify differently | No identity/foreign owner is exact external; exact lowercase authority is required; URL segments decode once before unique trusted-ref split with slash/%/Unicode vectors | `RESOLVED-SPEC`; provider event evidence remains `BLOCKED-EVIDENCE` X-07 |
| FCA-72 | Line-fragment recognition and document/code applicability were vague | Positive bounded `L…[-L…]` grammar, same-repository blob-only exemption, and exact document/code/tree result mapping are frozen | `RESOLVED-SPEC` |
| FCA-73 | Opaque region counts, adapter/count equalities, unused definitions, and unreachable unknown states lacked a wire law | Maximal interval unions, zero equivalences, occurrence count equality, no unused-definition field, and removal of unreachable document/target unknown enums close the ADTs | `RESOLVED-SPEC` |
| FCA-74 | An unchanged source projection with a changed AST address became a false subject change | One-to-one same-document pairs use the new unchanged-projection reason and equal source state; address churn alone changes only ObservationId | `RESOLVED-SPEC` |
| FCA-75 | Taxonomy existed without a complete document/occurrence/comparison-to-finding projection | Candidate boundary map, structural base/candidate grouping, resolved deletion projections, removal/correlation/impact triggers, and exact multiplicities are closed | `RESOLVED-SPEC`; mutation fixtures open |
| FCA-76 | Parse/Git error codes overlapped and `GIT_REF_INVALID` was unreachable | Mutually exclusive source/parser/span and Git missing/kind/corruption/index constructors are ordered; unreachable code/reason were removed | `RESOLVED-SPEC`; hostile fixtures open |
| FCA-77 | Tree-bound waivers/debt had undefined staged-index selection | Index mode requires debt/waiver none; staged exceptions need a new discriminated candidate-control schema | `RESOLVED-SPEC` / `DEFERRED-TYPED` |
| FCA-78 | Debt could disappear after parser drift and its historical report digest was not obtainable at runtime | Snapshot records the creation-time report audit digest; current engine must reproduce every adoption fact; required wrapper must boundedly pre-acquire adoption objects | `RESOLVED-SPEC` / `DEFERRED-TYPED`; shallow acquisition waits for request-wire/X-07 |
| FCA-79 | Resource activation, member identity, memoization, and error path could change pass/fail or wire | Late floor activation, fatal serializer reserve, logical trie/document/control/object charging, total order, and a per-resource path table extend FCA-45 | `RESOLVED-SPEC`; maximal/boundary fixtures open |
| FCA-80 | Repository policy/protected controls/inventory and control-scope paths lacked object/state constructors | Regular-policy blob rule, protected descriptor digest, inventory state/rule/source table, and exact rule-to-control-path table are fixed | `RESOLVED-SPEC`; attack fixtures open |
| FCA-81 | Closed schema arrays had stale headroom, release cardinality wording conflicted, and `.dotfiles` was invalid | Maxima equal closed key counts, releases list one-to-six unique platforms, and all identity schemas share punctuation-leading repository grammar/vector | `RESOLVED-SPEC` |
| FCA-82 | CLI option order, malformed format output, and human log escaping were not deterministic/safe | Options are order-independent; malformed format has fixed empty-stdout/error line; human is non-wire but uses exact ASCII atom escaping, bounds, channels, and fact order | `RESOLVED-SPEC`; hostile CLI fixtures open |
| FCA-83 | A green required status could outlive time, base, waiver/debt, floor, constraint, or revocation | Pre-publication expiry/digest recheck is exact; required enforcement is closed until a control-epoch RFC proves merge-time equality and invalidation/rerun | `DEFERRED-TYPED` / `BLOCKED-EVIDENCE` / `EXTERNAL-GATE` X-07 |
| FCA-84 | Candidate-SHA concurrency and optional PR-head hints lost base/control identity or invented facts | Cancellation requires the complete evaluation/control/release tuple; PR-head attribution is absent until a provider request-wire defines out-of-band metadata | `RESOLVED-SPEC` / `DEFERRED-TYPED` |
| FCA-85 | Future core imposed current ledger reports, directive errors, meta kinds, paths, files, and Git-history layout | Current scanner delegates only to machine-contracts; future governed report/kinds/storage are explicitly schema/gate/X-06 conditional | `RESOLVED-SPEC` |
| FCA-86 | Mutable/misstated sources and shrinkable oracle corpora undermined reproduction | CommonMark/LFS are pinned, RFC 3986 and GitHub freshness/name evidence are logged, Gitignore has tracked/untracked commands, and LFS/reference IDs are exact | `RESOLVED-SPEC`; external oracle reruns remain evidence work |
| FCA-87 | The 109-document E1 criterion invalidated itself when this dossier was added | E1 first replays exact recorded commit `1e31df…`, then separately reports/explains the current-tree denominator | `RESOLVED-SPEC` |
| FCA-88 | Duplicated schemas and new fragment fixtures could drift silently | Smoke checks normalized paths, repository/tree identity, source constructs, debt/waiver structural shapes, classification levels, index roots, `.dotfiles`, and exact digest chains | `RESOLVED-SPEC`; full independent conformance remains `BLOCKED-EVIDENCE` Gate A |
| FCA-89 | The deployment checklist still authorized a required status without the request wire, control epoch, or exact externally owned workflow source | Shadow mode is point-in-time only; stable enforcement requires the request-wire RFC, merge-time freshness/invalidation proof, and an active organization/enterprise exact-source ruleset workflow or proven equivalent; status name/expected app alone is rejected | `DEFERRED-TYPED` / `BLOCKED-EVIDENCE` / `EXTERNAL-GATE` X-07 |
| FCA-90 | The Gitignore oracle's transient repositories raced concurrent recursive dossier validation | The oracle now creates every disposable repository beneath the OS temporary directory, outside the dossier traversal root; concurrent validators are a required regression check | `RESOLVED-SPEC` |
| FCA-91 | E1 called recorded commit OID `1e31df…` a tree OID and omitted the mandatory base/identity tuple | E1 now publishes the exact SHA-1 base/candidate/repository/ref/profile command; GitHub-only literal owner/repo components ASCII-fold for the mixed-case recorded URLs; current-tree discovery remains separate | `RESOLVED-SPEC` |
| FCA-92 | A named required status could still come from mutable or candidate-controlled workflow content, or survive a changed control epoch | Stable enforcement now requires the provider request wire, merge-time freshness/invalidation, and exact organization/enterprise ruleset source repository, path/ref, full commit, workflow blob, and immutable dependency closure; names/apps alone never authorize | `DEFERRED-TYPED` / `BLOCKED-EVIDENCE` / `EXTERNAL-GATE` X-07 |
| FCA-93 | Acquisition needs network/credentials while the evaluator forbids both; skipped/neutral/conditional jobs could appear green | The trusted acquisition phase is minimal and closes handles/credentials/network before direct evaluator exec; the protected job is unconditional and only an accepted passing envelope maps to success, never skipped/neutral/continued failure | `RESOLVED-SPEC`; provider proof remains `BLOCKED-EVIDENCE` X-07 |
| FCA-94 | Parser labels did not select a closed grammar, autolink constructors were incomplete, and a partial corpus could silently define semantics | `commonmark-gfm-v1` and `mdx-source-v1` pin exact CommonMark/remark/MDX versions, options, precedence, and autolink transforms; parser integration waits for the complete nonshrinkable profile corpus | `RESOLVED-SPEC` / `BLOCKED-EVIDENCE` Gate A |
| FCA-95 | Parser node/depth limits, frontmatter handling, and structural addresses could vary by implementation or admit unreachable plain occurrences | `parser-work-accounting-v1` fixes post-frontmatter oracle input, node/depth counts, hostile-body opacity, plain synthetic accounting, syntax-node paths, zero reserved indices, and a 255-member address maximum; plain emits no occurrence/address | `RESOLVED-SPEC`; exact corpus goldens remain `BLOCKED-EVIDENCE` Gate A |
| FCA-96 | URI query/fragment order, autolink bytes, GitHub identity/ref splitting, missing paths, and invalid/default-only paths left TargetIntent or Resolution non-total | Exact first-`#`/first-pre-`#`-`?` components, syntax-specific autolink bytes, literal GitHub identity fold, single decode, nonempty validated remaining RepoPath, and total intent/version rows are fixed and exercised by 38 vectors | `RESOLVED-SPEC`; provider event evidence remains X-07 |
| FCA-97 | TargetIntent admitted constructor-impossible target kinds and examples encoded a native link as `blob` | Schema conditions and cross-field validation require ordinary native links→`either`, exact terminal-slash links→`tree`, native images→`blob` with image terminal slash invalid, no native autolink repository path, and GitHub `/blob/`/`/tree/`→`blob`/`tree`; report/debt/waiver digests and canonical wire were regenerated | `RESOLVED-SPEC` |
| FCA-98 | Correlation could compare raw unions inconsistently or advertise an unreachable native/GitHub equivalence | `CorrelationIntentV1` is an exact per-kind projection; the reachable image/blob equivalence and seven distinguishing cases are executable vectors; unchanged byte-equal projections may use the closed advisory reason | `RESOLVED-SPEC`; full mutation suite open |
| FCA-99 | Unchanged document sides ignored entry identity; finding locations/source ownership admitted impossible spans or plain/HTML observations | `DocumentSide.entry_oid` participates in equality; side/kind-specific location constructors, exact owner precedence, no source excerpts, and markdown/MDX-only occurrence ADTs close those shapes | `RESOLVED-SPEC`; negative producer fixtures open |
| FCA-100 | Reserved definitions had no exact candidate-node/span/digest/multiplicity law, and a losing reserved duplicate could suppress an ordinary first winner | Every candidate definition node is tested before label normalization, exact source bytes are domain-hashed and grouped, only a reserved first CommonMark winner suppresses its consumer, and six vectors cover case/entity/duplicate/base-only/precedence behavior | `RESOLVED-SPEC`; complete parser corpus open |
| FCA-101 | Git object acquisition depended on installed Git and left loose headers, duplicate packs, pack names/checksums, tree/commit grammar, and resource charging open | `primary-object-db-v1` fixes no-follow primary-only lookup, loose-first failure, raw pack order, exact headers/names/checksums/deltas, tree/commit bodies, lazy object kinds, and bounded object/pack/index/depth resources | `RESOLVED-SPEC`; X-04 hostile fixtures open |
| FCA-102 | Staged-index identity trusted a semantic projection without a closed raw parser and mishandled split/sparse/gitlink rows or unrelated blobs | `git-index-v1` pins Git 2.44 v2/v3/v4 bytes, rejects split/sparse indexes, checks raw-before/after equality, validates all blob/symlink rows, never opens gitlinks, and charges the exact raw-index resource | `RESOLVED-SPEC`; X-04 fixtures open |
| FCA-103 | SHA-1 OIDs were described as provenance-only and an unspecified collision detector could change pass/fail | SHA-1 object preimages use the exact Git-v2.44-pinned SHA1DC commit/config and collision corpus; alarms are object-unreadable, ordinary metadata checks retain their own codes, and stable SHA-1 authorization additionally authenticates a canonical SHA-256 digest of the complete evaluated object-preimage closure, not only its top tree | `RESOLVED-SPEC` / `DEFERRED-TYPED`; implementation and X-07 proof open |
| FCA-104 | Huge decimal object sizes could overflow a host integer or race the Git cap against smaller document/target/control caps | Size is arbitrary-precision decimal; the smallest applicable contextual cap is checked digitwise before allocation/conversion, exactly one resource wins, and only within-limit bodies reach declared-length validation | `RESOLVED-SPEC`; boundary fixtures open |
| FCA-105 | Fatal attribution could invent `unknown` facts, local runs implied waivers/owners, and diagnostics could leak repository text | Base-first fatal acquisition clears details; `unknown` requires two available unequal facts; local controls have no waiver/owner lane; location summaries contain no excerpts or raw destinations | `RESOLVED-SPEC`; hostile diagnostics open |
| FCA-106 | GitHub branch spellings such as `HEAD`/40-hex conflicted with exact trusted refs, and default-only invalid paths could downgrade to version boundaries | Exact supplied branch refs win regardless of ambiguous-looking spelling; empty/invalid matched remainders fail before version classification, while only a valid default-only remainder becomes unsupported scope | `RESOLVED-SPEC`; vectors cover empty and traversal cases |
| FCA-107 | New vector files and the Gitignore oracle were absent from the experiment manifest and concurrent validation could race | The manifest records 38 reference, eight correlation, nine frontmatter, six governed, 19 ref, 15 LFS, and 26 ignore cases; disposable Git repos live outside the traversed dossier and concurrent validation is required | `RESOLVED-SPEC` |
| FCA-108 | Readiness language authorized parser/evaluator implementation despite the missing full parser corpus | Only CLI/schema/Git-acquisition scaffolding and the conformance harness may start; parser integration/evaluator work remains explicitly `BLOCKED-EVIDENCE` until extraction/span/address/node/depth corpus goldens exist | `RESOLVED-SPEC` / `BLOCKED-EVIDENCE` Gate A |
| FCA-109 | A value-taking CLI option could consume the next `--option`, changing defect classification and output channel | Only a following token not beginning `--` is a value; option-shaped tokens parse independently, lone `--` and attached `--name=value` are unknown, and E0 must cover their permutations | `RESOLVED-SPEC`; argv fixtures open |
| FCA-110 | Resource ceilings were labeled only for a blocking profile even though `observe` is the current runnable mode | One scanner-v0 engine ceiling set and fatal charging law now applies identically to `observe` and `enforce`; only a future verified floor may tighten it and local v0 has no floor | `RESOLVED-SPEC`; boundary fixtures open |
| FCA-111 | The executable frontmatter recognizer accepted only LF even though the contract normalizes CRLF and bare CR | The helper now uses the exact CRLF/bare-CR/LF line scanner and nine vectors cover raw-byte ends for all three newline forms | `RESOLVED-SPEC` |
| FCA-112 | CLI argv/path encoding was undefined across POSIX byte strings and Windows UTF-16 | Every token must losslessly represent Unicode scalars; invalid UTF-8/unpaired surrogates are invocation errors, lexical checks use canonical UTF-8, and `--repo` opens the exact unnormalized native encoding relative to captured startup CWD | `RESOLVED-SPEC`; cross-platform argv fixtures open |
| FCA-113 | Observation IDs allowed a Markdown adapter with an MDX address kind and vice versa | Schema conditionals and the smoke cross-field validator bind each occurrence adapter to its exact address kind; the adapter descriptor matrix is likewise schema-refined | `RESOLVED-SPEC` |
| FCA-114 | Compressed deflate padding, unlimited pack-directory junk, and cumulative pack-index reads could reach only the nondeterministic watchdog | New per-stream, aggregate-compressed, all-directory-entry, and aggregate-index resources have exact caps, order, paths, and fixtures; both report/floor enums remain synchronized | `RESOLVED-SPEC`; X-04/X-05 hostile fixtures open |
| FCA-115 | Rereading a held index fd missed Git's atomic rename-over update | The final sample reopens the current no-follow `.git/index` directory entry and independently bounds/parses it; every invalid/different final sample is solely `GIT_SNAPSHOT_CHANGED`, while byte-identical replacement is accepted | `RESOLVED-SPEC`; atomic-race fixtures open |
| FCA-116 | A PR candidate with base/head first plus extra parents satisfied the synthetic-merge check | Pull-request candidates now require exactly two parents in the authenticated order; an octopus-shaped candidate is `INVALID_EVENT` | `RESOLVED-SPEC`; X-07 fixture open |
| FCA-117 | The written GFM profile omitted pinned `remark-gfm` footnotes/single-tilde behavior and mislabeled tagfilter as a parse transform | The exact `remark-gfm@4.0.1` pipeline uses `{singleTilde:true}`, names footnotes/single tilde explicitly, treats footnote refs/definitions precisely, runs no tagfilter renderer, and requires all CommonMark/GFM/plugin/MDX examples with full AST-derived goldens | `RESOLVED-SPEC` / `BLOCKED-EVIDENCE` Gate A |
| FCA-118 | The frozen RepoPath law turned user zero's valid `docs/content/docs/` directory link into an extra false positive | One native link terminal slash constructs a canonical tree intent; image and GitHub `/blob/` terminal slashes are invalid while GitHub `/tree/` permits it, four vectors cover the law, and E1 requires the known directory link to resolve | `RESOLVED-SPEC` |
| FCA-119 | One error-constructor paragraph limited `UNSUPPORTED_CAPABILITY` to governed definitions despite requested/protected unsupported documents | Every unsupported finding with non-`none` coverage now contributes one path-deduplicated boundary-incomplete error while retaining full findings; v0 has no path-null request surface | `RESOLVED-SPEC`; requested-format fixtures open |
| FCA-120 | Git traversal could reject/under-count a valid shared-subtree DAG, and pack-directory absence/nonordinary states were not total | Cycle detection is current-ancestor-only; shared subtrees expand per logical path; absent pack dir is an empty set/object-missing, present nonordinary/unreadable is object-unreadable, and only `.`/`..` pseudoentries escape the all-entry cap | `RESOLVED-SPEC`; X-04 fixtures open |

## Build gates A through D

### Gate A: before the discard-state scanner

| Requirement | Paper closure | Evidence closure |
| --- | --- | --- |
| Exact document/reference classes | Scanner candidate set and reference classes are closed; measured conservative set is 109 files | User-zero discovery measurement exists; the complete parser conformance corpus is absent and cross-repo evidence is X-02 |
| Candidate/index inputs | Exact commit and staged-index snapshot contracts exist; worktree is rejected | Index/object X-04 fixtures are open; worktree needs a separate RFC |
| No source evaluation and bounded resources | Threat model and hard ceilings exist | Fuzz/adversarial/platform envelope X-03/X-05 are open |
| Parser/Git errors cannot erase findings | Small error envelope, completeness flag, and exit 2 are fixed | Fault-injection implementation tests are open |
| Experimental output and no writes | Scanner authorization forbids every write/state command | Repository integrity tests are open |

Decision: the original Gate A is **closed on paper sufficiently to implement disposable
CLI/schema/Git-acquisition scaffolding and the conformance harness**. Parser integration and the
evaluator remain `BLOCKED-EVIDENCE` until the complete profile corpus exists; the gate is also not
evidence-closed for a stable API or required CI job.

### Gate B: before persisted state

| Requirement | Current closure |
| --- | --- |
| Ledger value/size/churn measured | User-zero size, writer pressure, and conflict data are partial; X-01/X-06 remain |
| Observation distinct from attestation | `RESOLVED-SPEC` |
| Canonical encoding/domains/vectors | Primitive contract exists; full state schema/vector/platform suite remains X-06 |
| Refresh cannot mutate governed state | Refresh is rejected entirely |
| Migration plan for persisted versions | Logical engine/lifecycle migrations exist; physical version migration depends on X-06 |

Decision: Gate B is **closed to implementation** (`BLOCKED-EVIDENCE`).

### Gate C: before governed claims

| Requirement | Current closure |
| --- | --- |
| Stable ID and directive renderer/parser matrix | ID/RFC exist; X-03 remains open |
| Separate definition/observation/acceptance/lifecycle/policy/trust | `RESOLVED-SPEC` |
| Delete/move/retarget/retire/split/merge/reuse transitions tested | Specified, not implemented; X-08 |
| Honest local trust and ownership expectations | `RESOLVED-SPEC`; provider proof remains external |
| Base-diff meta-findings | Specified, not attack-tested |

Decision: Gate C is **closed to stable/public governed claims**. After the explicitly listed X-08
entry conditions in [implementation-readiness.md](./implementation-readiness.md) pass, a disposable,
report-only governed harness may be built to run X-08. X-08 passing closes Gate C; it is not a
precondition for building its own test harness.

### Gate D: before required narrative enforcement

| Requirement | Current closure |
| --- | --- |
| Exact final candidate and real queue tests | Contract exists; X-07 open |
| Same-claim CAS after rebase | Contract exists; executable/provider test open |
| Protected owners and approval | `EXTERNAL-GATE`; absent in current pilot |
| Exact-equality-or-resolution adoption debt | Contract exists; administrative rollout input absent |
| Pre-registered shadow thresholds | `EXTERNAL-GATE` X-02 |
| Every adversarial test passes | No implementation suite yet |
| Parser/unsupported/timeout/truncation fail closed | Contract exists; X-03/X-05/X-07 open |

Decision: Gate D is **firmly closed**.

## Empirical gates X-01 through X-08

| ID | Required evidence | Current state | Consequence |
| --- | --- | --- | --- |
| X-01 exact historical graph replay | Per-revision graph, class labels, lineage ambiguity, seeded mutations, all known drift validators | Partial surviving-graph and five structural histories only | Inference stays outside stable v0; no durable observation claim |
| X-02 external prospective shadow | User zero plus 2–3 unaffiliated repositories, all-finding review, clean/unlinked audits, pre-registered thresholds | Not run | No required scanner class or general market precision claim |
| X-03 parser/renderer/directive | Exact GitHub/MDX/linter matrix, no visible output/execution, fuzz and limits | Partial local parser matrix | Directive adapters disabled; raw HTML/anchors/fences unsupported |
| X-04 Git index/object modes plus worktree RFC | Clean/divergent/staged/conflict/symlink/submodule/skip-worktree/sparse-directory-and-split-index rejection/LFS/path/object-format fixtures; separately, every listed cross-platform worktree blocker | Not run | Index/local API and required CI remain unproven; worktree stays unavailable regardless of index evidence |
| X-05 production resource envelope | Cold Linux/macOS/Windows, larger corpora, adversarial limits, compact output | Warm user-zero only: p95 4.875 s, about 176 MiB RSS, 1.95 MiB verbose JSON | No production latency/memory promise |
| X-06 serializer/layout | Actual schema across a large repo, ordinary one-claim acceptance, closed split/merge lifecycle transactions, same-claim CAS/concurrency, and filesystem/review/platform costs | Synthetic sizing; global JSONL conflicts 0%/18%/99% for 1/5/20 disjoint updates; per-claim only conditionally clean | No stable state storage or writer |
| X-07 real CI events | Same-repo/fork/moving-base/merge-group/push/shallow/error events and exact status source | Static workflow audit; zero merge-group workflows | No required CI or provider trust |
| X-08 governed pilot | Real stable claims, deterministic rules, all lifecycle/policy/concurrency transitions and review burden | Not run | No governed state/directives/narrative gate |

The exact methods, denominators, raw artifacts, and pass/fail rules are in
[preimpl-experiments.md](./preimpl-experiments.md). No local substitute is claimed for X-02 or
X-07.

## Red-team scenarios RT-01 through RT-18

All scenarios have a normative expected result. None is counted as an executable pass before an
implementation exists.

| ID | Scenario | Required result | Evidence status |
| --- | --- | --- | --- |
| RT-01 | Target changes; unrelated typo in claim | Still `review-required` | Specified; historical evidence supports it; test open |
| RT-02 | New inferred block | Observation only, never accepted | Specified; stable v0 inference removed |
| RT-03 | Contributor hand-edits wrong digest | Canonical/transition verification fails | Specified; test open |
| RT-04 | Contributor performs valid local acceptance | Current but `self-asserted`; cannot satisfy blocking narrative trust | Specified; pilot open |
| RT-05 | Failing claim/document deleted | Unsuppressible removal/coverage failure | Specified; test open |
| RT-06 | Exact-content target moves | Suggestion only; explicit migration | Specified; test open |
| RT-07 | One claim splits | Closed successor mapping, tombstone, no inherited acceptance | Specified; test open |
| RT-08 | Two PRs accept same predecessor | Second fails record/acceptance CAS | Specified; test open |
| RT-09 | Earlier queue item changes dependency | Final candidate invalidates later acceptance | Specified; X-07/X-08 open |
| RT-10 | Candidate lowers a valid policy or adds an unknown exclusion field | A valid weakening emits unsuppressible `policy-weakened`; an unknown exact-policy field is `UNKNOWN_FIELD`/`CONFIGURATION_INVALID`, exit 2; unrelated files have no control effect | Specified; attack test open |
| RT-11 | Candidate marks live docs historical | `scope-weakened` | Specified; test open |
| RT-12 | Engine implementation changes, same contract/result | Provenance only; not a doc edit | Specified; test open |
| RT-13 | Engine contract changes resolution | `engine-migration-required`; no baseline | Specified; test open |
| RT-14 | Versioned page lacks scope mapping | `scope-unresolved`; no current-tree inference | Specified; test open |
| RT-15 | Parser unsupported or limit hit | Typed unsupported/error, never clean | Specified; X-03/X-05 open |
| RT-16 | Zero governed claims | Zero governed coverage, not “in sync” | Specified; test open |
| RT-17 | Refresh sees orphan | Refresh is unsupported and writes nothing | Superseded by stronger rejection |
| RT-18 | State names its enclosing commit | Invalid validity field/self-reference | Specified; test open |

## Normative invariant families

Core section 17 assigns all 44 executable invariant IDs. This table enumerates every family and
retains the evidence gate; prose conformance is not recorded as a test pass.

| Family | Exact IDs | Contract location | Current evidence |
| --- | --- | --- | --- |
| Identity | `INV-ID-001..003` | Core identifiers/lifecycle | Not implemented |
| Observation | `INV-OBS-001` | Core observation identity | Historical support; no product test |
| Acceptance | `INV-ATT-001..007` | Core acceptance/trust/derivation | Not implemented |
| State/CAS | `INV-STATE-001..005` | Core hashing/storage/CAS; CI operations | Merge simulation partial; no serializer |
| Lifecycle | `INV-LIFE-001..007` | Core legal transitions | Not implemented |
| Engine migration | `INV-ENG-001..004` | Core engine compatibility | Not implemented |
| Scope | `INV-SCOPE-001..002` | Core scope/deferred behavior | Not implemented |
| Policy | `INV-POL-001..003` | Core/CI policy and waivers | Not implemented |
| CI | `INV-CI-001..003` | CI exact candidates/fail-closed behavior | Static audit only; X-07 open |
| Coverage | `INV-COV-001..002` | Core/scanner output honesty | Schema exists; no implementation |
| Hashing | `INV-HASH-001..003` | Core domains/vectors/state | Seed vectors checked locally; full state suite open |
| Security | `INV-SEC-001` | CI untrusted input/object kinds | Not implemented |
| CLI | `INV-CLI-001..003` | Core command state machine | Not implemented |

## OP-01 through OP-30 disposition

The old problems register proposed several mechanisms that the later review rejected. These rows
prevent their word “resolved” from being mistaken for current authorization.

| IDs | Final disposition |
| --- | --- |
| OP-01, OP-19, OP-22 | Automatic refresh/bot writer is rejected; v0 is stateless and governed state changes are explicit reviewed transitions |
| OP-02, OP-20 | Trust-on-edit/content identity is rejected; block projections are observation versions, while governed continuity uses `ClaimId` |
| OP-03, OP-23, OP-24, OP-27, OP-28 | Probable/bare/generated path heuristics are removed from stable v0; research inference artifacts cannot become a command, gate, persisted fact, debt match, or stable report field |
| OP-04 | Formatter immunity is not promised; v0 target impact uses raw bytes/mode and reports formatting impact honestly |
| OP-05 | Governed identity is authored; relocation needs explicit migration, never hash retargeting |
| OP-06 | Exact URI/path rules replace fallback guessing, history rescue, gitignore classification, and backslash normalization |
| OP-07 | Fence semantics require a conformance-tested adapter; otherwise advisory/unsupported |
| OP-08 | Source block ownership is parser-defined; frontmatter is exact; HTML is opaque; sections are reporting only |
| OP-09 | Adjacent skip/local downgrade is rejected |
| OP-10, OP-25 | External digest-protected floor plus raise-only repository policy; candidate content cannot weaken the checker |
| OP-11 | Old TOML/last-match/glob law is superseded by strict machine input schemas and exact set semantics |
| OP-12, OP-21 | Exact final-candidate invariants and explicit debt replace attribution exceptions; CAS handles same-claim concurrency |
| OP-13 | PR comments/SARIF are deferred; JSON/human output only |
| OP-14 | Privileged comment/parser and bypass App are rejected from v0; provider service remains separately gated |
| OP-15 | Bulk/file/target acceptance is rejected; one claim per `accept`, except closed split/merge lifecycle transactions |
| OP-16, OP-26 | Captured-value selectors are deferred; no fragile literal rebinding exists in v0 |
| OP-17 | Broad prose extensions are narrowed by measurement; submodule/materialization/translation semantics are typed unsupported |
| OP-18 | Market moat claim is corrected; direct competitors require build-vs-extend evidence |
| OP-29 | User-zero size/conflict pressure is measured; large-repository physical storage remains X-06 |
| OP-30 | Its consistency claim is superseded by this matrix and the current normative precedence |

## Market, adoption, and legal gates

| Issue | Required closure | Status |
| --- | --- | --- |
| Direct OSS overlap | Run the frozen corpus/build-vs-extend comparison against Fiberplane `drift`, `ryanwaits/drift`, and existing deterministic checks | `BLOCKED-EVIDENCE` |
| Willingness to maintain claims | Multiple unaffiliated contributors create/migrate/accept/retire without researcher operation | `EXTERNAL-GATE` X-08 |
| Preference for durable carried obligations | X-08 records whether pilot users choose, service, and retain carried review obligations over stateless per-change enforcement within pre-registered burden and conflict budgets | `EXTERNAL-GATE` X-08/user behavior |
| Agent-instruction wedge | X-02 shadow teams or design partners record concrete stale-instruction incidents or behavior degradation and retain the enabled check after evaluating its findings | `EXTERNAL-GATE` X-02/design partner |
| Willingness to pay/buyer | Paid pilot or design-partner commitment tied to measured outcomes | `EXTERNAL-GATE` |
| Patent/status/FTO | Counsel retrieves the official file and maintenance history; checks continuations, foreign family, assignment, expiration, and term adjustment; charts claims against the proposed workflows; and records the product decision and any design-around outside this dossier before a public commercial pilot | `EXTERNAL-GATE`; no legal conclusion in this dossier |

## Closure conclusion

Every identified issue now has an owner and terminal treatment: a normative rule, an explicit
rejection, a typed unsupported result, a falsifiable evidence gate, or an external authority. No
remaining item is an implicit implementation choice.

That does **not** make the whole product ready. It makes one narrow action safe now: implement the
CLI/schema/Git-acquisition scaffold and conformance harness, then close the parser-profile corpus.
Only after that corpus passes may the commit-pair/staged-index parser/evaluator experiment proceed
to produce evidence for X-02, the index/object portion of X-04, X-05, and X-07. Worktree mode,
persisted state, enabled governed directives, provider trust, required narrative gating, and
commercialization remain closed until their rows change through recorded evidence.
