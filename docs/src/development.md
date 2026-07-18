# Development

The toolchain version is pinned in `rust-toolchain.toml`, `unsafe` is forbidden in every
crate, and the lint table denies panics, lossy casts, wildcard matches, and undocumented
errors. Hooks run through [prek](https://github.com/j178/prek): formatting and the cheap checks on commit, then [Clippy](https://github.com/rust-lang/rust-clippy) with
warnings denied, the full test suite, `cargo deny`, `cargo shear`, and an exact-count
[similarity-rs](https://github.com/mizchi/similarity) twin-function ratchet on push. CI runs the
same two hook stages, so passing locally and passing remotely are the same thing unless the
hook table itself has a bug.

```sh
cargo nextest run --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Tests answer to a house rule called the teeth check: important tests are exercised against
deliberately broken behavior before they are trusted. The
[weekly mutation workflow](https://github.com/HardMax71/amiss/blob/main/.github/workflows/mutants.yml)
publishes a non-gating measurement of that property; it does not currently certify a global
mutation threshold.
The parsers sit under a vendored test corpus, pinned by digest, whose manifest records node
counts, extraction results, and byte positions for every case from the upstream [CommonMark](https://commonmark.org),
[GFM](https://github.github.com/gfm/), and [MDX](https://mdxjs.com) suites; the
[corpus notes](https://github.com/HardMax71/amiss/blob/main/corpus/README.md) document every
known difference. Each parser that takes untrusted bytes also has a fuzz target under
`fuzz/`, with committed regression inputs and a
[nightly coverage-guided run](https://github.com/HardMax71/amiss/blob/main/.github/workflows/fuzz-long.yml).

The scanner runs on its own repository under `--profile enforce` in CI. This documentation
passes through that same gate: every relative link in this book resolves in the tree, or
the pull request that broke it fails.

Releases are automated. A bot keeps a release pull request current with the version bump
and changelog; merging it publishes the crates, the version tag, and the GitHub release.
If a forge outage leaves that pull request stale, manually dispatching the
[release automation](https://github.com/HardMax71/amiss/blob/main/.github/workflows/release-plz.yml) on `main` refreshes its metadata
without running the publishing job; crate publication remains restricted to pushes on `main`.
Security checks are layered in CI as well: dependency update PRs with a cooldown, a weekly
advisory re-check against a fresh database, [CodeQL](https://codeql.github.com) over both the Rust and the workflows,
[Scorecard](https://scorecard.dev), secret scanning with push protection, and build provenance attestations on
release binaries.
