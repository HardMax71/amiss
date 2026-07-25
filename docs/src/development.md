# Development

The toolchain version is pinned in `rust-toolchain.toml`, `unsafe` is forbidden in every
crate, and the lint table denies panics, lossy casts, wildcard matches, and undocumented
errors. Hooks run through [prek](https://github.com/j178/prek): formatting and the cheap checks on commit, then [Clippy](https://github.com/rust-lang/rust-clippy) with
warnings denied, the full test suite, `cargo deny`, `cargo shear`, and two exact-count
[similarity-rs](https://github.com/mizchi/similarity) twin-function ratchets on push. The tool
compares functions within one file, so the first ratchet counts twins inside every file of both
workspaces, and the second concatenates the deliberately parallel provider files, the three
transports, the three lane-test harnesses, and the three service runtimes, so their cross-file
twins stay counted as well. Each baseline is exact rather than a ceiling: a new twin fails as a
regression, and a cleanup lowers the pinned number in the same change. A last push-stage hook
runs [cargo-sweep](https://github.com/holmgr/cargo-sweep) over `target/`, dropping artifacts and
incremental sessions older than five days; cargo never collects superseded builds, and this
repository mints a fresh copy of every test binary on each lockfile or version change. The hook
is a no-op where cargo-sweep is not installed. CI runs the
same two hook stages, so passing locally and passing remotely are the same thing unless the
hook table itself has a bug.

Two similarly named files point in opposite directions. `.pre-commit-config.yaml` is the hook
table this repository runs on itself through prek. `.pre-commit-hooks.yaml` is the hook this
repository publishes: a consumer's own pre-commit configuration names this repository and reads
that manifest to discover the `amiss` staged-index check shown in
[Running it in CI](ci.md).

```sh
cargo nextest run --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings

cargo test --manifest-path fuzz/Cargo.toml --locked --release
cargo clippy --manifest-path fuzz/Cargo.toml --all-targets --locked -- -D warnings
```

The first pair checks every crate, engine and provider alike, from one lockfile. The second
checks the scanner's standalone fuzz package, which keeps its own lockfile because coverage-guided
runs need nightly. The trust boundary is a dependency boundary rather than a workspace boundary:
HTTP, provider API, Git acquisition, credential, storage, and service-runtime dependencies belong
to the unpublished crates under `controller/`, and `deny-engine.toml` drops those crates from the
graph and then bans the network and async stack, so what an `amiss` user downloads cannot acquire
it. The prek hooks and Linux CI run both sets. The macOS and Windows jobs also run the controller
tests, including the cross-process file stores, provider authentication, worker, and
supervised-process cases. The supported service deployments are documented in
[Provider-verified controls](provider-controls.md).

Tests answer to a house rule called the teeth check: important tests are exercised against
deliberately broken behavior before they are trusted. The
[weekly mutation workflow](https://github.com/HardMax71/amiss/blob/main/.github/workflows/mutants.yml)
publishes a non-gating measurement of that property for the root scanner workspace. Two bounded
controller runs cover authentication and the ownership-to-publication path rather than attempting
every mutant in the unpublished workspace. These measurements do not certify a global mutation
threshold.
The parsers sit under a vendored test corpus, pinned by digest, whose manifest records node
counts, extraction results, and byte positions for every case from the upstream [CommonMark](https://commonmark.org),
[GFM](https://github.github.com/gfm/), and [MDX](https://mdxjs.com) suites; the
[corpus notes](https://github.com/HardMax71/amiss/blob/main/corpus/README.md) document every
known difference. Scanner parsers that take untrusted bytes have targets under `fuzz/`.
The [controller fuzz package](https://github.com/HardMax71/amiss/tree/main/controller/fuzz)
signs generated provider requests before varying their facts, so its account-free targets reach
the provider identity and binding checks. Both suites carry committed seeds and a
[nightly coverage-guided run](https://github.com/HardMax71/amiss/blob/main/.github/workflows/fuzz-long.yml).

The scanner runs on its own repository under `--profile enforce` in CI. This documentation
passes through that same gate: every relative link in this book resolves in the tree, or
the pull request that broke it fails.

Releases are automated. A bot keeps a release pull request current with the version bump,
changelog, and exact Action-dispatch ref. Merging it publishes the crates and source tag while
the GitHub release remains a draft. The release workflow then assembles the immutable
`action/vX.Y.Z` tree and exercises both that exact tree and the source-tag dispatcher on Linux,
both macOS architectures, and Windows. Only a green smoke matrix advances the stable major ref
without rewriting history and makes the release public; prereleases never advance the major ref.
If a forge outage leaves that pull request stale, manually dispatching the
[release automation](https://github.com/HardMax71/amiss/blob/main/.github/workflows/release-plz.yml) on `main` refreshes its metadata
without running the publishing job; crate publication remains restricted to pushes on `main`.
Security checks layer in CI as well. Dependency update PRs arrive with a cooldown, a
weekly advisory re-check runs against a fresh database, and
[CodeQL](https://codeql.github.com) covers both the Rust and the workflows.
[Scorecard](https://scorecard.dev), secret scanning with push protection, and build
provenance attestations on release binaries round it out.
