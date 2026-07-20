<h1 align="center">Amiss</h1>

<p align="center">
  <a href="https://crates.io/crates/amiss"><img alt="version" src="https://img.shields.io/crates/v/amiss?style=flat-square&label=version&labelColor=1e293b&color=475569"></a>
  <a href="LICENSE.md"><img alt="license" src="https://img.shields.io/badge/license-FSL--1.1--ALv2-475569?style=flat-square&labelColor=1e293b"></a>
  <a href="https://scorecard.dev/viewer/?uri=github.com/HardMax71/amiss"><img alt="scorecard" src="https://img.shields.io/ossf-scorecard/github.com/HardMax71/amiss?style=flat-square&label=scorecard&labelColor=1e293b&color=475569"></a>
</p>

Amiss checks documentation against the tree it describes. It reads the documents in a
repository, follows the references they make into that same repository, and reports when a
reference stops resolving, or when the file behind it changed while the prose around it did
not. It reads structure, not meaning: it will not tell you whether a sentence is true, and it
does not guess.

The scanner engine keeps no state, executes nothing, never touches the network, and never
writes; identical inputs through the same engine binary produce byte-identical reports. A
reference that does not resolve fails the run under `enforce`; a file changing under an
unchanged paragraph is always a signal for a human, never a machine verdict.

```sh
cargo install amiss

amiss check --repo . --object-format sha1 \
    --base "$(git rev-parse HEAD~1)" --candidate "$(git rev-parse HEAD)" \
    --profile observe
```

In CI the same engine ships as an action that derives both commits from the event. It groups
related findings by target, shows Fixes before Checks, and annotates only Fixes introduced by
the pull request:

```yaml
- uses: actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0 # v7.0.0
  with:
    fetch-depth: 2
- uses: HardMax71/amiss@v0
  with:
    profile: observe
```

Start in `observe` so Fixes can be triaged without blocking pull requests; Existing problems
stay in the report, Checks stay in the summary, and an incomplete or untrusted run still fails.
Switch the input to `enforce` once the initial backlog and repository policy have been reviewed.

Coding agents get the same treatment as people: every finding and error row carries a
sentence saying what it means and what to do, a rejected invocation prints the whole
grammar, and the book's
[Working with agents](https://hardmax71.github.io/amiss/agents.html) chapter has a paste
block for your repository's `AGENTS.md`. The book is one fetch at
[llms.txt](https://hardmax71.github.io/amiss/llms.txt), or in full at
[llms-full.txt](https://hardmax71.github.io/amiss/llms-full.txt).

Everything else is in the [documentation](https://hardmax71.github.io/amiss/). Distribution terms
and third-party attributions are in [the license](LICENSE.md) and
[notices](THIRD_PARTY_NOTICES.md).
