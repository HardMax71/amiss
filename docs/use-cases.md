# Use cases, mined from user zero

Date: 2026-07-10

Status correction (2026-07-11): the observed use cases remain valid, but release labels and the
claim that day zero automatically flags stale sections are superseded by
[pre-implementation-review.md](./pre-implementation-review.md). Scanner v0 resolves structural
references and reports base-versus-candidate impact without a ledger. Governed and deterministic
claim kinds ship only after their specific contract and evidence gates.

The tool under design is standalone. This file is the bridge between it and the repository the
investigation started in: every use case below was observed in spec_to_rest, which serves as user
zero. That repository is a useful specimen because it sits at an extreme of documentation coupling:
a Scala compiler with Isabelle proofs, three code-generation targets, a published docs site, 22
workflows, and roughly a dozen bespoke drift defenses (transclusion, executable CLI snippets, five
golden suites, a proof-extraction diff gate, an ArchUnit rule, a link checker, a duplication gate).
Despite all of that, the audit found seven live drift classes. A repository that tries this hard
and still drifts is a requirements generator, not an outlier.

Each use case has three parts: the observed instance (paths verified in
[repo-audit.md](./repo-audit.md)), the general form as it appears in any comparable repository, and
what the tool does about it. Claim kinds referenced below: a link claim (a referenced path, anchor,
or URL resolves), a snippet claim (displayed code equals selected source), a value claim (a number
or name in prose equals an extracted value), an inventory claim (a documented set equals an
extracted set), a tree claim (a documented file tree matches a path set), a graph claim (documented
edges match an extracted dependency graph), a transcript claim (a documented command reproduces its
recorded output), a narrative claim (free prose attested against fingerprinted evidence), and an
external claim (prose depends on something outside the repository and re-attests on a schedule).
The first seven are deterministic. The last two are review obligations, not truth proofs; the
distinction is argued in [design.md](./design.md).

| ID | Use case | Main claim kinds | Earliest release |
| --- | --- | --- | --- |
| UC-01 | Architecture pages vs the build graph | inventory, graph, narrative | v1 |
| UC-02 | CLI reference vs the actual CLI | value, inventory, transcript | v1 |
| UC-03 | Embedded snippets and transcripts | snippet, transcript | v1 (wrap existing) |
| UC-04 | Published copies of generated artifacts | value, tree, snippet | v1 |
| UC-05 | DSL and grammar teaching pages | inventory, snippet, narrative | v2 |
| UC-06 | Reference tables mirroring code registries | inventory, value | v1 |
| UC-07 | Target pages describing generated output | tree, link, narrative | v1 |
| UC-08 | Proof and guarantee claims | inventory, value, narrative | v2 |
| UC-09 | Install, deploy, and operations runbooks | value, inventory, external | v2 |
| UC-10 | README capability claims and scorecards | value, link | v1 |
| UC-11 | Version pins and compatibility matrices | value, external | v1 |
| UC-12 | Agent-instruction files | link, value, narrative | v1 |
| UC-13 | Docs living outside the repository | external, narrative | v3 |
| UC-14 | Historical documents that must not be gated | none (lifecycle policy) | v1 |
| UC-15 | Diagrams as claims | graph, snippet | v2 |
| UC-16 | Claims about the outside world | external, link | v2 |

Day zero of the tool ([design.md](./design.md)) precedes every tier above with nothing authored:
it already covers each use case's broken-reference and stale-section slice, so the tier dates the
use case's full mechanism, not first value.

## UC-01: Architecture pages vs the build graph

The user-zero architecture page says the repository has ten workflows and names a `docs.yml` that
does not exist; the tree has 22 workflows. Three module counts coexist: the page's project tree
lists eleven modules, the ArchUnit test enumerates twelve layers, and thirteen directories sit on
disk. The same page attributes format and lint checks to the wrong workflow. An ArchUnit test
enforces the module layering in code, yet nothing connects that test to the prose describing the
same layering, so the code stayed correct while the page rotted.

Nearly every engineering organization keeps a page like this, and it is usually the first page a
new hire or an AI agent reads. The general form: prose inventories (modules, services, workflows,
queues, topics) and prose dependency descriptions, each derivable from a build file, an IaC tree,
or a directory listing, none actually derived.

The tool binds the section to inventory and graph claims: workflow filenames as a path-set,
module names and `dependsOn` edges extracted from the build definition. Counts inside prose become
value claims so "ten workflows" can never be hand-written again. The interpretive text around the
inventories (why the layering exists) stays a narrative claim with suspect-on-change semantics.

## UC-02: CLI reference vs the actual CLI

User zero's `cli.mdx` omits the public `synth accept` subcommand and documents a 0/1/2 exit-code
contract, while the code maps two failure classes to exit 3. Both errors sat next to a working
executable-snippet system; the snippets were fresh, the table beside them was wrong.

Any project that ships a CLI has this page, and `--help` output, flags, defaults, and exit codes
are the classic rot surface. The failure pattern generalizes: executable examples protect the paths
they execute and nothing else on the page.

The tool extracts the subcommand and flag inventory from `--help` (or from parser metadata when a
project exposes it) and compares it with the documented table as an inventory claim. Exit-code
tables become value claims against the constants that define them. Recorded runs become transcript
claims with a fingerprint of the binary that produced them, for the reasons in EC-C1 of
[edge-cases.md](./edge-cases.md).

## UC-03: Embedded snippets and transcripts

User zero already solves the copied-snippet problem well: a remark plugin transcludes `.spec`
fixtures at build time, so the displayed source cannot drift, and `<CliRun>` markers execute the
real CLI and diff normalized output against committed goldens. The audit's lesson is that these
mechanisms are excellent and narrow, and that a new tool should index them rather than replace
them.

The general form is the include-and-execute family: AsciiDoc tagged includes, mdBook includes,
Sphinx `literalinclude`, doctest and its relatives, cog- and mdox-style regenerated regions. Most
repositories have at most one of these, partially applied, and no inventory of which pages are
covered by it.

The tool treats an existing include or executable-doc mechanism as an assurance lane: the claim is
declared, the mechanism is named, and the tool verifies the mechanism ran and reports its coverage
alongside everything else. Where no mechanism exists, the tool's own snippet claims (marker-bounded
regions, symbol selectors) fill the gap.

## UC-04: Published copies of generated artifacts

Two user-zero cases. A published OpenAPI file claims to be identical to compiler output and differs
from the current golden by five stale `maxLength` lines. Worse, the railroad-diagram generator
regenerates its SVGs on every docs build from a second, hand-copied grammar embedded in the script,
so regeneration succeeds forever while faithfully reproducing a stale input.

The general form: a docs site publishes a copy of something generated elsewhere (an API spec, a
schema, a config sample, a diagram), and the copy has no derivation link back to the source of
truth. The second case is the subtle one: freshness of the generation step proves nothing if the
generator's input is itself a copy.

The tool expresses these as equality against the authoritative artifact (value or snippet claims
against the golden or schema), and, for generated-in-docs assets, a derivation claim whose selector
set covers the generator inputs, not just its output. EC-B1 develops the design consequence.

## UC-05: DSL and grammar teaching pages

User zero documents its spec language in prose. The parser page omits three tokens that the grammar
defines, shows a lexer-member example in the wrong implementation language, and a convention table
assigns `module:symbol` semantics to a property whose validator accepts only `live` or `redacted`.
Separately, the grammar has a precedence footgun (`implies` binds looser than `and`/`or`) that the
docs must warn about; if precedence ever changes, the warning silently inverts from helpful to
wrong.

Everything with a DSL, a query language, a config dialect, or a public grammar has these pages:
syntax references, operator tables, keyword lists. Grammars evolve in small diffs and the teaching
prose never participates in the diff.

The tool extracts token and rule inventories from the grammar file as inventory claims, binds
property tables to the validator's accepted-value sets as value claims, and attaches the precedence
warning to a narrative claim whose selector is the relevant grammar rules, so a precedence change
flips the warning to suspect.

## UC-06: Reference tables mirroring code registries

User zero's lint table currently matches the seven registered passes, code for code, level for
level. It is the one hand-maintained table the audit found in sync, and it stays in sync only
because recent PRs happened to co-change it. The migration-operation matrix nearby already omits
one rejection rule the code enforces.

The general form is a table whose rows mirror a registry in code: error codes, lint rules, feature
flags, metric names, permission scopes. These tables are load-bearing (users grep for the codes)
and they are exactly extractable.

This is the tool's cheapest high-value claim: an inventory claim comparing the documented set with
the extracted registry, plus value claims for per-row attributes. Low noise, deterministic, and the
suspect state is almost always a genuine omission.

## UC-07: Target pages describing generated output

User zero's Python/Postgres target page shows a file tree that omits four files present in the
golden output tree and links a source file deleted in a refactor. Nine such target pages exist, one
per emitter dialect, each hand-describing what the generator produces while byte-exact goldens of
the same output sit in the repository unreferenced.

The general form: any product that generates artifacts (SDK generators, scaffolding tools,
compilers) documents "what you get" by hand while a test fixture already pins the truth. The docs
and the fixture rot independently.

The tool binds the documented tree to a tree claim over the golden's path set, and the prose links
to link claims. The nearest stable artifact wins as the anchor: the golden tree, not the emitter
internals, per EC-B3.

## UC-08: Proof and guarantee claims

User zero's proof docs say two Isabelle sessions are independent leaves when the session `ROOT`
makes one depend on the other, and count 23 theories where there are 24. These pages back the
repository's strongest marketing claim (machine-checked soundness), which makes silent inaccuracy
in them uniquely embarrassing.

The general form covers any repository that advertises a guarantee with formal or semi-formal
backing: proof structure, coverage percentages, threat-model claims, "all handlers are authorized"
style assertions. The guarantee's evidence is machine-readable (session files, coverage reports,
policy-test output); the prose describing it is not.

The tool extracts session, theory, and dependency inventories from the proof system's manifest as
inventory and graph claims, binds stated counts as value claims, and leaves the meaning of the
theorem (what soundness covers and excludes) as a narrative claim over the relevant theory files.

## UC-09: Install, deploy, and operations runbooks

User zero duplicates its platform-to-archive mapping across `install.mdx`, `action.yml`, and the
native-build workflow. Its playground deploy has a runbook subtlety recorded only in a maintainer's
memory notes: the fly.io image bakes the latest release, so after each release someone must run the
deploy workflow by hand or production serves stale behavior while docs describe the new release.

Runbooks are the highest-stakes doc class in most organizations and the least mechanically checked.
The general form: multi-surface duplication (README, workflow, action manifest, Dockerfile) plus
procedures whose truth depends on infrastructure state the repository cannot see.

The tool handles the duplication with cross-file value and inventory claims (three files claim the
same mapping; one claim binds all three). Procedure steps that name workflows, scripts, or flags
get link and value claims. The infrastructure-dependent residue becomes external claims on a
re-attestation schedule, because no repository diff will ever invalidate them.

## UC-10: README capability claims and scorecards

User zero's README states its target matrix (three stacks by three databases), a zero-sorry
soundness theorem, and a native conformance suite; each claim has real ground truth somewhere (the
profile registry, the proof session, three build workflows) and none is bound to it. The landing
page goes further: its hero terminal hard-codes `21/21 consistency checks passed (212ms)`, literal
check names, and a hand-pasted "(emitted)" code sample, and nothing regenerates any of it.
Maintainer notes show per-spec verification scores changing with nearly every proof campaign; every
such number was true at some commit.

READMEs are the most-read and most-scraped file in any repository, the primary input to package
registries and AI training sets, and effectively marketing. The general form: capability lists,
benchmark numbers, matrix-of-supported-things tables, all hand-written.

The tool binds each number to a value claim whose selector is the command or artifact that produces
it (a test count, a golden inventory, a benchmark output file), and each capability list to an
inventory claim. Where a number is expensive to reproduce (a benchmark), the claim records the
producing run's fingerprint and age instead, and re-attestation is scheduled rather than per-PR.

## UC-11: Version pins and compatibility matrices

User zero pins scalafmt 3.8.3 (because the proof-extraction diff gate depends on stable
formatting), an Isabelle release, Z3 and cvc5 versions, and Node and Scala versions across
`build.sbt`, workflow files, and docs prose. The docs mention several of these by number. LLM model
identifiers appear in committed synthesis-cache metadata, and the cache silently mismatches when
the compile default model differs from the cache's model key.

Every project with a toolchain has this: versions stated in prose, duplicated across build files
and CI, plus identifiers whose meaning is controlled by an external vendor. The pattern extends
beyond versions to model IDs, API endpoint names, and quota numbers.

Version statements in docs become value claims captured from the authoritative build file.
Cross-file pin duplication becomes one claim binding all sites. Vendor-controlled identifiers get
external claims: the repository can prove internal consistency, and only a scheduled probe or a
human on a timer can vouch for the outside world.

## UC-12: Agent-instruction files

User zero's `CLAUDE.md` names specific files (`modules/verify/.../z3/Backend.scala`, test suites to
imitate, `VerificationConfig.Default` as the single source of defaults) and specific commands
(`sbt scalafixAll`); an `AGENTS.md` delegates to it. Its maintainer keeps auxiliary memory files
that reference paths, flags, and workflows. The docs site already serves `llms.txt` and
`llms-full.txt` routes that aggregate its pages for machine readers, inheriting whatever drift the
pages carry. These files steer an AI agent's behavior on every session; a renamed path or changed
default silently degrades every future run, and nothing checks any of them today.

This category barely existed two years ago and now ships in a large fraction of active
repositories: `CLAUDE.md`, `AGENTS.md`, `.cursorrules`, `llms.txt`. They are documentation whose
primary reader executes instructions without skepticism, which makes their accuracy more
load-bearing than human-facing docs, and their audience cannot complain.

The tool treats them as first-class documents: link claims for every referenced path, value claims
for named defaults and commands, narrative claims for behavioral guidance bound to the code it
describes. This is also the sharpest go-to-market wedge the research identified, because the pain
is new, growing, and unserved.

## UC-13: Docs living outside the repository

User zero's own governance files are symlinks into a second repository (a dotfiles repo holding the
agent configuration), and its deploy notes describe resources in fly.io. The docs describing a
repository do not all live in it: wikis, Notion, runbook platforms, a platform team's central docs
repo describing fifty services.

The general form is a claim whose subject and evidence live in different trees with no shared
commit. There is no atomic change spanning them, so freshness must be defined against pinned
digests of the foreign side.

The tool supports cross-repository selectors pinned by content digest, checked on a schedule rather
than per-PR, with the honest state vocabulary (the foreign side moved; nobody has re-attested)
rather than a fake green. This is deliberately late: the single-repo product must earn trust
first.

## UC-14: Historical documents that must not be gated

User zero's `docs/content/docs/research/**` subtree mixes decision records, abandoned experiments,
and pages that explicitly defer to live references. The audit concluded a blanket freshness rule
over it would be pure noise. Two memory notes in the maintainer's archive record ideas that were
evaluated and rejected; those documents are correct precisely because they describe a past state.

Every mature repository accumulates ADRs, incident reports, migration guides for finished
migrations, and release notes. A tool that flags these as stale teaches users to ignore it; the
research on suspect-link fatigue says exactly this failure kills adoption.

The tool's answer is lifecycle classification as a first-class property: historical documents pin a
revision and are checked only for internal integrity (their links resolved at that pinned
revision), never for freshness against the current tree. The classification itself is cheap to
declare and its absence is a lintable gap on high-risk surfaces.

## UC-15: Diagrams as claims

User zero renders railroad diagrams from a stale embedded grammar (UC-04) and draws its module
structure as text trees inside the architecture page. Diagrams state inventories and edges in a
form neither greppable nor diffable, which is why they rot faster than the prose around them.

The general form: architecture diagrams, sequence diagrams, and dependency graphs whose boxes and
arrows assert the same facts a build graph or module list encodes. Text-native formats (Mermaid,
D2, Structurizr DSL) make the assertion machine-readable; image exports hide it.

The tool parses text-native diagram sources and treats node and edge sets as graph claims against
the extracted dependency graph, and it can require that diagrams on assured pages be text-native.
Bitmap diagrams degrade to narrative claims over the systems they depict, which at least makes
their review obligation visible. Precedent exists only at the dependency-topology level: ArchUnit
can check code against a committed PlantUML component diagram, and dependency-cruiser derives the
picture from the real graph instead; neither reaches the prose around the diagram.

## UC-16: Claims about the outside world

User zero's docs and configs name LLM model identifiers, solver release numbers, an upstream
formatting tool's behavior, and dozens of external URLs; one unit test even asserts a vendor's
per-million-token price. Its link checker (added late, after a PR found rot) validates only
internal references; 54 absolute same-repo GitHub URLs escape it, and external URLs are not checked
at all.

Every repository makes claims no diff of its own can invalidate: a vendor renamed a model, a
service changed a quota, a linked blog post moved. Link rot is the trivial, solved subcase;
semantic external drift (the URL resolves but the claim about its content is stale) is the general
one.

The tool folds link checking into link claims (including the same-repo-URL normalization user
zero's checker missed) and gives everything else external claims with a time-to-live: a scheduled
job probes what is probeable and expires attestations on what is not, so external claims surface as
review obligations at a controlled cadence instead of never.
