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
cargo nextest run --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

A change is acceptable when it keeps every gate green, adds tests for the
behavior it adds or changes, and stays inside the boundaries described in
[What Amiss is not](https://hardmax71.github.io/amiss/non-goals.html).
Important tests are exercised against deliberately broken behavior before they
are trusted. Documentation passes through the same gate as everything else:
the scanner runs on its own repository, so a link that stops resolving fails
the pull request that broke it.

By contributing you agree that your contribution is licensed under the
repository's [license](LICENSE.md).
