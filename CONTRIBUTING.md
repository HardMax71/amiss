# Contributing

Amiss takes contributions through pull requests on GitHub. Bug reports and
questions go through
[issues](https://github.com/HardMax71/amiss/issues); anything exploitable goes
through [SECURITY.md](SECURITY.md) instead, never a public issue.

Before writing code, read
[Development](https://hardmax71.github.io/amiss/development.html) for the
toolchain, the gates, and the release flow. The short version: the toolchain is
pinned, hooks run through prek, and a change passes locally exactly when it
passes in CI:

```sh
cargo nextest run --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings

cargo nextest run --manifest-path controller/Cargo.toml --workspace --locked
cargo clippy --manifest-path controller/Cargo.toml --workspace --all-targets --locked -- -D warnings
```

The second pair covers the separate provider-controller workspace; the root workspace remains
the offline scanner and does not take provider transport, storage, or runtime dependencies.

A change is acceptable when it keeps every gate green, adds tests for the
behavior it adds or changes, and stays inside the boundaries described in
[What Amiss is not](https://hardmax71.github.io/amiss/non-goals.html).
Important tests are exercised against deliberately broken behavior before they
are trusted. Documentation passes through the same gate as everything else:
the scanner runs on its own repository, so a link that stops resolving fails
the pull request that broke it.

By contributing you agree that your contribution is licensed under the
repository's [license](LICENSE.md).
