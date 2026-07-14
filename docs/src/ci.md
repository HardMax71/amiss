# Running it in CI

Amiss runs on its own repository under `--profile enforce`, and the job is four commands long.
The shape below is lifted from this repository's workflow.

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

Three details in that shape are load-bearing. `fetch-depth: 2` exists because a scan needs a
base, and on a merge checkout the parent of `HEAD` is the pull request's base. The repository
and branch names arrive as environment variables rather than as workflow expansions inside the
script, because a branch may be named anything, and text expanded into a shell script is code.
And the owner is lowercased in shell because GitHub reports it with the capitals the owner
signed up with, while the scanner requires the canonical form and refuses anything else.

On a pull request, evaluate the merge result against its first parent, exactly as above. On a
push, the same two commits work. The scan is a pure function of the two trees, so there is no
cache to keep warm and no state to restore between runs.

When the run blocks, read the JSON report rather than the human projection: the projection is
capped at the first two hundred findings in canonical order, so in a repository carrying
hundreds of advisory records it can print every one of them and still never show the row that
blocked. The blocking rows are the report's `errors` array and the findings whose
`effective_disposition` is `fail`.
