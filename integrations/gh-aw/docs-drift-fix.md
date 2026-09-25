---
on:
  schedule: weekly
permissions:
  contents: read
engine: claude
checkout:
  - fetch-depth: 0
network:
  allowed:
    - defaults
    - rust
safe-outputs:
  create-pull-request:
    title-prefix: "[docs-drift] "
    labels: [documentation]
---

# Repair the documentation drift Amiss found

Install Amiss with `cargo install --locked amiss`, pinning the exact version this
repository's CI pins. Then scan the current tree against the last release before it, or
against the first commit when no release is tagged:

```sh
if tag="$(git describe --tags --abbrev=0 HEAD~1 2>/dev/null)"; then
  base="$(git rev-parse "$tag^{commit}")"
else
  base="$(git rev-list --max-parents=0 HEAD | tail -n 1)"
fi
identity=(--repository "${GITHUB_SERVER_URL#https://}/${GITHUB_REPOSITORY,,}" --forge github
  --ref "$GITHUB_REF" --default-branch-ref "$GITHUB_REF")
amiss check --repo . --object-format sha1 \
  --base "$base" --candidate "$(git rev-parse HEAD)" "${identity[@]}" \
  --profile enforce --format json > amiss-report.json
```

The identity flags let Amiss check a URL into this repository's own files as it checks a
relative link; the scheduled run sits on the default branch, so `GITHUB_REF` names both refs.
Read `amiss-report.json`. Work only from the rows: the actionable ones are `errors[]`
and the findings whose `effective_disposition` is not `record`. Every row carries a
`description` stating what it means, and `location.path` with `location.span` naming the
exact source position. The `feedback` block is the grouped PR view; do not substitute it
for the raw evidence when deciding an automated edit.

Apply the edits the engine already proved first. A finding with a `fix` names the
document, the byte span and the replacement; stage the tree and let Amiss apply them all:

```sh
git add -A
amiss fix --repo . --object-format sha1 \
  --base "$base" --index "${identity[@]}" --profile enforce
```

Then repair by hand only what you can prove from the repository itself:

- A missing target whose `candidate_fact.evidence.resolution.same_object_at` names a
  path: the same file bytes live there now, so relink to that path.
- A missing target that never existed or was deleted deliberately: remove or correct
  the reference, quoting the deleting commit in the pull request body.
- A type mismatch from a trailing slash: make the link agree with what the path is.
- Changed content under unchanged prose (`dependency-changed-subject-unchanged`):
  reread the paragraph against the changed target and rewrite it only where the change
  made the prose false; leave true prose alone.

Never edit `.amiss/scanner-policy.json`, never delete a document to clear a finding,
and never invent link targets. Rerun the scan after your edits and include its result
in the pull request body. List any finding you chose not to touch, with one line on
why. Open a single pull request with everything, or no pull request if nothing needed
repair.
