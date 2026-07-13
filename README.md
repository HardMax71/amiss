# Amiss

Amiss checks documentation against the tree it describes. It reads the documents in a repository,
follows the references they make into that same repository, and reports when a reference stops
resolving or when the file behind it changed while the prose around it did not.

It does not read your prose. It cannot tell you whether a sentence is true, current, or well
written, and it does not guess. What it knows is structural, and it is exact: this link points at
`src/parser.rs`, that file's bytes changed between these two commits, and the paragraph holding the
link did not.

## What that looks like

A document describing a parser, and a commit that rewrites the parser:

```
$ amiss check --repo . --object-format sha1 \
      --base "$(git rev-parse HEAD~1)" --candidate "$(git rev-parse HEAD)" \
      --profile observe

amiss: pass (findings 2, errors 0, exit 0)
warn explicit-target-missing pre-existing "docs/parsing.md" x1
warn dependency-changed-subject-unchanged not-applicable "docs/parsing.md" x1
documents: discovered 1 scanned 1 unsupported 0 excluded 0 unlinked 0
references: extracted 2 local 2 github 0 external 0 unsupported 0 missing 1
findings: total 2 fail 0 warn 2 record 0
```

The first finding is a link to a file that is not in the tree. It was already broken before this
change, so it is marked `pre-existing` and you can tell it apart from anything the pull request
introduced. The second is the one worth having: `src/parser.rs` changed, and the block of
`docs/parsing.md` that references it did not.

Amiss makes no claim that the paragraph is now wrong. It says the code moved and the prose did not,
and it leaves the judgment where it belongs. Under `--profile enforce` the same run exits 1, because
a reference that does not resolve is a structural failure. A file changing under a document never
rises above a warning, whatever the profile: it is a signal, not a verdict.

## What it answers

Four questions, and nothing else:

1. Does a same-repository reference in a document resolve in the exact tree being evaluated?
2. Did the bytes or the Git mode of a referenced file change between the base and the candidate?
3. Did the source block holding that reference change with it, stay byte-identical, disappear, or
   become impossible to correlate without guessing?
4. What document and reference surface was discovered, excluded, opaque, unsupported, or unlinked?

The fourth carries as much weight as the first three. A checker that quietly skips what it cannot
parse is worse than no checker at all, because it reports a success it never earned. Every document
Amiss cannot read, every reference it cannot follow, and every region it cannot see into is a row in
the report, and a document it cannot decode fails the run instead of vanishing from it.

There is no baseline, no state directory, no ledger, and no lockfile. Amiss remembers nothing between
runs, so there is nothing to migrate and nothing to drift. It accepts no claims, waivers, or
annotations from the repository it is scanning, because a check whose subject can switch it off is
not a check.

## Guarantees

Each of these is a test, not a promise.

- It never writes to the repository. The suite snapshots the whole tree before and after every
  command and compares, and it runs the scanner against a tree it has no permission to write.
- It never runs anything from the repository, and it never shells out to `git`. It reads objects,
  packs, and the index itself.
- Every read goes through a directory handle that is never followed. A symlink, junction, or reparse
  point at the root, at `.git`, at `objects`, or anywhere in an object's path is refused, not
  followed, and never mistaken for an absent object.
- It never touches the network, and the engine's dependency closure contains no network crate. Check
  it yourself with `cargo tree -p amiss --edges normal`.
- The same repository and the same commits produce the same bytes. `--format json` emits exactly
  `JCS(envelope)` followed by one newline, with a digest over the payload.
- Every limit is a number in the contract. Crossing one is a typed error carrying both the limit and
  what was observed, never a hang, a crash, or a silent truncation.

## Running it

Nothing is published yet, so build it:

```
cargo build --release -p amiss
```

The command line is closed. Every option below is the whole grammar, each may appear at most once,
order does not matter, and anything else is an invalid invocation. There is no `--help`, which is
why the grammar is written out here.

```
amiss check --repo <path> --object-format <sha1|sha256>
            --base <full-oid> (--candidate <full-oid> | --index)
            [--repository github.com/<owner>/<name>
             --ref refs/heads/<name>
             --default-branch-ref refs/heads/<name>]
            --profile <observe|enforce>
            [--explain-scope] [--format <human|json>]
```

`--base` and `--candidate` take full object IDs, never refs or abbreviations: Amiss evaluates the
exact trees you name and resolves nothing on your behalf. Use `--index` in place of `--candidate` to
evaluate the staged index against a base commit.

`observe` warns on everything and is where a rollout starts. `enforce` turns an unresolved reference
into a failure, and is meant for a required check after the existing breakage is cleaned up. Exit 0
is a complete run with nothing blocking, exit 1 is a complete run with at least one blocking finding,
and exit 2 is anything that stopped Amiss from producing a result it trusts.

## Status

Experimental, and the reports say so in a `compatibility` field. Nothing is published to any
registry, no release has shipped, and the license is not decided.

The local command is the whole product today. The GitHub Action tree builds, pins, and validates, and
`amiss-bootstrap` will launch a verified engine under a watchdog with an empty environment, but no
workflow invokes it yet: the required-workflow and provider-verified lanes wait on the request-wire
RFC in [docs/request-wire-rfc.md](docs/request-wire-rfc.md). Until that lands, every external control
(organization floor, adoption debt, waivers, trusted time, execution constraint) reports as `none`,
which is the honest answer rather than a placeholder.

## Layout

- `crates/amiss` is the engine and the command line. It runs the evaluation in-process and emits the
  report.
- `crates/amiss-bootstrap` is the trusted wrapper for CI. It validates the pinned action tree as
  data, launches the verified engine with a cleared environment and fixed arguments, holds it to a
  wall ceiling, and republishes only a report it can accept.
- `crates/amiss-git` is the object store: loose objects, packs, deltas, and the index, all behind the
  no-follow handle boundary.
- `crates/amiss-md` holds the document parsers, pinned against the upstream CommonMark and GFM
  conformance corpora.
- `crates/amiss-scan` is discovery, reference resolution, block correlation, evaluation, and policy.
- `crates/amiss-wire` is JSON canonicalization, the digest domains, the report envelope, and the
  machine contracts.

## Development

The toolchain is pinned in `rust-toolchain.toml`. Hooks run through [prek](https://github.com/j178/prek):
formatting and the cheap checks on commit, then Clippy with warnings denied, the full test suite,
`cargo deny`, and `cargo shear` on push.

```
cargo nextest run --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

`fuzz/` is a separate workspace of cargo-fuzz targets over the parsers, the object store, and the
wire formats. `unsafe` is forbidden in every crate.

## The specification

`docs/` holds the research and the normative specifications this implementation is built against.
[`scanner-v0-spec.md`](docs/scanner-v0-spec.md) is the implementation boundary,
[`ci-security-spec.md`](docs/ci-security-spec.md) the threat model and sandbox rules, and
[`machine-contracts.md`](docs/machine-contracts.md) the wire shapes and digests.

They are the authority. Where the code and a specification disagree, the specification is right and
the code has a bug.
