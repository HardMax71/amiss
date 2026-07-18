# Working with agents

Amiss meets coding agents in two directions. The reactive one is the gate: a pull
request fails, and everything the agent needs travels with the failure. Annotations
carry each finding's fixed description and the normalized target path. The job summary
lists every failing kind once with its sentence. Each report row explains itself. Even a
rejected invocation teaches, printing the closed grammar on stderr, so an agent with no
book at hand can construct a working command from the refusal alone.

The preemptive direction is telling your repository's agents to check before they push.
If your repository keeps an `AGENTS.md`, paste this section into it:

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

## The deterministic sensor in a continuous-AI loop

GitHub's agentic workflows run coding agents inside Actions and describe themselves as
augmenting deterministic CI rather than replacing it. Amiss is the deterministic half
of that pairing: it finds drift and refuses to guess, and an agent repairs what it
found. A starting recipe lives at
[`integrations/gh-aw/docs-drift-fix.md`](https://github.com/HardMax71/amiss/blob/main/integrations/gh-aw/docs-drift-fix.md):
copied into `.github/workflows/` and compiled with the `gh aw` extension, it runs the
scan on a schedule, reads the report, repairs the drift it can prove, and opens a pull
request that passes back through the same gate it started from.

## Claude Code

This repository doubles as a Claude Code plugin marketplace. One command registers it:

```text
/plugin marketplace add HardMax71/amiss
```

Installing the `amiss` plugin from that marketplace adds a skill that knows the
invocation grammar, the exit classes, and the fix loop; its text is maintained at
[`integrations/claude`](https://github.com/HardMax71/amiss/blob/main/integrations/claude/skills/amiss/SKILL.md).
