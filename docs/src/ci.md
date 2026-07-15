# Running it in CI

The short form is the published action, which carries the engine inside the pinned
tree, derives both commits from the triggering event, and turns findings into file
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

The long form is the engine invoked directly, which is how Amiss runs on its own
repository under `--profile enforce`, and the job is four commands. This shape is
lifted from this repository's workflow:

```yaml
- uses: actions/checkout@<pinned-sha>
  with:
    fetch-depth: 2
    persist-credentials: false
- run: cargo install amiss
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

Three details carry weight. `fetch-depth: 2` exists because a scan needs a base commit, and
on a pull request checkout the parent of `HEAD` is the pull request's base. The repository
and branch names go through environment variables instead of being pasted into the script by
the workflow engine, because a branch can be named anything, and text pasted into a shell
script becomes code. And the owner is lowercased in shell because GitHub hands it over with
its original capitals while Amiss requires the lowercase form and refuses anything else.

On a pull request, compare the merge result against its first parent, as above. On a push,
the same two commits work. A scan is a pure function of the two trees, so there is no cache
to warm and nothing to restore between runs.

When a run blocks, read the JSON, not the human printout. The printout stops at two hundred
findings, so a repository with hundreds of harmless records can fill the screen and still
not show the row that blocked. The blocking rows are in the report's `errors` array and in
the findings whose `effective_disposition` is `fail`.
