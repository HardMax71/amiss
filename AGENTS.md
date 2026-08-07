# AGENTS.md

Amiss is one Rust workspace. The engine crates under `crates/` check documentation against
the repository tree it describes and take no network or async dependency; the unpublished
provider crates under `controller/` carry that stack and nothing in the engine depends on
them. The book under `docs/` is the reference;
`CONTRIBUTING.md` states the acceptance bar.

## Build and test

```sh
cargo nextest run --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings

cargo test --manifest-path fuzz/Cargo.toml --locked --release
cargo clippy --manifest-path fuzz/Cargo.toml --all-targets --locked -- -D warnings
```

The toolchain is pinned by `rust-toolchain.toml`. Hooks run through prek: formatting and
cheap checks on commit; clippy, the full suite, `cargo deny`, `cargo shear`, a
pinned similarity-rs twin-function count, and a cargo-sweep pass over `target/` on push. CI runs the same stages, so local
green and remote green are the same thing.

## Laws the linters cannot see

- `unsafe` is forbidden everywhere; the lint table denies panics, lossy casts, and
  wildcard matches.
- Comments are rare: one short line for a constraint the code cannot show, never a
  restatement of the code.
- The wire is one rolling contract. A report change moves the schema in `spec/`, both
  examples (with a recomputed payload digest), the writer, and the docs together.
- Blocks between `amiss-doc-contract` markers in `docs/` copy a value a Rust source owns,
  and a test asserts the two are equal. Nothing writes the block for you: move the Rust
  source and the block together, because editing either one alone fails CI.
- The fixed description sentences live in `FindingKind::meaning`,
  `AnalysisErrorCode::meaning`, and `FixKind::meaning` and nowhere else; every other
  appearance is a checked projection.
- New function twins move the similarity baseline in `.pre-commit-config.yaml`; bump it
  in the same change, or better, deduplicate.
- The scanner's repository I/O stays inside the repository, and it spawns nothing. Its
  private sealed entry additionally reads only the closed request frame from stdin. Shared
  test scaffolding goes in `amiss-fixtures`.
- `controller/` is a separate, unpublished Rust workspace. Provider transport, storage,
  credential, and runtime dependencies stay there; never add them to the offline root
  workspace or its lockfile.

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
