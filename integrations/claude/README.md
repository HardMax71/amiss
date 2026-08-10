# The Amiss plugin for Claude Code

One skill that teaches an agent to run Amiss well: the closed invocation grammar, the
three exit classes and what each obliges, the staged pre-commit loop, and how to apply
the proof-gated fixes a blocking report carries. The skill text lives at
[skills/amiss/SKILL.md](skills/amiss/SKILL.md) and assumes an installed `amiss` binary
(`cargo install --locked amiss`, pinned to the version the repository's CI reviews).

The repository doubles as its own marketplace, so installation is two commands:

```text
/plugin marketplace add HardMax71/amiss
/plugin install amiss
```

The plugin's version tracks the plugin content, not the engine crate: it moves when the
skill text moves, which is deliberate, so an unchanged skill never churns updates on
engine releases. What the scanner itself is and does is the book's job:
https://hardmax71.github.io/amiss/ starts at the introduction, and the
[Working with agents](https://hardmax71.github.io/amiss/agents.html) chapter states how
the gate and an agent divide the work.
