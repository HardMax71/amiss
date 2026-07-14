# Development

The toolchain is pinned in `rust-toolchain.toml`, `unsafe` is forbidden in every crate, and
the lint table denies the panic family, lossy casts, wildcard matches, and undocumented
errors. Hooks run through prek: formatting and the cheap checks on commit, then Clippy with
warnings denied, the full test suite, `cargo deny`, and `cargo shear` on push. CI runs the
same two hook stages, so green locally means green remotely or the hook table has a bug worth
fixing.

```text
cargo nextest run --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Tests answer to a discipline the project calls teeth checks: a test earns its place by
failing when the code it guards is deliberately broken, and the mutation is run before the
test is trusted. A weekly mutation-testing lane keeps the honest version of that claim
measured. The parser sits under a vendored conformance corpus, pinned by digest, whose
manifest records node counts, extraction goldens, and byte spans for every case from the
upstream CommonMark, GFM, and MDX suites; `corpus/README.md` documents the recorded
divergences. Every parser that consumes untrusted bytes also has a fuzz target under
`fuzz/`, with committed regression seeds and a nightly coverage-guided lane.

The scanner runs on its own repository under `--profile enforce` in CI, which is the
dogfooding gate this documentation itself passes through: every relative link in this book
resolves in the tree, or the pull request that broke it fails.

Releases are automated: a bot maintains a release pull request with the version bump and
changelog, and merging it publishes the crates, the `v` tag, and the GitHub release. Security
posture is layered in CI as well: dependency update PRs with a cooldown, a weekly advisory
re-check against a fresh database, CodeQL over both the Rust and the workflows, Scorecard,
secret scanning with push protection, and SLSA provenance attestations on release binaries.
