# Running it in CI

The short form is the published action, which carries the engine inside the selected
action tree, derives both commits from the triggering event, and turns findings into file
annotations on the pull request:

```yaml
- uses: actions/checkout@<pinned-sha>
  with:
    fetch-depth: 2
- uses: HardMax71/amiss@v0
```

The moving major ref follows the engine's semver major, `v0` for the 0.x series and
`v1` from 1.0.0 on, so one series can never rewrite another's ref; `action/vX.Y.Z`
tags are immutable exact pins, and pinning the full commit id works as everywhere. The action verifies the selected binary against
the release manifest shipped in the same tree before running it, fails the job on exit
classes 1 and 2 under the default `enforce` profile, and exposes `exit-class` and
`report` outputs for anything downstream. Its inputs (`profile`, `base`, `candidate`,
`repo`, `object-format`, `annotations`) exist for the cases the defaults do not cover.
The identity host comes from the event's server URL, so on GitHub Enterprise Server
the report claims the instance's own host and recognizes that host's blob and tree
links, with the github dialect declared explicitly; nothing about the action assumes
github.com.

The long form invokes the engine directly. It is useful outside GitHub Actions or when a
workflow needs to construct the exact evaluation itself, but it is not the repository's
dogfood path. Amiss's
[self-scan workflow](https://github.com/HardMax71/amiss/blob/main/.github/workflows/ci.yml)
builds the pull request's engine, assembles a local action tree with its manifest, and runs
that composite under `--profile enforce`. A minimal adjacent-commit direct invocation is:

```yaml
- uses: actions/checkout@<pinned-sha>
  with:
    fetch-depth: 2
    persist-credentials: false
- run: cargo install --locked --registry crates-io --version 0.4.0 amiss
- env:
    REPOSITORY: ${{ github.repository }}
    BRANCH: ${{ github.head_ref || github.ref_name }}
    DEFAULT_BRANCH: ${{ github.event.repository.default_branch }}
  run: |
    amiss check --repo . --object-format sha1 \
      --base "$(git rev-parse HEAD~1)" \
      --candidate "$(git rev-parse HEAD)" \
      --repository "github.com/${REPOSITORY,,}" \
      --ref "refs/heads/${BRANCH}" \
      --default-branch-ref "refs/heads/${DEFAULT_BRANCH}" \
      --profile enforce --format json > amiss-report.json
```

The direct form pins both the released crate and its packaged dependency graph. Cargo checks
the downloaded crate archive against the SHA-256 checksum in the crates.io index, while
`--locked` refuses to recompute the packaged lockfile.

Three details carry weight. Both named commits must exist in the checkout. The Action derives
pull-request base and merge-result commits, merge-group base and head commits, or push
`before` and head commits from the event; explicit `base` and `candidate` inputs override
them. `fetch-depth: 2` is normally enough for the pull-request merge checkout, while a batched
push or unusual checkout may require `fetch-depth: 0`. The repository and branch names go
through environment variables instead of being pasted into the script by the workflow
engine, because a branch can be named anything, and text pasted into a shell script becomes
code. The owner is lowercased in shell because GitHub hands it over with its original
capitals while Amiss requires the lowercase form and refuses anything else.

For a direct adjacent-commit scan, `HEAD~1` and `HEAD` work as above. For event-aware runs,
prefer the composite Action's derivation or pass the exact two commits explicitly. A scan is
a pure function of the two snapshots and invocation, so there is no baseline cache to warm
or restore between runs.

When a run blocks, read the JSON, not the human printout. The printout stops at two hundred
findings, so a repository with hundreds of harmless records can fill the screen and still
not show the row that blocked. The blocking rows are in the report's `errors` array and in
the findings whose `effective_disposition` is `fail`.
