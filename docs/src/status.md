# Project status

This page describes the supported surface on `main`, not the history of individual
releases. Versions and release-specific changes live in the
[changelog](https://github.com/HardMax71/amiss/blob/main/CHANGELOG.md). Future work and its
entry conditions live in the [Roadmap](roadmap.md).

## Supported surface

| Area | Current contract | Implementation anchor |
| --- | --- | --- |
| Command | `amiss check` compares a base commit with either a candidate commit or the staged index. The command grammar is closed. | [CLI parser](https://github.com/HardMax71/amiss/blob/main/crates/amiss/src/invocation.rs) |
| Repository access | The engine reads Git objects, packs, deltas, trees, and the index directly. It does not invoke `git`, follow repository symlinks, or fetch missing data. | [Git store](https://github.com/HardMax71/amiss/blob/main/crates/amiss-git/src/repo.rs) |
| Documents | Built-in discovery covers Markdown, GFM, MDX, six extensionless Markdown basenames, and two plain-advisory basenames. Repository policy may add paths without installing another parser. | [Classifier](https://github.com/HardMax71/amiss/blob/main/crates/amiss-scan/src/document.rs) |
| References | Relative repository paths and same-repository GitHub, GitLab, and Gitea-family URLs are resolved under their declared dialect. Numeric line fragments select and compare an exact inclusive byte range; unsupported and external shapes remain visible in the report. | [Resolver](https://github.com/HardMax71/amiss/blob/main/crates/amiss-scan/src/resolve.rs) |
| Policy | `.amiss/scanner-policy.json` may expand discovery and raise the disposition of missing targets, target-type mismatches, and invalid references. It cannot downgrade or suppress a finding. | [Policy application](https://github.com/HardMax71/amiss/blob/main/crates/amiss-scan/src/policy.rs) |
| Reports | Machine output uses the rolling pre-1.0 report envelope and payload contract. Exact findings remain the evidence surface; engine-grouped Fix, Check, and Existing feedback is its review projection. The compatibility marker remains `experimental`. | [Current schema](https://github.com/HardMax71/amiss/blob/main/spec/scanner-report.schema.json) |
| GitHub convenience Action | A source-tag dispatcher selects the same version's immutable runtime tree. The runtime derives snapshots from supported GitHub events, verifies the selected engine against its manifest, shows at most ten grouped items, and annotates only displayed Fixes. It is not a provider-authenticated controller adapter or an independent trust boundary. | [Dispatcher](https://github.com/HardMax71/amiss/blob/main/action.yml) and [runtime](https://github.com/HardMax71/amiss/blob/main/crates/amiss/action/runtime.yml) |

Repository form is deliberately closed too. The reader accepts a primary non-bare checkout
whose `.git` entry is a real directory. Bare repositories and linked worktrees represented
by a `.git` file are unavailable, and alternate object stores are not consulted. The
[repository boundary](https://github.com/HardMax71/amiss/blob/main/crates/amiss-git/src/repo.rs)
and its [boundary tests](https://github.com/HardMax71/amiss/blob/main/crates/amiss-git/tests/boundary.rs)
pin that behavior.

The supported reference surface is intentionally smaller than "every path-like phrase in
prose." Bare filenames in ordinary text are not inferred; raw HTML and MDX code regions are
opaque; leading-slash site routes, heading fragments, code symbols, live URLs, and references
to other repositories are not validated under those systems' semantics. Their visible
boundary behavior is described in [Discovery](discovery.md) and [Resolution](resolution.md).

## Built, but not a supported delivery lane

The repository contains strict parsers and canonical writers for evaluation, snapshot, and
external-control requests, plus evaluation logic for organization floors, adoption debt,
waivers, trusted time, and execution constraints. The evaluation identity separates the
candidate ref used for same-repository URL resolution from the protected target ref used by
the branch-scoped floor, trusted-time, debt, and waiver gates. The public command still
supplies all five controls as absent and has no target-ref option; its repository and
candidate-ref fields remain caller assertions. The
`forge` field selects a URL dialect, not an authenticated provider. Compare the
[request schemas](https://github.com/HardMax71/amiss/blob/main/spec/scanner-evaluation-request.schema.json),
[strict parsers](https://github.com/HardMax71/amiss/blob/main/crates/amiss-wire/src/requests.rs),
[pipeline shell](https://github.com/HardMax71/amiss/blob/main/crates/amiss-scan/src/pipeline.rs), and
[CLI wiring](https://github.com/HardMax71/amiss/blob/main/crates/amiss/src/main.rs).

`amiss-bootstrap` now has a sealed engine path. It bounded-captures the three request files,
requires their canonical forms, a complete repository/dialect/ref identity, and coherent
commit-pair materialization, matches the embedded execution constraint and trusted-time
provider/run tuple, checks that both requested commits were pre-acquired, validates the action
tree and runtime closure, and then sends only a closed evaluation/snapshot/controls frame over
stdin to the verified engine. The child receives the repository as its fixed working directory,
a cleared environment, one private engine argument, and no caller-selected engine command.
Report acceptance rejects an unavailable hybrid and
binds the requested profile, both commits, candidate and target refs, candidate identity,
provider run and trusted instant, the exact presence, digest, and trust source of the organization
floor, debt snapshot, and waiver bundle, and the execution constraint's digest, trust source,
and recomputed semantics. It likewise recomputes the trusted-time statement's semantic digest and
requires the sandbox provenance to remain self-asserted. The wire crate now exposes checked
constructors and canonical writers for the execution constraint and trusted-time statement, so a
future controller need not reproduce those encoders. The path is built by the release workflow,
but the published composite Action still launches `amiss` directly; nothing public
acquires these inputs from an authenticated provider. The distinction is visible in the
[bootstrap entry point](https://github.com/HardMax71/amiss/blob/main/crates/amiss-bootstrap/src/main.rs),
[release assembly](https://github.com/HardMax71/amiss/blob/main/.github/workflows/release.yml), and
[Action execution](https://github.com/HardMax71/amiss/blob/main/crates/amiss/action/runtime.yml).

The separate nested Rust workspace under
[`controller/`](https://github.com/HardMax71/amiss/tree/main/controller) is also implemented only
as a transport-neutral foundation. It defines provider-neutral identities and the adapter,
durable-record, runner, and orchestration boundaries described in
[Controller delivery](controller.md). Its provider-neutral
[`FileLedger`](https://github.com/HardMax71/amiss/blob/main/controller/src/file_ledger.rs) gives
that boundary a cross-process, durable local file record without SQL or a database. The root has a
fixed record cap, fixed maintenance, admission, and clock locks, at most 256 row-lock shards, and
one state and report path per admitted delivery. Checksummed root metadata fixes the replay window
and keeps a high-water clock. Cleanup removes dead files and only bounded completed rows after their
authenticated lifetime ends; it retains running work, saved results, and permanent exact-body
replay markers. A full root rejects new identities rather than evicting them.

The same workspace has a [bounded ingress contract](https://github.com/HardMax71/amiss/blob/main/controller/src/ingress/policy.rs)
and separate GitHub, GitLab Standard Webhooks, and Gitea-family HMAC verifiers with rotating,
revocable in-memory anchors. A future GitLab route must require its authenticated timestamp to be
fresh. GitHub and Gitea-family routes instead key replay protection by the exact signed body and
use permanent completion markers. No adapter or route loader enforces these pairings yet. The
workspace deliberately has no provider enum and no concrete provider adapter,
HTTP listener, authenticated payload decoder, provider API client, credential store, repository
acquisition worker, bootstrap runner, deployable binary, publication transport, or provider
status publisher. These absences make it an internal foundation, not a supported delivery lane.

Local and convenience-Action reports consequently describe repository policy with no
external authority consulted. Each external control has status `none`, and the sandbox
assurance is `self-asserted`; there is no aggregate `provider_verified` report field. Even on
the sealed internal path, a control row marked `verified` means that the engine accepted its
digest and identity bindings. The report does not prove who acquired that control, and neither
the bootstrap nor controller signs or augments the engine's report with provider evidence. No
provider-authenticated required-check lane or provider-verified sandbox is supported. See
[Controls and policy](controls.md) for the exact interpretation.

## Keeping this page honest

Links from factual prose to the implementation are deliberate. The repository's own Amiss
scan makes a changed dependency under unchanged prose visible for review.

The mechanical claims are generated, not maintained. Default dispositions in
[Profiles and findings](profiles.md) and resource ceilings in
[Limits and refusals](limits.md) come from the Rust constants through a test, so changing
a constant without the book fails CI. The same
[documentation contract test](https://github.com/HardMax71/amiss/blob/main/crates/amiss/tests/documentation_contracts.rs)
finds every public schema-backed example, validates it against its schema, and feeds it to
its owning typed reader; a contract without a registered reader fails CI too.

The examples execute. The report's readable form passes the strict JSON reader, and its
canonical bytes clear the
[wrapper acceptance law](https://github.com/HardMax71/amiss/blob/main/crates/amiss-bootstrap/tests/acceptance.rs)
end to end. The commit and staged-index identity preimages reproduce the production digest
chain in the
[identity golden test](https://github.com/HardMax71/amiss/blob/main/crates/amiss-scan/tests/identity.rs).
The published semantic corpora drive their live code paths:
[frontmatter vectors](https://github.com/HardMax71/amiss/blob/main/spec/examples/frontmatter-vectors.json)
through the [recognizer](https://github.com/HardMax71/amiss/blob/main/crates/amiss-md/tests/frontmatter.rs),
[correlation vectors](https://github.com/HardMax71/amiss/blob/main/spec/examples/correlation-intent-vectors.json)
through the [intent projection](https://github.com/HardMax71/amiss/blob/main/crates/amiss-scan/tests/correlation_vectors.rs), and
[governed-definition vectors](https://github.com/HardMax71/amiss/blob/main/spec/examples/governed-definition-vectors.json)
through [report construction](https://github.com/HardMax71/amiss/blob/main/crates/amiss-scan/tests/governed.rs).

Published CI snippets must pin upstream Actions immutably, name an explicit reviewed crate
version, and advertise the current release major. Version strings inside example fixtures
are reproducible evidence, not claims about the latest release. None of this proves the
meaning of free prose. It makes the mechanical drift visible, which is the part a machine
can own.
