# Implementation feasibility for `spec_to_rest`

Status (2026-07-11): repository/runtime evidence and historical implementation sketch. The current
scanner boundary, wire formats, and authorization are
[scanner-v0-spec.md](./scanner-v0-spec.md), [machine-contracts.md](./machine-contracts.md), and
[implementation-readiness.md](./implementation-readiness.md). Earlier references here to inline
path extraction, source-bearing fences, artifacts, future lock files, or refresh automation are
not v0 authorization. A refresh actor or operation is not part of the accepted future design.

## Verdict

The proposed checker is implementable in this repository, and the repository is a good dogfood
target. It already contains enough explicit code citations and enough confirmed drift to calibrate
a useful first release. The smallest safe slice is not a semantic truth checker and does not yet
write a ledger. It is a read-only, discard-state Markdown/MDX citation resolver:

1. find explicit same-repository links, repository-rooted inline paths, and source-bearing code
   fences;
2. resolve them against the candidate Git tree without executing repository code;
3. report unambiguous broken links separately from lower-confidence inferred paths;
4. emit deterministic machine output and run on every pull request and code-only change, with no
   path filter.

The persisted contract is **not ready to implement yet**. The concurrently completed
[pre-implementation red team](./preimpl-red-team.md) and
[v0 contract review](./v0-contract-review.md) identify incompatible definitions of relationship
identity, baseline versus attestation, the now-rejected automatic-refresh proposal, policy
transitions, lock contents, and
the repeated `[assure]` directive. Parser and resolver work that discards its state is safe now.
`assure.lock`, `accept`, blocking narrative-impact policy, and a public JSON schema should wait
until one normative contract resolves those MUST decisions. This is a schema gate, not an
implementation-feasibility failure.

That slice catches real defects in the current checkout. There are 55 absolute GitHub links back
into this repository; two currently target removed files:

- `docs/content/docs/design/convention-engine.mdx:99` points at the old convention-module
  `Generator.scala`, renamed with 99% similarity to
  `modules/dafny/src/main/scala/specrest/dafny/Generator.scala`;
- the same page at line 203 points at the old convention-module `Naming.scala`, renamed with 99%
  similarity to `modules/ir/src/main/scala/specrest/ir/Naming.scala`.

The second item was not in the first repository audit, so the feasibility investigation itself
found another calibration case. The Python/Postgres page also contains two inline references to
the deleted `RouteKind.scala`. The existing link checker reports success because it treats all
`https:` URLs as external and strips inline code before scanning.

The main technical caveat is MDX. The product design's Rust choice still makes sense, but `comrak`
alone is not an MDX parser. A production implementation needs an MDX-aware front end, or a
conservative Markdown scanner that treats JSX, ESM, and expressions as opaque while preserving
source positions. This should be settled before the parser becomes a public compatibility
contract.

## Runtime and location decision

There are two different decisions: where the product belongs and how to perform a short-lived
calibration spike.

| Purpose | Recommended runtime and location | Reason |
| --- | --- | --- |
| Long-lived product | Standalone Rust workspace and released single binary, outside this repository | This repository has no Rust toolchain or Rust ownership surface. The checker is a repository-agnostic product, while this repository is a Scala application. Rust supplies the Git/ignore/tree-sitter ecosystem and fast offline startup described in [design.md](./design.md). |
| Dogfood integration | Pinned binary or commit-pinned action invoked by a new root workflow | It keeps the gate independent of sbt and of the large docs dependency installation. |
| Pre-product calibration only | A disposable Node 20 package under `tools/assure-v0/`, with its own dependency lockfile and direct MDX dependencies | Node 20 is already standard in this repository's workflows, and the exact MDX parser family is already known. Keeping a separate package avoids installing the full Next/Fumadocs tree. It must discard scan state and is a measurement harness, not the product architecture. |

Do not implement the production checker as another Scala module. The root build has useful
libraries—Circe, SnakeYAML, decline, and Cats Effect—but starting sbt and compiling the project is
the wrong latency and dependency profile for a cheap always-on repository gate. Do not put the
generic checker into `docs/scripts/` either: the checker must cover `README.md`, `CONTRIBUTING.md`,
proof documentation, workflow documentation, agent instructions, and code-only changes, not just
the Fumadocs site.

The repository-local Node contingency should have its own package rather than import hoisted
transitive packages from `docs/node_modules`. `remark-parse` and `remark-mdx` are present through
`@mdx-js/mdx`, but they are not direct dependencies in `docs/package.json`. Depending on npm's
current hoisting layout would be accidental. Conversely, running `npm ci` in `docs/` installs the
entire docs application; the current local `docs/node_modules` occupies roughly 548 MiB. That is
unreasonable for the permanent fast lane.

## Repository constraints and existing hooks

The current tracked surface is small enough that correctness matters more than optimization:

| Fact | Observed value or behavior | Consequence |
| --- | --- | --- |
| Tracked Markdown/MDX | 109 files, about 897 KiB and 15,246 lines | A full scan is cheap; incremental discovery is unnecessary at this scale. |
| Fumadocs content | 89 pages | This is the main public-doc set, but not the complete assurance scope. |
| Workflows | 22 tracked YAML workflows | A new always-on lane is consistent with the repository, but should not be folded into a path-filtered docs lane. |
| Existing link check | Pure-stdlib Node script; 90 files scanned; currently passes | Reuse its route/heading rules conceptually, but it is not a repository-reference checker. |
| Existing MDX parser | `@mdx-js/mdx` 3.1.1, `remark-parse` 11, `remark-mdx` 3.1.1 in the docs lock | Useful for a spike and as an executable compatibility oracle. The production binary cannot assume npm. |
| Existing transclusion | `remark-spec-file.ts` reads `file="..."` fences from the repository | Those four fences are already strong source-to-doc edges and should be imported, not duplicated. |
| Existing executable docs | `run-cli-snippets.mjs` and committed JSON outputs | Wrap its result as stronger evidence later; do not execute it inside the fast reference scanner. |
| Existing CI checkout | Most workflows use the checkout action's shallow default | Enough for lock-based checking; insufficient for base/candidate attribution and history-assisted rename suggestions. |
| Existing ownership | No `CODEOWNERS` file is tracked | A contributor can change code and bless the lock in one PR unless review policy protects the ledger. CI creates visibility, not independent authorization. |

The current workflows demonstrate both the integration point and the gap:

- `.github/workflows/links.yml` is dependency-free but runs only when `README.md`, docs content, or
  the checker changes. It cannot observe a code-only change that invalidates a doc.
- `.github/workflows/deploy-fly.yml` runs `npm ci`, the CLI snippet drift check, and the full docs
  build, but its path filters omit many parser, convention, profile, proof, and generator changes
  that docs describe.
- `.github/workflows/ci.yml` and `quality.yml` deliberately skip docs-only work. They are expensive
  and are not the right home for this check.
- `.github/workflows/duplication.yml` already shows the repository's pattern for a separate,
  always-on, read-only job and for fetching full history only where a diff actually needs it.

The new gate should therefore be a separate workflow. It must run on `pull_request`, `merge_group`
if merge queues are enabled, and pushes to `main`, without `paths` or `paths-ignore`.

## What the smallest vertical slice does

### Included

The first implementation should deliberately support only these inputs:

1. tracked UTF-8 `.md` and `.mdx` files, plus untracked non-ignored files in local worktree mode;
2. standard Markdown links whose URL is a same-repository GitHub `blob/<ref>/...` or
   `tree/<ref>/...` URL;
3. relative Markdown links that resolve to repository files or documentation routes;
4. inline-code values that are unambiguous repository-rooted paths;
5. fenced-code metadata with `file=` or `src=`;
6. a scope explanation and counts for scanned, excluded, unsupported, linked, and unlinked docs;
7. ephemeral text and experimental JSON output from one read-only `scan` command.

This is enough to prove discovery, path resolution, candidate-state reading, fingerprinting,
reporting, and CI wiring without freezing relationship identity or the lock schema. It has at least
three useful calibration outcomes in this checkout:

| Calibration | Expected v0 result |
| --- | --- |
| Removed `Generator.scala` absolute link | Blocking `broken-reference`, with the current renamed path offered only as a suggestion. |
| Removed `Naming.scala` absolute link | Blocking `broken-reference`, likewise with a history-assisted suggestion when history is available. |
| Two removed `RouteKind.scala` inline paths | Advisory `probable-broken-reference`; inferred references are not governed claims in the spike. |

“Blocking” in that table is the eventual built-in disposition for an explicit native link. The
first repository run should still be report-only because both defects are pre-existing debt. In an
enforcing run, every candidate broken explicit link fails unless the exact finding key and fact
digest match externally registered adoption debt; an unequal fact is not grandfathered.

The current `docs.yml` inventory claim is not a good v0 blocking case. The page establishes the
directory in one paragraph and names `docs.yml` in a later table cell; joining those fragments is
inference, and the file never existed in history. It belongs in the v1 workflow-inventory check,
not in a hand-tuned v0 heuristic.

### Explicitly excluded

The slice must not claim to detect whether prose is true. Exclude these until the basic state model
is stable:

- symbol parsing and public API projections;
- counts, inventories, stated-value extraction, and managed regions;
- generated-tree equality and OpenAPI semantic comparison;
- executable snippets, shell probes, network URLs, and LLM analysis;
- cross-repository dependencies;
- automatic retargeting or automatic edits to prose;
- persisted observations, attestations, acceptance, policy suppressions, a refresh operation, or a
  stable lock and report schema until the normative contract is approved.

The known OpenAPI copy drift is the best first v1 check, not a reason to hard-code project logic in
v0. V1 can define an equality check between
`docs/public/openapi/url_shortener.yaml` and
`fixtures/golden/codegen/python/fastapi/postgres/url_shortener/openapi.yaml`, then attach it to the
OpenAPI section. The current files have different SHA-256 digests and differ by five stale
`maxLength: 10` lines. The workflow count, proof-session graph, and target file-tree inventories
then exercise count, graph, and path-set selectors respectively.

## Markdown and MDX parsing

### What was validated

An in-memory parse using the repository's MDX parser family produced the following node behavior:

- an unused `[assure]: modules/a/F.scala#Foo` line becomes an MDAST `definition` node with its URL
  and exact line/column position;
- two `[assure]: ...` definitions under different headings are both preserved in the ordered AST,
  even though they share the normalized identifier `assure`;
- compiling that document with `@mdx-js/mdx` 3.1.1 succeeds and emits no rendered output for the
  unused definitions;
- ESM imports become `mdxjsEsm` nodes and JSX blocks become `mdxJsxFlowElement` nodes;
- fenced-code language, metadata, body, and source position are preserved.

The proposed adjacent definition mechanism is syntactically viable for this site, but the literal
repeated `[assure]` form is not ready as a governed-claim schema. Markdown reference labels have
document-wide lookup semantics, and a single path-like destination cannot carry a stable claim ID,
relation type, scope, selector version, or multiple dependencies. The red-team recommendation is a
unique label such as `[assure:expr-precedence]` plus a versioned `assure:` URI grammar. Freeze that
directive RFC before accepting declarations. Until then, the spike should extract only native
links and paths. Any experiment that reads repeated definitions must iterate nodes in source order
and must not collect them into a map keyed by identifier, because that silently collapses entries.

The bare `remark-parse` plus `remark-mdx` combination does not recognize this repository's YAML
frontmatter by itself; the opening delimiter was parsed as a thematic break in the experiment.
Fumadocs handles frontmatter as part of its wider pipeline. A standalone checker must explicitly
mask or parse the leading frontmatter before assigning definitions and blocks to sections.

### Safe parse contract

The scanner should operate on source, never on evaluated or rendered MDX:

- do not import the page;
- do not evaluate ESM, JSX expressions, component props, remark plugins, or Next configuration;
- treat ESM and JSX/expression regions as opaque, except for a small allowlist of literal
  `href`/`src` attributes if that support is intentionally added;
- preserve byte offsets and map them to line/column once, so annotations point at source;
- reject invalid UTF-8, oversized documents, excessive nesting, and parser timeouts as bounded
  analysis findings rather than crashing or hanging;
- never resolve a path outside the Git worktree after normalization or symlink handling.

This matters because MDX is executable application source, not merely Markdown with components.
Running a docs compiler in a privileged checking job would execute pull-request-controlled code.

The Rust design needs one correction at this point: use `comrak` only for genuine CommonMark.
For `.mdx`, choose and test an MDX-capable grammar or build a narrow, fuzzed lexer for the native
Markdown constructs the checker consumes. Masking arbitrary JSX with regular expressions is not
sufficient because JSX expressions can nest braces, strings, comments, and template literals.
The parser does not need to understand React semantics; it does need to avoid interpreting strings
inside MDX code as documentation citations.

### Repository-specific extraction rules

These rules should be adapters, not global guesses:

- `remark-spec-file.ts` currently treats any code node carrying `file="..."` as a transclusion and
  replaces its body. All four current uses are empty fences. The checker can classify empty fences
  as generated projections and non-empty fences as snippet/source relationships, while warning if
  that differs from the site's configured renderer.
- Fumadocs routes such as `/pipelines/concurrency` resolve under `docs/content/docs`, while static
  assets such as `/openapi/url_shortener.yaml` resolve under `docs/public`. This is different from
  repository-root path resolution and needs a docs-site resolver.
- repository paths should try document-relative resolution first for normal links, then
  repository-relative resolution for explicit repository citations. Ambiguity must be reported,
  not silently resolved.
- same-repository GitHub links are internal evidence even though they have an `https:` scheme.
  Parse owner, repository, ref, and path; do not classify all schemes as external as the current
  link checker does.
- strip display-only `:line`, `#fragment`, and query suffixes only according to a defined grammar.
  Do not trim arbitrary punctuation until a nonexistent path happens to exist.
- placeholders containing markers such as `<`, `{`, `*`, `$`, or `...` stay non-binding unless
  explicitly accepted.

A rough scan of tracked inline-code paths found 14 nonexistent literal-looking paths even after
excluding obvious ellipses. Several are deliberate historical or planned Lean/Isabelle references,
while two are the real `RouteKind.scala` drift. That is direct evidence that inferred inline paths
must begin advisory and that lifecycle rules are not optional.

## Git, worktree, and index semantics

The checker's input state must be explicit. “The repository” has at least four possible meanings:

| Mode | Required behavior |
| --- | --- |
| Default local `assure check` | Read the working tree, including modified tracked files and new non-ignored files. Omit tracked files deleted from disk. This matches what a developer sees and what ordinary build tools consume. |
| `assure check --index` | Read stage-0 blobs and the staged path set only. Never let unstaged content influence a pre-commit result. Reject unmerged stages and clearly handle intent-to-add entries. |
| Clean CI checkout | Read the candidate checkout. Because the checkout is clean, worktree and index views should produce the same relationship state. Test this invariant. |
| Base attribution | Read the base tree from Git objects and compare its finding set with the candidate finding set. The base is for “introduced by this PR” attribution, not for validity itself. |

Implementation details that should be part of the contract:

- enumerate with NUL-delimited Git output, not newline-delimited shell parsing;
- use `git ls-files --cached --others --exclude-standard` as the conceptual local candidate set,
  then account for deletions; do not use `git ls-tree HEAD` for default local mode;
- in index mode, read blobs by object ID or `:<path>` rather than opening the worktree file;
- a mode `120000` entry is a symlink blob. Do not follow it outside the worktree. A mode `160000`
  entry is a submodule commit and is not a normal directory to recurse into;
- detect sparse checkout, Git LFS pointer content, unmerged index stages, and unsupported path
  encodings explicitly;
- use Git ignore semantics for discovery, but let an explicitly tracked file win over an ignore
  rule;
- keep the tool's canonical content fingerprint independent of Git's object hash. This checkout
  uses SHA-1, while other repositories can use SHA-256;
- do not use modification times or “latest commit touching this file” as the validity key.

No tracked symlinks, submodules, sparse-checkout flags, or LFS attributes were found in this
checkout, so they are test-fixture requirements rather than current blockers.

History is optional for checking and useful for explanations. The committed lock makes shallow
offline validation possible. The current checkout action defaults are not enough for a protected
comparison: the wrapper must pass or fetch the provider-supplied immutable base and candidate
object IDs and verify their exact trees. A missing required object makes the protected evaluation
incomplete; it must not degrade to a clean or merely unattributed check.

## Manifest, lock, and authorization

The discard-state spike should be configless. It should infer citations and write no repository
state. Before any persisted beta, accept one normative answer for at least these contract points:

- artifact, selector, resolution, projection, and relationship identity;
- a fact lattice rather than one mutually exclusive “relationship status” enum;
- observation versus self-asserted acceptance versus provider-verified review;
- per-endpoint snapshots and an atomic combined seal;
- authority and invalidation direction;
- base-to-candidate transition validation, deletion, retirement, and concurrent acceptance;
- selector-engine and projection-schema migration;
- canonical SHA-256 encoding and golden hash vectors;
- policy weakening, scope changes, discovery coverage, and waiver semantics;
- normative JSON, exit codes, and document discovery boundaries.

[v0-contract-review.md](./v0-contract-review.md) proposes concrete defaults for those decisions;
[preimpl-red-team.md](./preimpl-red-team.md) explains the adversarial failures they must prevent.
The two reviews still disagree on whether a governed claim may use a content-derived relationship
instance or requires an immutable authored `ClaimId`. That conflict alone is enough to defer the
lock: changing identity later would rewrite every baseline, suppression, SARIF fingerprint, and
audit transition.

After the contract is frozen, the lock is state, not a cache. It should be:

- deterministic and sorted;
- line-oriented or otherwise merge-friendly;
- versioned by schema and fingerprint algorithm;
- written atomically only by a single standardized explicit command, recommended as `accept`;
- unchanged byte-for-byte by `check`;
- strict about duplicates, unknown required fields, and unsupported future schema versions.

A later typed-check release can add root `assure.toml` for named equality, path-set,
captured-value, and lifecycle rules. A versioned unique directive can then bind a section to one of
those centrally reviewed checks. The Node spike should not implement that surface. The Rust product
can use a strict Serde-backed TOML schema with unknown-field rejection after the schema is chosen.

There is an authorization limitation CI cannot solve alone. Because this repository has no
`CODEOWNERS`, the same PR can change implementation, run `assure accept`, and commit the new lock.
That still creates a visible review artifact, but it is not independent approval. If “docs owner
must attest” is a requirement, protect `assure.lock`, `assure.toml`, and the workflow with
`CODEOWNERS` or a repository ruleset. Never give the pull-request checking job write permission.
Do not add a post-merge refresh actor or operation. Every governed state change must remain an
explicit reviewed acceptance or lifecycle transaction; the checker is read-only.

## Commands, output, and exit behavior

The stable command surface needed for the slice is small:

```bash
assure check
assure check --index
assure check --base "$PROVIDER_BASE_OID" --candidate "$PROVIDER_CANDIDATE_OID" --format json
assure accept --claim expr-precedence
assure refs modules/dafny/src/main/scala/specrest/dafny/Generator.scala
```

`accept` must show the old and new dependency projection, reject unresolved selectors, require a
reason for unchanged-doc acceptance, and update exactly one named claim. It must not accept a file,
finding set, or every current finding. Typed `split` and `merge` lifecycle commands are the only
multi-claim transactions and do not implicitly accept their successors.

Human output should lead with source location and stable finding code:

```text
docs/content/docs/design/convention-engine.mdx:99:1 [broken-reference]
  modules/convention/src/main/scala/specrest/convention/dafny/Generator.scala does not exist
  possible rename: modules/dafny/src/main/scala/specrest/dafny/Generator.scala

2 blocking, 3 advisory, 109 documents scanned
```

GitHub format should emit annotations and a job summary. JSON is the durable machine interface;
each finding should include at least:

- schema version and stable finding code;
- relationship ID and state;
- severity and policy rule that selected it;
- document path, byte span, line, column, and section anchor;
- raw citation and normalized selector;
- resolution attempts and ambiguity candidates;
- old and current canonical fingerprints when applicable;
- base/candidate attribution status;
- suggested remediation, marked clearly as a suggestion.

Use a narrow exit contract and test it as public API:

| Exit | Meaning |
| --- | --- |
| `0` | Analysis completed and no blocking finding exists. Advisory findings may exist. |
| `1` | Analysis completed and at least one policy-blocking finding exists. |
| `2` | The check could not produce a trustworthy result because of configuration, lock, parser, Git-state, or internal analysis failure. |

Usage errors may use the CLI framework's conventional code, but they must not be confused with
“docs drifted.” Parser crashes, timeouts, and truncated output are analysis failures, not a pass.

## CI shape

The new `.github/workflows/assure.yml` should have:

- `contents: read` only;
- no path filters;
- concurrency cancellation for superseded pull-request commits but not pushes to `main`;
- a commit-pinned checkout action with credentials disabled;
- a checksum-verified, version-pinned binary or commit-pinned action;
- provider-supplied immutable base and candidate object IDs on pull requests and merge groups,
  verified before every protected evaluation;
- bounded runtime and output;
- deterministic JSON retained only in the unprivileged job workspace and human-log annotations;
- no npm, sbt, network probing, doc build, repository script execution, or lock mutation in the
  check step.

Do not merge this into `links.yml`. The existing link checker has docs-site route and anchor logic
worth preserving, and it may remain a small specialized check. The assurance workflow observes a
different dependency graph and must run when only code changes.

During rollout, configure the new job as report-only, then block only explicit broken
same-repository links. Promote explicit candidate relationships to governed one-claim acceptance
only after measuring irrelevant-trigger and unchanged-accept rates. Making every inferred file
hash blocking on the first run would create an
approval-tax machine and teach maintainers to bypass it.

## Test strategy

Tests should be layered around invariants, not around this checkout continuing to contain stale
docs forever.

### Parser and extraction tests

- fixture snapshots for Markdown, GFM tables, MDX imports, nested JSX, expressions, frontmatter,
  duplicate `[assure]` definitions, definitions in different sections, HTML links, reference
  links, escaped delimiters, code fences, and CRLF;
- assert exact source spans and section ownership;
- assert that citations inside code, ESM, and JSX expressions are not accidentally extracted;
- use `@mdx-js/mdx` on the current repository corpus as a compatibility oracle during the Node
  spike, without evaluating the compiled module;
- fuzz malformed Markdown/MDX and impose input, nesting, finding-count, and parse-time bounds.

### Resolution and fingerprint tests

- document-relative versus repository-relative paths;
- same-repository GitHub URL parsing for blob and tree links, encoded characters, anchors, refs,
  and foreign repositories;
- exact path, rename suggestion, ambiguous basename, missing generated artifact, ignored path,
  directory, and placeholder cases;
- normalization idempotence and stable ordering;
- newline, Unicode, binary, path-set, and file-mode cases;
- relationship identity after section moves, duplicate content, and duplicate references.

### Git-state integration tests

Create disposable temporary Git repositories and cover:

- clean checkout, unstaged edit, staged edit plus different unstaged edit, untracked doc, ignored
  doc, deletion, rename, and intent-to-add;
- merge-conflict index stages, symlink escape, submodule entry, sparse checkout, LFS pointer, odd
  filename, and both Git object formats where the installed Git supports them;
- shallow clone with and without a fetched base;
- equivalence of worktree and index results in a clean checkout;
- a read-only invariant that hashes repository status before and after `check`.

### End-to-end calibration

Copy minimal forms of the `Generator.scala`, `Naming.scala`, and `RouteKind.scala` examples into test
fixtures so fixes to the live docs do not delete regression coverage. Also run a non-blocking
smoke check on this repository and record the finding inventory during the observation phase.
Add the OpenAPI equality case only when the named-check layer exists. Workflow changes must pass
the repository's existing `actionlint` and `zizmor` lane.

The most important properties are:

1. `check` never writes;
2. no refresh actor or operation exists;
3. candidate-attributed findings are a subset of all candidate findings;
4. canonicalization is idempotent;
5. malformed or adversarial input cannot cause a pass by truncation, crash, or timeout.

## Cache and performance

No persistent cache is needed for this repository's v0. The complete tracked Markdown/MDX corpus
is under 1 MiB. Use per-process caches for parsed documents, path stats, Git blobs, and selector
fingerprints, then measure before adding state.

If later repositories justify persistence, put local cache data under `.git/assure/` so no
`.gitignore` edit is needed. Key it by checker version, schema version, parser version, path and
mode, plus Git blob ID or a worktree content digest. The cache must be disposable and validated;
it is never evidence and never part of `assure.lock`. Do not restore a shared trusted CI cache into
an untrusted pull-request job. A clean full scan is safer than cache poisoning at this scale.

## Relative implementation complexity

These are relative complexity estimates, not calendar promises:

| Component | Relative complexity | Main uncertainty |
| --- | --- | --- |
| Explicit same-repo link extraction and path existence | Low | Correct URL/path normalization and useful source positions. |
| Conservative inline-path inference | Medium | False positives from historical, planned, generated, and placeholder paths. |
| Worktree/index/base readers | Medium | Partial staging, unusual Git modes, shallow bases, and path encodings. |
| Deterministic lock and read-only check | Medium | Stable relationship identity and conflict-friendly migration. |
| Production Markdown/MDX front end in Rust | Medium-high | MDX grammar quality, position preservation, denial-of-service bounds, and compatibility with site syntax. |
| GitHub annotations, JSON contract, and workflow | Low-medium | Exact provider base/candidate behavior across pull requests and merge queues. |
| V1 equality, count, path-set, and captured-value checks | Medium | Canonicalization and precise policy semantics. |
| Symbol/public-shape selectors | High | Language coverage, overloads, generated code, and parser-version migrations. |
| Executable probes and external claims | High | Hermeticity, toolchain identity, secrets, network policy, and timeouts. |

The go/no-go point is after the configless slice has run in observation mode and measured inferred
relationship volume, irrelevant-trigger rate, unchanged acceptance rate, and escaped known drift.
The implementation risk is not hashing or CI wiring. It is keeping claim identity and inference
precise enough that the gate remains cheaper than the drift it prevents.

## Exact repository files a future implementation would touch

For the recommended production integration, the intended change set is narrow:

| File | Change | Phase |
| --- | --- | --- |
| `.github/workflows/assure.yml` | New always-on read-only workflow using a pinned binary/action. | V0 |
| `assure.lock` | New tool-written attestation ledger. | V0 after observation/acceptance begins |
| `CONTRIBUTING.md` | Document local check, index mode, acceptance, and review expectations. | V0 |
| `.github/CODEOWNERS` | New ownership for lock, policy, and workflow if independent approval is required. | Policy hardening |
| `assure.toml` | New named checks and lifecycle/policy rules. | V1 |
| `docs/content/docs/targets/python/fastapi/postgres.mdx` | One adjacent binding for the OpenAPI equality check, if zero-touch ledger binding is not chosen. | V1 calibration |

Fixing findings is separate from implementing the checker. The first remediation PR would update
`docs/content/docs/design/convention-engine.mdx` for the two renamed files and
`docs/content/docs/targets/python/fastapi/postgres.mdx` for `RouteKind.scala`; those edits should
not be hidden inside the checker-introduction commit.

The production path does **not** require changes to `build.sbt`, `docs/package.json`,
`docs/package-lock.json`, `docs/scripts/check-links.mjs`, or `.gitignore`. Keep cache data inside
`.git/assure/`, and keep the existing docs-specific checks independent.

If a disposable Node calibration harness is requested before the Rust binary exists, its isolated
files would be:

```text
tools/assure-v0/package.json
tools/assure-v0/package-lock.json
tools/assure-v0/src/assure.mjs
tools/assure-v0/test/assure.test.mjs
tools/assure-v0/test/fixtures/...
```

That harness should emit the same proposed JSON contract and should be deleted or replaced when the
standalone binary is integrated. It should not acquire named checks, symbol parsing, or other
product surface.

## Current worktree constraints

At investigation time:

- branch: `main`, exactly aligned with `origin/main`;
- HEAD: `1e31dfebf2bc21fe90933394e7338541eaaadaad`;
- index and tracked worktree: clean;
- only dirty state: the entire `ci-idea/` directory is untracked shared work;
- this report is therefore also untracked and cannot be reviewed with ordinary `git diff` until it
  is added to the index;
- no files outside `ci-idea/` were changed by this audit.

Do not run `git stash`, reset, clean, or any hypothetical state-writing discovery command while the
shared dossier is untracked. Scanner v0 deliberately exposes no such initialization operation. A
future checker test should use a temporary Git
fixture or an explicit scope. Its default local discovery mode would otherwise include all
non-ignored `ci-idea/*.md` files, which is correct product behavior but would contaminate a
calibration run performed while several agents are still writing the dossier.

## Reproduction notes

The central observations can be reproduced without building the project:

```bash
git status --short
git ls-files '*.md' '*.mdx' | wc -l
git ls-files '.github/workflows/*.yml' '.github/workflows/*.yaml' | wc -l
node docs/scripts/check-links.mjs
sha256sum docs/public/openapi/url_shortener.yaml \
  fixtures/golden/codegen/python/fastapi/postgres/url_shortener/openapi.yaml
diff -u docs/public/openapi/url_shortener.yaml \
  fixtures/golden/codegen/python/fastapi/postgres/url_shortener/openapi.yaml
git log --follow --name-status -- \
  modules/convention/src/main/scala/specrest/convention/Naming.scala
git log --follow --name-status -- \
  modules/convention/src/main/scala/specrest/convention/dafny/Generator.scala
```

Observed outputs relevant to the feasibility decision were 109 tracked Markdown/MDX files, 22
workflows, a passing current link check over 90 files, two broken absolute same-repository links,
and unequal OpenAPI digests. The parser experiment used only in-memory input and made no repository
changes.
