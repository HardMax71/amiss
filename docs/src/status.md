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
| GitHub provider lane | A source-built App service authenticates pull-request events, refreshes exact state and strict App-bound rules, acquires exact SHA-1 objects, runs the sealed bootstrap, and publishes on GitHub's test-merge commit. GitHub.com and compatible GHES releases are supported. | [GitHub setup](provider-github.md) |
| GitLab provider lane | A source-built service authenticates a pipeline execution policy job through OIDC, verifies its enforced merge train, policy origin, runner, project, and exact train commit, then lets only an exact pass make that job succeed. GitLab 19.3 or newer with Ultimate is supported. | [GitLab setup](provider-gitlab.md) |
| Gitea and Forgejo provider lane | A source-built service authenticates pull-request webhooks, refreshes the effective protected-branch rule, acquires exact SHA-1 objects, runs the sealed bootstrap, and approves or rejects through one dedicated reviewer. Gitea 1.27 or newer and Forgejo 16 or newer are supported. | [Gitea and Forgejo setup](provider-gitea.md) |

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

## Trust surfaces

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
requires the sandbox provenance to remain self-asserted. The wire crate exposes checked
constructors and canonical writers for the execution constraint and trusted-time statement, and
the controller uses them when it derives a sealed job. The path is built by the release workflow.
The published composite Action still launches `amiss` directly, while the separately operated
provider services acquire their inputs and invoke the sealed path. The distinction is visible in
the
[bootstrap entry point](https://github.com/HardMax71/amiss/blob/main/crates/amiss-bootstrap/src/main.rs),
[release assembly](https://github.com/HardMax71/amiss/blob/main/.github/workflows/release.yml), and
[Action execution](https://github.com/HardMax71/amiss/blob/main/crates/amiss/action/runtime.yml).

The separate nested Rust workspace under
[`controller/`](https://github.com/HardMax71/amiss/tree/main/controller) defines the
provider-neutral identities, bounded ingress, rotating verifier keys, durable raw inbox,
delivery record, worker, orchestration, acquisition, and supervised runner contracts. Its state
uses checksummed ordinary files with fixed capacity and atomic replacement, not SQL or a
database. [Controller delivery](controller.md) records the cross-process ownership, heartbeat,
replay, and exact-publication retry rules.

The provider crates add signed-input decoders, controller-owned credentials and API clients,
strict merge-rule authorization, fixed-budget protocol-v2 Git acquisition, and provider evidence.
GitHub uses an App-bound required Check Run on the test merge. GitLab uses an independently owned
pipeline execution policy job on an enforced merge train and authenticates that job through OIDC.
Gitea and Forgejo use a protected approval restricted to one dedicated reviewer. The workspace
keeps those adapters separate instead of turning provider differences into one closed provider
enum. [Provider-verified controls](provider-controls.md) compares the supported lanes and links
their exact setup, retry limits, and trust boundaries.

Local and convenience-Action reports still describe repository policy with no outside authority
consulted. Their external controls are absent and sandbox assurance is `self-asserted`. A report
produced through a provider lane can contain verified external controls, but the report does not
authenticate who supplied them and gains no `provider_verified` field or signature. Provider
evidence lives in the App-owned Check Run, protected policy job, or dedicated review and the
matching merge rule. See
[Controls and policy](controls.md) and [The report](report.md) for the exact distinction.

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
