# Market evidence and positioning

Date: 2026-07-10

Correction (2026-07-11): the “empty quadrant” and standalone-build conclusions below are
superseded by [market-reassessment.md](./market-reassessment.md). A later search found a second
active OSS drift tool, confirmed that Fiberplane `drift` now uses a committed signature lock, and
found vendor-reported evidence of substantial lockfile merge-conflict rates. The problem evidence
remains useful; competition, differentiation, and build-vs-extend conclusions must use the later
review.

This file collects the demand evidence for the standalone tool: how big the problem measurably is,
why this is the window, who would pay, what already competes, and the strongest case against.
Numbers are cited to their source; where only secondary coverage of a gated report was reachable,
the sentence says so. The mechanism-level comparison with other tools lives in
[prior-art.md](./prior-art.md).

## The problem, measured

| Source | Finding |
| --- | --- |
| Postman State of the API 2023 (about 37,000 respondents) | 52% call lack of documentation the top obstacle to consuming APIs; asked what would most improve docs, 57% answer "up-to-date documentation", ahead of code samples (55%) and better examples (53%) |
| Postman State of the API 2024 (about 5,600) | 39% call inconsistent documentation the biggest roadblock to API collaboration; 44% dig through source code to understand APIs |
| Stack Overflow survey insights 2024 | 68% of developers hit a knowledge silo at least weekly; under half agree they can easily surface up-to-date information inside their organization |
| Atlassian State of Developer Experience 2024 (2,100+) | 69% lose eight or more hours a week to inefficiencies; developers name insufficient documentation among the top causes, leaders blame understaffing |
| DORA 2021 | Teams with quality documentation are 2.4 times more likely to hit top delivery performance; press coverage of the report put roughly one developer in four at that documentation level |
| DORA 2022 | Documentation quality multiplied the measured benefit of every technical practice studied; the predicted benefit of continuous integration was 750% with above-average docs versus 34% below |
| Aghajani et al., ICSE 2019 and 2020 | 878 documentation artifacts classified into 162 issue types; correctness and up-to-dateness dominate, and surveyed practitioners explicitly ask for automated tooling that keeps docs current |
| Ibrahim et al., JSS 2012 | Deviations from a project's usual comment-update behavior (code changed, comment left stale) predict elevated later defect rates |

Caveats worth keeping attached: Postman's sample fell from 37,000 to about 5,600 between 2023 and
2024, so year-over-year trend claims are weak; SmartBear's report is gated and its exact
freshness-challenge percentage could not be verified; several DORA figures above were read through
the project's capability pages and press coverage rather than raw report tables.

The numbers agree on the shape: documentation exists, people depend on it, and its failure mode is
not absence but staleness. That matches user zero exactly, a repository with 89 documentation
pages, roughly a dozen bespoke defenses, and seven live drift classes anyway.

## Why now

The consumption side changed first. DORA 2025 reports 90% of developers using AI at work. Mintlify,
which hosts documentation for 20,000+ companies, reported in April 2026 that nearly half of the
traffic to the docs it hosts comes from AI agents. Context7, an MCP server whose entire pitch is
"your model's training data has stale docs, fetch fresh ones", sits around 57,000 GitHub stars and
about a million uses a week. Three conventions appeared in two years to standardize how machines
discover documentation (llms.txt in 2024, AGENTS.md in 2025 and now under the Linux Foundation, MCP
documentation servers throughout), and none of them verifies that what gets discovered is true. A
human reader discounts a stale sentence from context; an agent executes it.

The production side changed in the same direction. DORA 2024 measured delivery stability falling
7.2% as AI adoption rose; Faros AI's analysis has AI-assisted teams opening 98% more pull requests
while review time rises 91%. Code now outruns the prose describing it by a wider margin than ever,
while the people who used to absorb the difference are being cut (Canva laid off ten of its twelve
technical writers in March 2025; a veteran practitioner's estimate puts the profession's recent
contraction near 30%).

And a new documentation class appeared whose reader is exclusively a machine: CLAUDE.md, AGENTS.md,
.cursorrules, llms.txt. Practitioner writing already describes the characteristic failure (several
instruction files saying almost the same thing, one gets updated, agents keep reading the others).
User zero has all of these files today, referencing concrete paths, defaults, and commands, with
nothing checking them. This category is two years old, growing, and unserved; it is the sharpest
wedge in the catalog (UC-12 in [use-cases.md](./use-cases.md)).

One more demand signal deserves its own line: DORA frames AI as an amplifier whose payoff scales
with the reliability of internal information, and its 2024 model predicts documentation quality
rising with AI adoption more than any other measured factor. That gives a platform team a stated,
survey-backed reason to fund documentation assurance as AI infrastructure rather than as hygiene.

## Who pays

Platform and developer-experience teams are the natural first buyer; the DORA amplifier framing is
the budget argument, and the deterministic layer's output (a claim graph with owners and states) is
the kind of artifact those teams already report on. AI-tooling teams are the second: an agent
platform that can prefer attested statements over unattested ones has a concrete reliability story.
Compliance is the third and slowest: SOC 2 and ISO 27001 audits treat procedures that do not match
practice as deficiencies, and the regulated-traceability world (DO-178C, ISO 26262) already pays
Jama, Polarion, and Codebeamer for suspect-link workflows, which is evidence that attestation
mechanics command money when a standard demands them.

Willingness-to-pay reference points: Swimm raised $33.3M total but nothing since 2021; Mintlify
raised a $45M Series B at a $500M valuation in April 2026 on the docs-for-AI thesis; DeepDocs
charges $30 per seat per month for AI doc updates; Dosu's published CI recipe prices its advisory
LLM pass at an estimated five to fifteen cents per pull request. The deterministic core of this
tool costs nearly nothing to run, which supports an open core with paid workflow surface (GitHub
App, dashboards, audit trail, agent API) rather than paid checking.

## Competition and the formerly claimed empty quadrant

The landscape sorts into four schools, detailed mechanism by mechanism in
[prior-art.md](./prior-art.md). Re-anchoring: Swimm heuristically relocates coupled snippets and
tokens and escalates to a human when unsure; verification stops at the coupled elements. Regenerate:
Speakeasy, Fern, Stainless, and the spec-refresh features of GitBook and Theneo keep generated
reference fresh and leave surrounding prose unmanaged. Execute: Doc Detective, Runme, and the
doctest family prove that embedded procedures still run, and say nothing about claims. AI-rewrite:
Mintlify's agent, DeepDocs, DocuWriter, and GitBook's agent open generated doc-update PRs whose
only gate is a human reviewer.

The second pass claimed nobody occupied the intended quadrant. That is no longer a safe market
statement. Fiberplane `drift` (open source, v0.10.1 in June 2026, 119 stars on the later review
date) ships explicit anchors, tree-sitter-normalized fingerprints in a lockfile, CI failure,
relinking, and reverse lookup. `ryanwaits/drift` separately ships TypeScript documentation rules,
example checks, a coverage ratchet, JSON, agent workflows, and a GitHub Action. Swimm remains active
in the enterprise. Typed cross-artifact claims and honest governed lifecycle may still
differentiate, but only after the build-vs-extend and user-validation gates in
[market-reassessment.md](./market-reassessment.md).

Two housekeeping notes from the research: Jama holds patents on suspect-link management (US
8,266,591 and relatives), which deserves a legal skim before the attestation workflow ships
commercially; and the name "tether" used as a working label in this dossier collides with a large
cryptocurrency and would need replacing.

## The bear case

The strongest counterargument is incentives, not mechanism. The Hacker News reception of Swimm and
Mintlify is a catalog of it: wrong documentation is worse than none, there are close to zero
incentives to maintain docs, and a determined engineer believes they can build the checker in an
afternoon. Swimm itself, the best-funded attempt at code-coupled docs, has raised nothing since
2021, and its adoption complaint was workflow friction rather than technical failure. Employers
cutting technical writers can be read as low willingness to pay for documentation quality in
general.

The second counterargument is that the regeneration school wins outright: if agents can answer
questions from code on demand (Context7, DeepWiki-style generated wikis, Mintlify's agent), durable
prose becomes a disposable cache, and nobody verifies a cache. Google's John Mueller adds a
deflating data point on the machine-consumption story: server logs show AI services do not actually
fetch llms.txt, so part of the docs-for-agents movement may be cargo cult.

The rebuttals worth recording. Regeneration without provenance is drift with extra steps; user
zero's railroad diagrams regenerate on every build from a stale embedded copy of the grammar, and
the Mintlify-school reviewer is the only gate on generated updates. If prose becomes a regenerated
cache, the cache needs exactly what this tool records: which statements are load-bearing, what
evidence they rest on, and who vouched when. And the afternoon-build objection is half right, which
is a positioning instruction rather than a refutation: the deterministic core must install in
minutes and be obviously better than the afternoon version (anchor migration, normalization,
fan-out grouping, audit trail), because the afternoon version is the real competitor.

## What the evidence says the product must be

- A CLI plus a CI action with committed state, no platform dependency, and no full-history
  requirement; Swimm's clone constraint and workflow friction are the documented adoption killers.
  Claims are authored in the docs themselves, never in per-document configuration.
- Deterministic checking free and fast; the paid surface is workflow (App UX, audit trail,
  dashboards, agent API), and any LLM lane is metered and advisory, in Dosu's cost range.
- Agent-readable output of claim states from day one, because half the readership is now machines
  and that is the buyer's freshest budget line.
- An install-to-first-value path measured in minutes on an existing repository, aimed squarely at
  the engineer who would otherwise build the afternoon version.
- Honest states everywhere; the one unforgivable product sin in this category is a green that
  means nothing, which is how every freshness-date regime before it died.
- Useful before anything is authored: the zero-configuration pass on an unmodified repository
  must already catch broken references and flag stale sections, or the afternoon-build objection
  wins.
