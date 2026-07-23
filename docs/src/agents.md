# Working with agents

Amiss meets coding agents in two directions: as the gate an agent runs into, and as a
check the agent runs itself before pushing.

## The failing gate

When a pull request fails, everything the agent needs travels with the failure.
Annotations point to introduced Fixes. Grouped Checks and Existing inventory stay in the
job summary and report, and the exact finding and error rows carry their fixed
descriptions. Even a rejected invocation teaches: it prints the closed grammar on stderr,
so an agent with no book at hand can construct a working command from the refusal alone.

## Check before pushing

Tell your repository's agents to scan before they push. If the repository keeps an
`AGENTS.md`, paste this section into it:

```markdown
## Documentation checks

This repository gates documentation drift with Amiss
(https://hardmax71.github.io/amiss/). After changing documentation, or code that
documentation points at, check the staged state before committing:

    amiss check --repo . --object-format sha1 \
      --base "$(git rev-parse HEAD)" --index --profile enforce --format json

Exit 0 passes. Exit 1 blocks: the blocking rows are `errors[]` and the findings whose
`effective_disposition` is `fail`; each row's `description` says what it means and how
to fix it, and `key_input.scope.normalized_target_intent.path` names the target. Exit 2
means the run itself could not be trusted, and the error rows say why. Fix what the row
points at; never weaken `.amiss/scanner-policy.json` to silence a finding.
```

The block assumes the binary is installed (`cargo install --locked amiss`); pin the
version your CI pins.

## Scheduled repair

GitHub's agentic workflows run coding agents inside Actions and describe themselves as
augmenting deterministic CI rather than replacing it. Amiss is the deterministic half
of that pairing: it finds drift and refuses to guess, and an agent repairs what it
found. A starting recipe lives at
[`integrations/gh-aw/docs-drift-fix.md`](https://github.com/HardMax71/amiss/blob/main/integrations/gh-aw/docs-drift-fix.md).
Copied into `.github/workflows/` and compiled with the `gh aw` extension, it runs the
scan on a schedule and reads the report. What it can prove it repairs, and the pull
request it opens passes back through the same gate it started from.

## Claude Code

This repository doubles as a Claude Code plugin marketplace. One command registers it:

```text
/plugin marketplace add HardMax71/amiss
```

Installing the `amiss` plugin from that marketplace adds a skill that knows the
invocation grammar, the exit classes, and the fix loop; its text is maintained at
[`integrations/claude`](https://github.com/HardMax71/amiss/blob/main/integrations/claude/skills/amiss/SKILL.md).
