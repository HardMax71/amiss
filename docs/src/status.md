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
| References | Relative repository paths and same-repository GitHub, GitLab, and Gitea-family URLs are resolved under their declared dialect. Unsupported and external shapes remain visible in the report. | [Resolver](https://github.com/HardMax71/amiss/blob/main/crates/amiss-scan/src/resolve.rs) |
| Policy | `.amiss/scanner-policy.json` may expand discovery and raise the disposition of missing targets, target-type mismatches, and invalid references. It cannot downgrade or suppress a finding. | [Policy application](https://github.com/HardMax71/amiss/blob/main/crates/amiss-scan/src/policy.rs) |
| Reports | Machine output uses report envelope and payload v3. The v0 compatibility level remains `experimental`. | [v3 schema](https://github.com/HardMax71/amiss/blob/main/spec/scanner-report-v3.schema.json) |
| GitHub Action | The published composite Action derives snapshots from supported GitHub events, verifies the selected engine against the manifest in the same action tree, runs it, and emits annotations. | [Composite Action](https://github.com/HardMax71/amiss/blob/main/action/action.yml) |

Repository form is deliberately closed too. The reader accepts a primary non-bare checkout
whose `.git` entry is a real directory. Bare repositories and linked worktrees represented
by a `.git` file are unavailable, and alternate object stores are not consulted. The
[repository boundary](https://github.com/HardMax71/amiss/blob/main/crates/amiss-git/src/repo.rs)
and its [boundary tests](https://github.com/HardMax71/amiss/blob/main/crates/amiss-git/tests/boundary.rs)
pin that behavior.

The supported reference surface is intentionally smaller than “every path-like phrase in
prose.” Bare filenames in ordinary text are not inferred; raw HTML and MDX code regions are
opaque; leading-slash site routes, heading fragments, code symbols, live URLs, and references
to other repositories are not validated under those systems' semantics. Their visible
boundary behavior is described in [Discovery](discovery.md) and [Resolution](resolution.md).

## Built, but not a supported delivery lane

The repository contains strict parsers for evaluation, snapshot, and external-control
requests, plus evaluation logic for organization floors, adoption debt, waivers, trusted
time, and execution constraints. The library entry point accepts those values; the public
CLI supplies all five as absent. Compare the
[request contracts](https://github.com/HardMax71/amiss/blob/main/crates/amiss-wire/src/requests.rs),
[pipeline shell](https://github.com/HardMax71/amiss/blob/main/crates/amiss-scan/src/pipeline.rs),
and [CLI wiring](https://github.com/HardMax71/amiss/blob/main/crates/amiss/src/main.rs).

`amiss-bootstrap` can validate an action tree and execution constraint, launch a verified
engine with a cleared environment, supervise it, and validate its report. It is built by the
release workflow, but the published composite Action currently launches `amiss` directly;
provider-authenticated request acquisition and bootstrap integration are therefore not a
supported required-check lane. The distinction is visible in the
[bootstrap entry point](https://github.com/HardMax71/amiss/blob/main/crates/amiss-bootstrap/src/main.rs),
[release assembly](https://github.com/HardMax71/amiss/blob/main/.github/workflows/release.yml),
and [Action execution](https://github.com/HardMax71/amiss/blob/main/action/action.yml).

Local and convenience-Action reports consequently describe repository policy with no
external authority consulted. Each external control has status `none`, and the sandbox
assurance is `self-asserted`; there is no aggregate `provider_verified` report field. See
[Controls and policy](controls.md) for the exact interpretation.

## Keeping this page honest

Links from factual prose to the implementation are deliberate. The repository's own Amiss
scan makes a changed dependency under unchanged prose visible for review. Exact default
dispositions in [Profiles and findings](profiles.md) and exact resource ceilings in
[Limits and refusals](limits.md) are generated from the Rust contract by a test, so changing
the corresponding constants without updating the book fails CI. The same test validates the
v3 example against its schema and canonical bytes. Neither mechanism pretends to prove the
meaning of free prose; together they make the most mechanical drift visible.
