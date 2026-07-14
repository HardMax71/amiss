# Running it in CI

Amiss runs on its own repository under `--profile enforce`, and the job is four commands.
This shape is lifted from this repository's workflow:

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
