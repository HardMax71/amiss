# Pre-implementation experiments

These are disposable, read-only calibration experiments for
[preimpl-experiments.md](../preimpl-experiments.md). They do not define a public scanner,
directive, JSON, ledger, or policy contract. Generated outputs are intentionally verbose so the
reported counts can be audited.

Run from the repository root, in this order:

```bash
node ci-idea/experiments/scan-current.mjs \
  --out ci-idea/experiments/current-scan.json
node ci-idea/experiments/directive-matrix.mjs \
  --out ci-idea/experiments/directive-matrix.json
node ci-idea/experiments/label-inline-sample.mjs \
  --out ci-idea/experiments/inline-path-sample.json
node ci-idea/experiments/workflow-audit.mjs \
  --out ci-idea/experiments/workflow-audit.json
node ci-idea/experiments/history-replay.mjs \
  --out ci-idea/experiments/history-replay.json
node ci-idea/experiments/ledger-simulation.mjs \
  --out ci-idea/experiments/ledger-simulation.json
bash ci-idea/experiments/run-merge-conflict-simulation.sh 100
bash ci-idea/experiments/run-runtime-benchmark.sh 10
node ci-idea/experiments/validate-machine-contracts.mjs
node ci-idea/experiments/validate-gitignore-vectors.mjs
node ci-idea/experiments/validate-dossier.mjs
```

The scanner and parser matrix use the already-installed packages under `docs/node_modules`; they
make no network requests. The history replay requires the repository's full first-parent history.
The merge experiment creates and removes repositories only beneath `/tmp`. Every durable result is
under this directory.

## Artifact manifest

| Artifact | Contents |
| --- | --- |
| `current-scan.json` | Git-backed discovery inventory, parsed explicit references, inline candidates, resolution attempts, source locations, and one resource observation |
| `directive-matrix.json` | CommonMark, GFM, MDX, math, frontmatter, duplicate-definition, unique-definition, JSX, ESM, Unicode, and CRLF behavior |
| `inline-path-sample.json` | Manual labels for all 16 missing repository-rooted candidates and a preselected 20-record ambiguous sample |
| `workflow-audit.json` | Trigger, path-filter, permission, checkout-depth, credential, and immutable-pin inventory |
| `history-replay.json` | First-parent churn, surviving-current-graph replay, and transition history for five known broken cases |
| `ledger-simulation.json` | Cardinality, serialized-size, body-store, and writer-churn scenarios |
| `simulated-minimal.jsonl` | Compact synthetic observation specimen |
| `simulated-detailed.jsonl` | Deliberately verbose synthetic endpoint-snapshot specimen |
| `merge-conflict-workload.json` | Seed and generated workload for the merge experiment |
| `merge-conflict-results.tsv` | Raw `git merge-file` outcomes for the single sorted JSONL layout |
| `merge-conflict-sharded-results.tsv` | Raw representative repository-merge outcomes for one file per ClaimId |
| `merge-conflict-simulation.json` | Conflict rates, intervals, assumptions, and non-claims |
| `runtime-benchmark.tsv` | Raw per-process wall time, scanner self-time, memory, and output bytes |
| `runtime-benchmark.json` | Runtime summary for the experiment scanner and existing link checker |
| `validate-machine-contracts.mjs` | Read-only smoke checker for five root schema/examples, four advertised fragment examples, canonical report bytes, selected semantic digests, 26 ignore cases, 15 LFS-pointer cases, 19 ref-format cases, 38 reference-constructor cases, eight correlation-intent cases, nine frontmatter boundaries, six governed-definition cases, and five core seed vectors; not a strict-JSON/product semantic conformance validator |
| `validate-gitignore-vectors.mjs` | Disposable isolated-repository differential runner for the 26 `gitignore-v1` cases/60 entry outcomes; transient repositories live beneath the OS temporary directory so concurrent dossier validation cannot traverse them; the local Git run is supporting evidence, while the pinned Git 2.42.0 oracle remains normative |
| `validate-dossier.mjs` | Read-only JSON/JSONL, relative-link/anchor, fence, and Markdown-whitespace validator for this dossier |
| `../spec/examples/index-projection-v1.json` | Complete two-entry logical staged-index digest preimage, including one skip-worktree row |
| `../spec/examples/synthetic-snapshot-v1.json` | Small synthetic snapshot preimage bound to the index-projection golden |
| `../spec/examples/candidate-identity-v1.json` | Commit-pair candidate-identity preimage reproduced from the report example |
| `../spec/examples/candidate-identity-index-v1.json` | Index-mode candidate identity binding entry/skip counts and synthetic snapshot digest |
| `../spec/examples/lfs-pointer-v1-vectors.json` | Exact ordered current, legacy, extension, encoding, line-ending, and byte/integer-boundary LFS recognizer cases |
| `../spec/examples/reference-constructor-v1-vectors.json` | Native link/image/directory target kinds, all autolink constructors, GitHub line and tree-terminal-slash boundaries, exact query/fragment splitting, case-sensitive host and folded literal identity, Unicode/percent/ref splitting, empty destinations, missing/invalid remaining paths, and network paths |
| `../spec/examples/correlation-intent-v1-vectors.json` | Reachable native/GitHub equivalence plus path/kind/query/external/site/unsupported projection differences |
| `../spec/examples/frontmatter-v1-vectors.json` | BOM-relative exclusive-end, LF/CRLF/bare-CR, and EOF frontmatter recognition boundaries; the still-missing parser corpus must separately prove hostile-body opacity |
| `../spec/examples/governed-definition-v1-vectors.json` | Reserved-label escape/entity decoding, source-byte digest, duplicate multiplicity, nonreserved definitions, and first-winner consumer precedence |

## Interpretation limits

- This is one repository, selected because it already has known drift. It is not external product
  validation.
- The current-graph replay projects relationships surviving at HEAD backward. It misses deleted
  relationships and uses current selectors, so it is a workload estimate with survivorship bias,
  not historical precision or recall.
- The inline labels are manual. The repository-rooted stratum is a census of current missing
  occurrences; the ambiguous stratum is a deterministic sample, not a random external sample.
- The synthetic ledger formats deliberately have no compatibility promise.
- The sharded merge result is conditional on disjoint ClaimIds and no rename, case-folding, or
  directory/file collision. It does not measure filesystem or review overhead.
- Runtime was measured on a warm local filesystem and this machine, not a hosted CI runner.
