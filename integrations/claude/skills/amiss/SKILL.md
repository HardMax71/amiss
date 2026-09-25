---
name: amiss
description: Check documentation against the repository tree with Amiss. Use before committing changes that touch documentation or files documentation references, when an amiss CI check fails and needs fixing, or when asked whether docs drifted.
---

# Checking documentation with Amiss

Amiss compares two exact snapshots of a Git repository and reports every documentation
reference that stopped resolving, every referenced file that changed under unchanged
prose, and every loosened control. It is deterministic, reads only the repository, and
never guesses. `amiss --help` prints the closed grammar on stdout; an invalid human
invocation prints the same grammar on stderr, and both outputs are trustworthy.

## Running it

The binary comes from `cargo install --locked amiss`, or, with no Rust toolchain at hand,
from the release asset for the platform, such as `amiss-linux-x86_64` from
`gh release download v<version> --repo HardMax71/amiss`; prefer the exact version the
repository's CI pins. Both snapshot arguments are full commit IDs, never refs.

The staged check reads the index, not the working tree, so stage your edits first. It
uses the profile the published pre-commit hook uses, which blocks what the change
introduces and only warns about older findings:

```sh
git add -A
amiss check --repo . --object-format sha1 \
  --base "$(git rev-parse HEAD)" --index --profile enforce-introduced --format json > report.json
```

`--profile enforce` also blocks the backlog that was already there before your change;
use it only when the task is to clear that backlog. A pushed range, the what-CI-saw check:

```sh
amiss check --repo . --object-format sha1 \
  --base "$(git rev-parse <base-oid>)" --candidate "$(git rev-parse <head-oid>)" \
  --profile enforce-introduced --format json > report.json
```

If the repository's CI passes `--repository`, `--ref` and `--default-branch-ref`, pass the
same values: without them a link to this repository written as a full forge URL is treated
as external and never checked.

Exit 0: complete pass. Exit 1: complete run, at least one finding blocks. Exit 2:
nothing trustworthy was produced; read `errors[]` for why, and expect causes like a
shallow checkout missing a commit or a crossed resource ceiling.

## Reading the report

The JSON payload is the source of truth. Blocking rows are `errors[]` and the findings
whose `effective_disposition` is `fail`. Every row carries `description`, a fixed
sentence saying what the row means and what to do, and `location.path` plus
`location.span` name the exact source position. One line to list the actionable rows:

```sh
jq -r '.payload.findings[]
  | select(.effective_disposition != "record")
  | [.effective_disposition, .kind, .attribution,
     "\((.location.path | strings) // "-"):\(.location.span.start_line // "-")",
     (if .fix then "has-fix" else "-" end)]
  | @tsv' report.json
```

For a missing target, `candidate_fact.evidence.resolution` holds two hints: `near` is an
existing path or anchor spelled almost the same, and `same_object_at` is where the same
file bytes live now, which is what a rename leaves behind. `feedback` is the grouped PR
presentation: Fixes, summary-only Checks, and an Existing count. Use it to orient, but use
the raw finding rows above for exact repair evidence.

## Fixing findings

A finding with a `fix` carries an edit the engine proved: the document, the byte span and
the replacement. With the same arguments as the staged check, `amiss fix` applies every
one of them in place; stage the result and check again:

```sh
amiss fix --repo . --object-format sha1 \
  --base "$(git rev-parse HEAD)" --index --profile enforce-introduced
```

Fix the rest by hand from what the row points at: relink a renamed target to its
`same_object_at` path, restore a missing target or correct the link, make a trailing
slash agree with what the path is, reread prose whose referenced code changed. To see
every reference to one path before moving or deleting it, run
`amiss refs --report report.json --target <repo-path>`.

Never weaken `.amiss/scanner-policy.json` to silence a finding; policy can only raise
severity, and loosening it is itself a blocking finding. Leave `.amiss/router.yml` to a
maintainer, since it states which router publishes the tree. Never delete a document just
to clear a finding. If a finding names drift you cannot verify, say so instead of
guessing.

The full reference is the book at https://hardmax71.github.io/amiss/, with every
finding kind's meaning at https://hardmax71.github.io/amiss/profiles.html and every
error code's at https://hardmax71.github.io/amiss/errors.html.
