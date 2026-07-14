# Amiss

Amiss checks documentation against the tree it describes. It reads the documents in a
repository, follows the references they make into that same repository, and reports when a
reference stops resolving, or when the file behind it changed while the prose around it did
not. It reads structure, not meaning: it will not tell you whether a sentence is true, and it
does not guess.

A reference that does not resolve is a structural failure, and under `enforce` it fails the
run. A file changing under a document never rises above a warning, whatever the profile: it is
a signal, not a verdict. Amiss keeps no state between runs, so there is nothing to migrate and
nothing to drift, and it accepts no claims or waivers from the repository it scans, because a
check whose subject can switch it off is not a check.

## Build and run

Nothing is published yet, so build it:

```
cargo build --release -p amiss
```

The command line is closed. Every option is listed here, each may appear at most once, order
does not matter, and there is no `--help`.

```
amiss check --repo <path> --object-format <sha1|sha256>
            --base <full-oid> (--candidate <full-oid> | --index)
            [--repository github.com/<owner>/<name>
             --ref refs/heads/<name>
             --default-branch-ref refs/heads/<name>]
            --profile <observe|enforce>
            [--explain-scope] [--format <human|json>]
```

`--base` and `--candidate` take full object IDs, never refs or abbreviations. Use `--index` in
place of `--candidate` to evaluate the staged index against a base commit. The optional
`--repository` triple turns a GitHub blob URL in your prose into a path Amiss will check;
without it those links are foreign URLs and go unchecked, which the report says out loud.

`observe` warns on everything and is where a rollout starts. `enforce` turns an unresolved
reference into a failure. Exit 0 is a complete run with nothing blocking, exit 1 is a complete
run with a blocking finding, and exit 2 is anything that stopped Amiss from producing a result
it trusts.

## Status

Experimental, and the reports say so in a `compatibility` field. The license is FSL-1.1-ALv2:
free to use, including commercially, but not to turn into a competing product, and each release
becomes Apache-2.0 two years on. Releases are cut by a bot: merging its release PR publishes
the crates, the tag, and the GitHub release. The local command is the whole product today; the
research and the normative specifications behind it are not in this tree yet.

## Development

The toolchain is pinned in `rust-toolchain.toml`, and `unsafe` is forbidden in every crate.
Hooks run through prek: formatting and the cheap checks on commit, then Clippy with warnings
denied, the test suite, `cargo deny`, and `cargo shear` on push.

```
cargo nextest run --workspace
cargo clippy --workspace --all-targets -- -D warnings
```
