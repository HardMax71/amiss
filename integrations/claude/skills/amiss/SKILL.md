---
name: amiss
description: Check documentation against the repository tree with Amiss. Use before committing changes that touch documentation or files documentation references, when an amiss CI check fails and needs fixing, or when asked whether docs drifted.
---

# Checking documentation with Amiss

Amiss compares two exact snapshots of a Git repository and reports every documentation
reference that stopped resolving, every referenced file that changed under unchanged
prose, and every loosened control. It is deterministic, reads only the repository, and
never guesses. There is no `--help`; an invalid invocation prints the closed grammar on
stderr, and that output is trustworthy.

## Running it

The binary comes from `cargo install --locked amiss`; prefer the exact version the
repository's CI pins. Both snapshot arguments are full commit IDs, never refs.

Staged state against the last commit, the pre-commit check:

```sh
amiss check --repo . --object-format sha1 \
  --base "$(git rev-parse HEAD)" --index --profile enforce --format json
```

A pushed range, the what-CI-saw check:

```sh
amiss check --repo . --object-format sha1 \
  --base "$(git rev-parse <base-oid>)" --candidate "$(git rev-parse <head-oid>)" \
  --profile enforce --format json
```

Exit 0: complete pass. Exit 1: complete run, at least one finding blocks. Exit 2:
nothing trustworthy was produced; read `errors[]` for why, and expect causes like a
shallow checkout missing a commit, an undecodable document, or a crossed resource
ceiling.

## Reading the report

The JSON payload is the source of truth. Blocking rows are `errors[]` and the findings
whose `effective_disposition` is `fail`. Every row carries `description`, a fixed
sentence saying what the row means and what to do. For reference findings,
`key_input.scope.normalized_target_intent.path` names the target and
`location.path` plus `location.span` name the exact source position. One line to list
the actionable rows:

`feedback` is the grouped PR presentation: Fixes, summary-only Checks, and an Existing
count. Use it to orient, but use the raw finding rows below for exact repair evidence.

```sh
jq -r '.payload.findings[]
  | select(.effective_disposition != "record")
  | [.effective_disposition, .kind,
     ((.location.path | strings) // "-"),
     ((.key_input.scope.normalized_target_intent.path | strings) // "-")]
  | @tsv' report.json
```

## Fixing findings

Fix what the row points at: restore a missing target or correct the link, make a
trailing slash agree with what the path is, reread prose whose referenced code changed.
Never weaken `.amiss/scanner-policy.json` to silence a finding; policy can only raise
severity, and loosening it is itself a blocking finding. Never delete a document just
to clear a finding. If a finding names drift you cannot verify, say so instead of
guessing.

The full reference is the book at https://hardmax71.github.io/amiss/, with every
finding kind's meaning at https://hardmax71.github.io/amiss/profiles.html and every
error code's at https://hardmax71.github.io/amiss/limits.html.
