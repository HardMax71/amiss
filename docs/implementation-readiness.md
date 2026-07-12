# Implementation-readiness decision

Date: 2026-07-12.

Status: authoritative go/no-go handoff after the pre-implementation review, contract audit, market
reassessment, and user-zero experiments.

## Naming addendum (2026-07-12)

E0 is additionally blocked on the product/machine-namespace freeze. The provisional `Assure` name
has exact collisions in several software package registries and cannot ship. The existing
`assure` CLI, path, URI, schema, and hash-domain strings therefore remain research inputs rather
than public compatibility commitments. [naming-clearance.md](./naming-clearance.md) records the
screening and finds no replacement safe enough to freeze; the technically clean `DocWake` candidate
failed the final legal knockout against active `DOCSWAVE`. Implementation may begin only after a
new coined mark is selected, professionally cleared, and migrated atomically so every affected
schema example and digest vector is regenerated. This addendum does not itself authorize that
migration.

## Verdict

Proceed toward exactly one implementation: a disposable, stateless, read-only structural scanner.
At present, only CLI/schema/Git-acquisition scaffolding and the conformance harness may be coded.
Parser integration and evaluator implementation must wait for the complete checked-in
CommonMark/GFM/MDX profile corpus with exact extraction, span, address, node-count, and depth
goldens.
Do not implement persisted observations, claim state, enabled governed directives, acceptance,
refresh, executable validators, privileged provider automation, or a required narrative gate.

This is not a compromise phrasing. It is the boundary supported by the evidence:

- the current repository has two definite broken explicit references;
- explicit references are deterministic enough to test structurally;
- inline missing-path inference was actionable in only 5 of 16 rooted cases and 0 of a
  deterministic 20-case ambiguous sample;
- trust-on-edit failed in three observed containing-block edits;
- a global state file produced avoidable conflicts in 18% of disjoint five-update trials and 99%
  of twenty-update trials;
- local performance is adequate for a spike, not yet a production promise;
- real index, external-repository, fork, and merge-queue evidence does not exist; worktree mode is
  additionally blocked on an unresolved filesystem-semantics RFC.

The complete issue accounting is in
[issue-closure-matrix.md](./issue-closure-matrix.md). An item is not silently postponed: every
non-v0 capability has a typed unsupported result or a named closed gate.

## Authorized scanner boundary

The implementation may expose only:

```text
assure check --repo <path> --object-format <sha1|sha256> --base <full-oid> (--candidate <full-oid>|--index) [--repository github.com/<owner>/<name> --ref refs/heads/<name> --default-branch-ref refs/heads/<name>] --profile <observe|enforce> [--explain-scope] [--format <human|json>]
```

It must conform to [scanner-v0-spec.md](./scanner-v0-spec.md),
[machine-contracts.md](./machine-contracts.md), and
[ci-security-spec.md](./ci-security-spec.md). In particular:

- input is an exact Git commit pair or staged index snapshot;
- the candidate tree is scanned completely within the disclosed document set;
- stable reference extraction is native Markdown/MDX Markdown links, reference links, images,
  autolinks, and same-repository GitHub links only;
- raw HTML, heading-anchor semantics, site routes, fence metadata, inline paths, symbols,
  similarity, and history inference are not silently interpreted;
- source blocks and raw target changes produce observation/impact facts, never attestation;
- output uses the strict scanner report envelope and exact control inputs;
- repository content is data: no imports, plugins, commands, network, secrets, or writes;
- every engine-detected incomplete/error/deterministic-limit path emits an unaccepted error
  envelope and exits 2; watchdog/OOM/signal termination may emit nothing, and the wrapper must
  reject missing/partial output and fail the status;
- there is no `init`, `accept`, `ok`, `refresh`, `migrate`, state directory, or lockfile.

The `enforce` profile exists in the contract so the same evaluator can later become a required
structural check. It must not be registered as required until the evidence gates below pass.

## Pre-code checklist

These design decisions are complete and must be copied into tests rather than reopened ad hoc;
the parser-profile corpus is an explicit missing prerequisite, not an implied completed decision:

- document discovery and exclusions;
- path/URI normalization and Git object-kind rules;
- source adapters and opaque-region behavior;
- observation correlation and base/candidate impact table;
- closed finding taxonomy and finding-key contract;
- commit/index snapshot modes, representable path domain, `ref-format-v1`, and exact GitHub event
  tuple;
- output, observation/finding/fact, adapter/action/sandbox, digest, policy, debt, waiver, and exit
  schemas;
- resource ceilings and zero-capability budgets;
- candidate-owned policy weakening behavior;
- product language and coverage denominators.

If implementation exposes an ambiguity not covered by those artifacts, stop and amend the spec,
schema, fixture, and closure matrix together. Do not choose a convenient fallback in code.

## Implementation sequence

### E0: contract and hostile-input harness

Implement strict schema parsing/digest verification, virtual Git-tree reads, and the small error
envelope scaffolding first. Build and check in the complete parser-profile corpus next; only after
it passes against the pinned oracle may E0 integrate source spans, extraction, path resolution, or
the evaluator. Add the commit/index portion of X-04 and the scanner/CI attack matrix before
optimizing. Every command must prove repository status, index, refs, and bytes are unchanged.

The ref/canonical wire vectors now exist and are design inputs, not an E0 choice. `--worktree` MUST
return `INVALID_INVOCATION`; it is not part of E0. The authorized commit/index modes already use
the exact primary non-bare/no-follow repository form in the scanner contract. Reopening worktree
mode requires a separate RFC and X-04 fixtures that justify any broader repository form plus D/F
and skip-worktree conflicts,
ignored-rule identity, bounded ignore-matcher work, unreadable/mount/junction/reparse behavior,
Windows path encoding, alias versus hardlink semantics, race retry policy, and exact failure-wire
projection. The existing ignore vectors are research inputs to that RFC, not implementation
authorization.

The dossier's machine-contract script is only a smoke checker. E0 must independently reject
duplicate keys, invalid UTF-8/surrogates, impossible dates, noncanonical ordering, every digest and
cross-field mismatch, impossible policy traces, and malformed raw-byte inputs with a conforming
Draft 2020-12 validator plus product semantic validation.

Exit criterion: exact repeatable reports for fixtures; every analysis failure and every
requested/built-in/repository/floor-promised unsupported capability is non-green, while unrequested
boundary findings remain visible and follow their exact policy; schema examples and digest vectors
reproduce byte-for-byte.

### E1: user-zero report-only scanner

Run the implementation without installing a required workflow. First replay the recorded user-zero
candidate with this exact command from the repository root:

```text
assure check --repo . --object-format sha1 --base 236d83fb46948e7452d4cc2956d72112e3cb18f2 --candidate 1e31dfebf2bc21fe90933394e7338541eaaadaad --repository github.com/hardmax71/spec_to_rest --ref refs/heads/main --default-branch-ref refs/heads/main --profile observe --format json
```

The declared identity is mandatory: the recorded links spell the owner with mixed case, which the
closed GitHub-only ASCII component fold maps to the caller-supplied lowercase owner in this local
replay. Reproduce its measured
109-file denominator and two broken same-repository GitHub references. The known native
`docs/content/docs/` directory link must resolve to that tree under the single-terminal-slash
constructor rather than becoming a third false positive. Then scan the current tree, report its independently
derived denominator (including any committed `ci-idea/**` Markdown), and explain every discrepancy
as an exact tree or implementation difference. Keep impact facts advisory.

Exit criterion: maintainers can reproduce every native finding from exact base/candidate IDs;
random unlinked/clean-page audits do not contradict the stated boundary; output stays within the
declared cap.

### E2: external shadow and production envelope

Run X-02 and X-05 on two or three unaffiliated repositories with team-owned, pre-registered
actionability, latency, memory, and maintenance thresholds. Review all findings and random clean
samples. For agent-using teams, record concrete stale-instruction incidents or behavior degradation
before the run and whether the team retains the enabled check afterward. Run cold hosted
Linux/macOS/Windows and adversarial-size fixtures.

Exit criterion: each promoted structural class independently meets its team's threshold. A class
that misses remains report-only or is removed; results are not blended into one favorable score.

### E3: provider event sandbox

Run X-07 in a sandbox repository for same-repository PRs, forks, moving bases, actual merge groups,
default-branch pushes, shallow acquisition, and every failure condition. Bind the status to the
exact candidate. The trusted acquisition phase may use the minimum read credential/network needed
to obtain pinned workflow/action and exact Git objects; it must close handles, remove credentials,
and sever network before exec. Keep the sandboxed evaluator—not the whole acquisition job—strictly
networkless and credentialless.

Exit criterion: the separate provider request-wire RFC/root schemas/framing goldens are complete;
the trusted bootstrap/action/platform/constraint bindings verify; the exact eligible-to-merge tree
and every debt-adoption reproduction object are boundedly acquired even from shallow provider
checkouts; the candidate is always checked with explicit base attribution; there is no path filter or privileged candidate
input lane; no incomplete, missing-output, killed, or OOM run can publish success; and a separate
control-epoch/provider-freshness contract proves merge-time equality of base/candidate/control
digests plus invalidation/rerun on expiry, revocation, base movement, or control rotation. The lane
must also be an active organization/enterprise ruleset workflow bound to its exact externally owned
source repository ID, workflow path/ref, non-null full commit SHA, resolved workflow blob OID/raw
digest, and immutable dependency closure, or a tested provider-equivalent content identity;
for a SHA-1 repository the epoch also authenticates and binds a canonical independent SHA-256
digest over the complete acquired/evaluated loose-equivalent object closure—commits, traversed
trees, and selected blobs—rather than only the top tree (otherwise the required lane accepts only
SHA-256 object format).
Status-name, mutable branch/path, or expected-app matching alone is not sufficient. Only then may the structural
`enforce` profile become a required check.

## Explicitly closed implementation paths

| Capability | Why closed | Reopen condition |
| --- | --- | --- |
| Stable inline/plain-text inference | User-zero precision is too low and lexer semantics were not stable | X-01/X-02 class-specific evidence and a new schema |
| Persisted observation ledger | No measured need that exceeds stateless comparison; write/churn cost exists | Gate B plus X-01/X-06 |
| Per-claim repository state | Logical model exists but physical compatibility/filesystem costs are untested | X-06 and a stable record/vector suite |
| Enabled RFC A-001 directive | Local syntax evidence is incomplete across actual renderers/linters/fuzzing | X-03 |
| Governed acceptance/lifecycle | No real transition implementation, ownership, or burden evidence | Gate C plus X-08 |
| Blocking narrative claims | Self-assertion cannot prove reviewer identity; final-tree/provider evidence absent | Gate D, X-02, X-07, provider-verified ownership |
| Executable validators | Repository execution violates v0 threat boundary | Separate hermetic unprivileged evidence RFC and tests |
| Provider/service receipts | No canonical signature/verifier/revocation/replay RFC | Separate receipt RFC and provider sandbox |
| Stable provider wrapper/engine wire | Root evaluation/snapshot/control request schemas, handle framing, and wire goldens are absent | Separate request-wire RFC before E3/required status |
| Worktree snapshot mode | Filesystem/admin/nested-repo/path/alias/D-F/mount/reparse/ignore-work/error-wire semantics are not closed across platforms | Separate worktree RFC plus X-04; v0 returns `INVALID_INVOCATION` |
| Standalone product codebase | Direct competitors occupy the basic loop; differentiation unvalidated | Build-vs-extend pilot demonstrates a breaking high-value need |
| Commercial launch | Buyer and professional legal review absent | Design-partner evidence and counsel sign-off |

## X-08 governed-pilot entry conditions

No governed code is authorized now. A future disposable, report-only X-08 harness may begin only
when all of these entry conditions are recorded as passed, not merely planned:

1. X-01 through X-05 and X-07 meet their stated pass criteria for the selected surface. X-06 is
   explicitly excluded because a positive X-08 durable-obligation decision is its entry condition.
2. RFC A-001 passes the full renderer/parser/linter/fuzz matrix.
3. A strict logical claim-record schema and in-memory canonical vector suite exist; no durable
   repository layout is required for this disposable harness.
4. Protected document and evidence owners are resolvable.
5. X-08 pre-registers actionability and review-burden thresholds before claims are created.
6. Every pre-pilot `INV-*`, RT, and security attack fixture applicable without governed mutations
   passes.

The harness may use self-asserted local acceptance only, remains non-required, and exists to run
the lifecycle, CAS, policy, migration, and burden tests. Its state is disposable and may be held in
an isolated temporary directory or in memory; it must not choose or normalize a stable repository
layout. Gate C passes only after X-08 succeeds.
Provider receipt semantics, eligible dual-owner approval, all governed mutation invariants, and
Gate D remain additional prerequisites before any narrative result can block.

X-08 must also record whether pilot users choose, service, and retain carried review obligations
over a stateless per-change alternative while remaining within the pre-registered burden budget.
Only a positive durable-obligation decision authorizes X-06 to compare physical layouts. Passing
lifecycle fixtures or a synthetic serializer benchmark does not establish that user-behavior
hypothesis.

Failure does not force more machinery. The default fallback is to keep structural checks and add
narrow deterministic validators such as OpenAPI equality. If maintainers token-edit or bulk-review
claims, if storage cost is high, or if audits find escaped drift outside the stated denominator,
drop or narrow the governed layer.

## Product and sourcing decision

Before creating a standalone product repository, execute the comparison in
[market-reassessment.md](./market-reassessment.md): the local scanner versus Fiberplane `drift`,
`ryanwaits/drift` on a suitable TypeScript corpus, and existing deterministic mechanisms. Build a
new stateful core only if a pilot proves a high-value requirement that cannot be wrapped or added
upstream without breaking the competing architecture.

No engineering result closes the legal gate. Before a public commercial suspect-link/change-impact
pilot, counsel must retrieve the official file and maintenance history; check continuations,
foreign family, assignment, expiration, and term adjustment; chart the claims against the proposed
workflows; and record the resulting product decision and any design-around outside this dossier.

## Handoff

The mental-heavy-lifting phase has produced a bounded experiment, not permission to build the
original magical version. Implement the complete parser-profile corpus, hostile fixtures,
CLI/schema/Git-acquisition scaffold, and conformance harness first. Only after those corpus goldens
pass may parser integration and the read-only evaluator begin; measured results then earn—or
kill—each later layer.
