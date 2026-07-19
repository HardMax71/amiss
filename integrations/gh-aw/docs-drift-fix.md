---
on:
  schedule: weekly
permissions:
  contents: read
engine: claude
safe-outputs:
  create-pull-request:
    title-prefix: "[docs-drift] "
    labels: [documentation]
---

# Repair the documentation drift Amiss found

Install Amiss with `cargo install --locked amiss`, pinning the exact version this
repository's CI pins. Then scan the current tree against the last release:

```sh
base="$(git rev-parse "$(git describe --tags --abbrev=0)" 2>/dev/null || git rev-parse HEAD~50)"
amiss check --repo . --object-format sha1 \
  --base "$base" --candidate "$(git rev-parse HEAD)" \
  --profile enforce --format json > amiss-report.json
```

Read `amiss-report.json`. Work only from the rows: the actionable ones are `errors[]`
and the findings whose `effective_disposition` is not `record`. Every row carries a
`description` stating what it means, `location.path` with `location.span` naming the
exact source position, and for reference findings
`key_input.scope.normalized_target_intent.path` naming the target.
The `feedback` block is the grouped PR view; do not substitute it for the raw evidence
when deciding an automated edit.

Repair only what you can prove from the repository itself:

- A missing target whose file was renamed in history: update the link to the new path.
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
