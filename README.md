<h1 align="center">Amiss</h1>

<p align="center">
  <a href="https://crates.io/crates/amiss"><img alt="version" src="https://img.shields.io/crates/v/amiss?style=flat-square&label=version&labelColor=1e293b&color=475569"></a>
  <a href="LICENSE.md"><img alt="license" src="https://img.shields.io/badge/license-FSL--1.1--ALv2-475569?style=flat-square&labelColor=1e293b"></a>
</p>

Amiss checks documentation against the tree it describes. It reads the documents in a
repository, follows the references they make into that same repository, and reports when a
reference stops resolving, or when the file behind it changed while the prose around it did
not. It reads structure, not meaning: it will not tell you whether a sentence is true, and it
does not guess.

It keeps no state, executes nothing, never touches the network, and never writes; identical
inputs through the same engine binary produce byte-identical reports. A reference that does not
resolve fails the run under `enforce`; a file changing under an unchanged paragraph is always
a signal for a human, never a machine verdict.

```sh
cargo install amiss

amiss check --repo . --object-format sha1 \
    --base "$(git rev-parse HEAD~1)" --candidate "$(git rev-parse HEAD)" \
    --profile observe
```

In CI the same engine ships as an action that derives both commits from the event and
annotates findings on the pull request:

```yaml
- uses: actions/checkout@v7
  with:
    fetch-depth: 2
- uses: HardMax71/amiss@v0
```

Everything else is in the [documentation](https://hardmax71.github.io/amiss/).
