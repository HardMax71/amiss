# Amiss

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
annotates findings on the pull request; the moving major ref follows the engine's
semver major, so it is `v0` for the 0.x series:

```yaml
- uses: actions/checkout@v7
  with:
    fetch-depth: 2
- uses: HardMax71/amiss@v0
```

Exit 0 means a complete run with nothing blocking, 1 a complete run with a blocking finding,
and 2 anything that prevented a result worth trusting.

The [documentation](https://hardmax71.github.io/amiss/) has the rest: the full command
grammar, the evaluation semantics, the report contract, resource ceilings, the security
model, and how this repository scans itself with its own scanner in CI.

Licensed [FSL-1.1-ALv2](LICENSE.md): free to use, including commercially, not to turn into a
competing product, and each release becomes Apache-2.0 two years on.

Development runs through pinned hooks: `cargo nextest run --workspace` and
`cargo clippy --workspace --all-targets -- -D warnings` are the local gate.
