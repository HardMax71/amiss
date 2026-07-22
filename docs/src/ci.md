# Running it in CI

The short form is the published GitHub convenience Action. It carries the engine inside the
selected action tree, derives both commits from the triggering event, and turns findings into
file feedback on the pull request. It is not the provider-authenticated controller lane:

```yaml
- uses: actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0 # v7.0.0
  with:
    fetch-depth: 2
- uses: HardMax71/amiss@v0
  with:
    profile: observe
```

The published first run uses `observe`: introduced problems appear as Fixes without blocking,
changed targets appear as summary-only Checks, and pre-existing problems remain Existing
inventory. An incomplete or untrusted run still fails. Triage the initial report, adopt any
repository policy it needs, then change the input to `profile: enforce` to make blocking
findings fail.

The moving major ref follows the engine's semver major: `v0` for the 0.x series, `v1`
from 1.0.0 on, so one series can never rewrite another's ref. A conventional `vX.Y.Z`
source tag is an immutable exact pin whose dispatcher delegates to the equally immutable
`action/vX.Y.Z` runtime tag. Pin `action/vX.Y.Z` directly, or its generated Action commit,
when policy requires the complete runtime tree in one ref. A source commit pins the dispatcher
but still makes that immutable second hop.

Before running anything, the action verifies the selected binary against the release
manifest shipped in the same tree. A wall-clock watchdog backstops the engine's
resource ceilings: 120 seconds unless the `watchdog-seconds` input moves it, and a scan
that outlives the window is ended so the job fails with no result. The action fails
the job on exit classes 1 and 2 under the default `enforce` profile and exposes
`exit-class` and `report` outputs for anything downstream. Its inputs (`profile`,
`base`, `candidate`, `repo`, `object-format`, `annotations`, `watchdog-seconds`) exist
for the cases the defaults do not cover.

For pull requests the derived base is the candidate's own first parent, never the
event payload's base tip; with the default candidate that parent is exactly the base
GitHub built the test merge from, and the rule holds unchanged when a workflow passes
its own candidate. The payload races the merge ref GitHub rebuilds lazily after a base
branch moves, and the first parent is present in any checkout that holds the candidate
at all.

The identity host comes from the event's server URL. On GitHub Enterprise Server the
report therefore claims the instance's own host and recognizes that host's blob and tree
links, with the github dialect declared explicitly. Release assembly supplies the host
the same way, to a
[manifest builder](https://github.com/HardMax71/amiss/blob/main/crates/amiss-bootstrap/src/build.rs)
that stores an open build-source identity instead of assuming `github.com`; the
[release workflow](https://github.com/HardMax71/amiss/blob/main/.github/workflows/release.yml)
is a checkable example of that input. The report and request formats are forge-neutral. This
repository currently ships only the GitHub convenience event wrapper, not a concrete
provider-authenticated adapter.

The long form invokes the engine directly. It is useful outside GitHub Actions or when a
workflow needs to construct the exact evaluation itself, but it is not the repository's
dogfood path. Amiss's
[self-scan workflow](https://github.com/HardMax71/amiss/blob/main/.github/workflows/ci.yml)
builds the pull request's engine, assembles a local action tree with its manifest, and runs
that composite under `--profile enforce`. A minimal adjacent-commit direct invocation is:

```yaml
- uses: actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0 # v7.0.0
  with:
    fetch-depth: 2
    persist-credentials: false
- run: cargo install --locked --registry crates-io --version '=<reviewed-version>' amiss
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
      --profile observe --format json > amiss-report.json
```

Replace `<reviewed-version>` with the exact release you reviewed. The leading `=` makes the
Cargo requirement exact instead of selecting another compatible release. The direct form
then pins both the released crate and its packaged dependency graph: Cargo checks the
downloaded crate archive against the SHA-256 checksum in the crates.io index, while
`--locked` refuses to recompute the packaged lockfile. The placeholder is deliberately
release-independent: the book does not copy each patch version out of the workspace package
metadata that release-plz updates.
As with the Action form, graduate this command to `--profile enforce` after the first report is
triaged.

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

The Action invokes the public command, so its branch is the candidate/source ref used for URL
resolution and its report target ref is null. It does not acquire provider-authenticated
external controls, invoke the sealed bootstrap path, or publish through an independently
authenticated integration. Using the open identity fields alone does not turn caller-supplied
JSON into provider authority.

The repository now contains two internal foundations for a future required-check lane. For
commit-pair materialization, the sealed bootstrap accepts a canonical
evaluation/snapshot/controls request triplet, verifies its constraint and trusted-time bindings,
and frames its exact bytes to a verified engine over stdin. The separate nested Rust controller
defines provider-neutral delivery and retry contracts, implements their local file record, and
contains bounded GitHub, GitLab Standard Webhooks, and Gitea-family signature verifiers;
[Controller delivery](controller.md) documents that mechanism. Neither foundation supplies a
supported provider transport or integration, so there is no configuration snippet for that lane
yet. See
[Project status](status.md) for the exact boundary.

When a run blocks, use the grouped feedback to orient, then read the exact JSON findings for
repair evidence. The human and Action views stop at ten items and state the overflow. The
blocking rows remain the report's `errors` and findings whose `effective_disposition` is
`fail`.

The same check runs before a commit exists. The repository publishes a
[pre-commit](https://pre-commit.com) hook that scans the staged index against `HEAD`
with an installed `amiss` binary:

```yaml
repos:
  - repo: https://github.com/HardMax71/amiss
    rev: v0.5.1
    hooks:
      - id: amiss
```

The action's `report` output names that JSON file, so a later step or a tool reads it
without rerunning anything. One line lists every grouped PR item with its target and
affected-place count:

```sh
jq -r '.payload.feedback
  | select(.status == "available")
  | .items[]
  | [.action, .effective_disposition,
     ((.target | strings) // "-"), .location_count]
  | @tsv' amiss-report.json
```

The Action shows at most ten Fix and Check items combined, in engine order, with one overflow
line. Only a displayed Fix with a candidate text location becomes a file annotation; Checks
and Existing inventory stay in the summary and report. If the scan failed, feedback is
unavailable and at most ten retained errors are annotated instead. The complete grouped and
raw sets always remain in the report.
