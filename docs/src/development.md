# Development

The toolchain version is pinned in `rust-toolchain.toml`, today at 1.97.0, `unsafe` is
forbidden in every crate, and the lint table denies panics, lossy casts, wildcard
matches, and undocumented errors. The version in that sentence is this book's first live
[value claim](claims.md): the definition below pins the pinning line itself, so a
toolchain bump that forgets this page fails the repository's own gate with the corrected
expectation in the finding.

[amiss:toolchain-channel]: <amiss:value?path=rust-toolchain.toml&line=L2> 'channel = "1.97.0"'

Hooks run through [prek](https://github.com/j178/prek): formatting and the cheap checks
on commit, then [Clippy](https://github.com/rust-lang/rust-clippy) with
warnings denied, the full test suite, `cargo deny`, `cargo shear`, and two exact-count
[similarity-rs](https://github.com/mizchi/similarity) twin-function ratchets on push. The tool
compares functions within one file, so the first ratchet counts twins inside every file of both
workspaces, and the second concatenates the deliberately parallel provider files, the three
transports, the three lane-test harnesses, and the three service runtimes, so their cross-file
twins stay counted as well. Each baseline is exact rather than a ceiling: a new twin fails as a
regression, and a cleanup lowers the pinned number in the same change. A last push-stage hook
runs [cargo-sweep](https://github.com/holmgr/cargo-sweep) over `target/`, dropping artifacts and
incremental sessions older than two days; cargo never collects superseded builds, and this
repository mints a fresh copy of every test binary on each lockfile or version change. Five days
held 86 GB and the sweep reclaimed nothing from it, because every generation was inside the
window. The hook
is a no-op where cargo-sweep is not installed. CI runs the same two hook stages, so a hook
that passes locally passes remotely unless the hook table itself has a bug. What CI adds on
top is the work that does not belong on a developer's machine: the fuzz packages, whose
release builds and separate lockfiles cost minutes, and mutation, which costs ten of them for
a change of any size. A push should not buy what a pull request already measures.

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
it. The prek hooks run the first pair and Linux CI runs both. The macOS and Windows jobs also run the controller
tests, including the cross-process file stores, provider authentication, worker, and
supervised-process cases. The supported service deployments are documented in
[Provider-verified controls](provider-controls.md).

Tests answer to a house rule called the teeth check: important tests are exercised against
deliberately broken behavior before they are trusted. The
[mutation workflow](https://github.com/HardMax71/amiss/blob/main/.github/workflows/mutants.yml)
publishes a non-gating measurement of that property, in three sizes.

Every pull request measures only the mutants the change itself reaches, over shards counted
from the mutants the diff actually reaches rather than from a guess, because listing them needs
no build. That lane asks the whole workspace whether each mutant lives, at about twenty seconds
per mutant. The shards used to pay a worse floor, a cold workspace build plus a baseline test
pass repeated in every shard; now each shard restores the build cache that the baseline job
saves on every push to main, and skips its own baseline because ci proves the same commit in
the same run, so what remains is the build delta against the last merge and the mutants
themselves. A release pull
request measures what the release ships, every change since the last tag, rather than the version
bump standing in front of it. Both ask the whole workspace whether a mutant lives.

The sweep over every mutant in both workspaces runs only when someone asks for it, through
`workflow_dispatch`. It is split across shards sized from the mutant count rather than a fixed
number, it takes tens of minutes, and it exists to find gaps in code that no longer changes,
which is not something a release should pay for. Fixture crates are excluded, because code that
exists to be exercised by its callers says nothing about the tests. Unlike the smaller lanes it
runs each mutant against its own package's tests, which is what makes it affordable and also
means a mutant that a sibling package covers is reported as surviving. Its output is a list to
verify, not a verdict.

The sweep of 2026-07-28 is the reading to compare against: 6,523 mutants, 4,103 caught, 1,335
missed, 1,085 unviable, no timeouts. Roughly half of those missed are the scoping artifact
above, measured at 43% on one file and 62% on another, so the number of real gaps is nearer
seven hundred. A later sweep that misses far more has either lost tests or gained untested
code, and the point of writing the figure down is to be able to tell which.

None of the three gates a merge and none certifies a global mutation threshold: a surviving
mutant is a place where a lie would go unnoticed, to be judged against whether the perturbed
value is observable through real behavior, not a score to raise.

Three agent lanes sit beside the gates, none of them gating. A new issue gets its premise
checked against the tree before a maintainer reads it, behind an account-age gate so drive-by
accounts spend no tokens. A new pull request gets one evidence-based review comment. And `/oc`
in a comment summons the agent for collaborators, who can ask it to fix an issue and open a
pull request; that lane borrows the release token, since a pull request opened by the runner's
own token would never trigger ci. All three run opencode on DeepSeek inside the repository's
runners, read AGENTS.md like any contributor, and treat issue text as a claim under test
rather than as instructions.
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

Every pull request also packages what a release would upload. `cargo package` over the
publishable members resolves their siblings through a temporary local registry and builds every
tarball, so a file dropped from a package, a path dependency missing a version, or a new crate
its dependants cannot see fails on the pull request instead of halfway through an upload that
cannot be taken back. It is `cargo package` rather than `cargo publish --dry-run` because the
dry run prefers a version already on crates.io over the tree, which makes it blind to exactly
the crate a change is adding.

Releases are automated. A bot keeps a release pull request current with the version bump,
changelog, and exact Action-dispatch ref. Merging it publishes the crates and source tag while
the GitHub release remains a draft. The release workflow then assembles the immutable
`action/vX.Y.Z` tree and exercises both that exact tree and the source-tag dispatcher on Linux,
both macOS architectures, and Windows. Only a green smoke matrix advances the stable major ref
without rewriting history and makes the release public; prereleases never advance the major ref.
The same gate governs the release assets: the four engine binaries, their `SHA256SUMS`, and the
sigstore bundle attesting that file attach to the draft, so a release that fails the matrix
never publishes a binary.
If a forge outage leaves that pull request stale, manually dispatching the
[release automation](https://github.com/HardMax71/amiss/blob/main/.github/workflows/release-plz.yml) on `main` refreshes its metadata
without running the publishing job; crate publication remains restricted to pushes on `main`.
Security checks layer in CI as well. Dependency update PRs arrive with a cooldown, a
weekly advisory re-check runs against a fresh database, and
[CodeQL](https://codeql.github.com) covers both the Rust and the workflows.
[Scorecard](https://scorecard.dev), secret scanning with push protection, and build
provenance attestations on release binaries round it out.
