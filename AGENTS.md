# AGENTS.md

Amiss is a Rust workspace: an engine that checks documentation against the repository
tree it describes. The book under `docs/` is the reference; `CONTRIBUTING.md` states the
acceptance bar.

## Build and test

```sh
cargo nextest run --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

The toolchain is pinned by `rust-toolchain.toml`. Hooks run through prek: formatting and
cheap checks on commit; clippy, the full suite, `cargo deny`, `cargo shear`, and a
pinned similarity-rs twin-function count on push. CI runs the same stages, so local
green and remote green are the same thing.

## Laws the linters cannot see

- `unsafe` is forbidden everywhere; the lint table denies panics, lossy casts, and
  wildcard matches.
- Comments are rare: one short line for a constraint the code cannot show, never a
  restatement of the code.
- The wire is one rolling contract. A report change moves the schema in `spec/`, both
  examples (with a recomputed payload digest), the writer, and the docs together.
- Blocks between `amiss-doc-contract` markers in `docs/` are generated from Rust and
  compared in CI; edit the Rust source, not the block.
- The fixed description sentences live in `FindingKind::meaning` and
  `AnalysisErrorCode::meaning` and nowhere else; every other appearance is a checked
  projection.
- New function twins move the similarity baseline in `.pre-commit-config.yaml`; bump it
  in the same change, or better, deduplicate.
- The engine spawns nothing and reads only the repository. Shared test scaffolding goes
  in `amiss-fixtures`.

## Checking your own change

The scanner runs on this repository in CI under `--profile enforce`. To run what CI
runs, on the staged state:

```sh
cargo run -p amiss -- check --repo . --object-format sha1 \
  --base "$(git rev-parse HEAD)" --index --profile enforce
```

Exit 0 passes, 1 blocks, 2 means the run could not be trusted. Use `--format json` for
detail; every finding and error row carries a `description` saying what it means and
what to do.
