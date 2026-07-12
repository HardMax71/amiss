# User zero: repository audit of spec_to_rest

Date: 2026-07-10

Role in the dossier: this file is the evidence base, not the target. The tool under design is
standalone; spec_to_rest is user zero, the first corpus it must serve well. Every finding below is
generalized in [use-cases.md](./use-cases.md) (UC identifiers) and
[edge-cases.md](./edge-cases.md) (EC identifiers), which is where the product requirements live.
The historical pilot sketch at the end is superseded by
[implementation-readiness.md](./implementation-readiness.md) and
[preimpl-experiments.md](./preimpl-experiments.md). It remains here only as dated user-zero
evidence and does not authorize product scope, commands, storage, or a required CI gate.

## Executive finding

This repository is a strong calibration case for a documentation-drift gate. It already has several
good coupling mechanisms, but they protect generated artifacts and executable examples more often
than they protect prose claims. The gaps are concrete: the current tree contains stale CLI behavior,
grammar diagrams, proof-session topology, code paths, target file inventories, and an OpenAPI copy
that describes itself as identical to generated output but is not.

The scale is non-trivial even before counting source comments: 109 tracked Markdown/MDX files, 89
Fumadocs content pages totaling 12,568 lines, 21 canonical `.spec` fixtures, 350 golden files, 77
code-generation templates, and 22 GitHub workflows. Reproduce those counts with:

```bash
git ls-files | awk 'BEGIN{md=0;content=0;spec=0;gold=0;wf=0;tmpl=0} /\.(md|mdx)$/{md++} /^docs\/content\/docs\/.*\.(md|mdx)$/{content++} /^fixtures\/spec\/.*\.spec$/{spec++} /^fixtures\/golden\//{gold++} /^\.github\/workflows\/.*\.ya?ml$/{wf++} /^modules\/codegen\/src\/main\/resources\/templates\//{tmpl++} END{printf "markdown_mdx=%d docs_content=%d fixture_specs=%d golden_files=%d workflows=%d codegen_templates=%d\n",md,content,spec,gold,wf,tmpl}'
git ls-files 'docs/content/docs/*.mdx' 'docs/content/docs/**/*.mdx' 'docs/content/docs/**/*.md' | xargs wc -l | tail -1
```

The useful design lesson is that "doc-to-code edge" is not one relation. This repo needs at least
four validator types:

- Derived/transcluded content, where equality or successful regeneration can prove freshness.
- Executable examples, where the command and expected result can be rerun.
- Structural inventories, where names, paths, modules, sessions, flags, and enum members can be
  extracted and compared.
- Interpretive prose, where a source change can only create a review obligation; a hash can prove
  that review happened, not that the prose is true.

## Inventory and document classes

| Class | Representative paths | Drift character |
| --- | --- | --- |
| User reference | `docs/content/docs/cli.mdx`, `spec-language.mdx`, `install.mdx` | Claims flags, defaults, exit codes, syntax, supported platforms, and behavior. High user impact. |
| Pipeline/design reference | `docs/content/docs/design/**`, `pipelines/**`, `synth/**` | Claims module ownership, algorithms, lifecycle, guarantees, and limitations. Often one page to many source files. |
| Target reference | `docs/content/docs/targets/**` | Nine pages depend on profiles, emitters, templates, goldens, generated dependency versions, and CI matrices. Dense many-to-many surface. |
| Research/decision record | `docs/content/docs/research/**` | Mixes historical rationale, decisions of record, planned work, and claims about the shipped system. A blanket freshness rule would be noisy. |
| Proof documentation | `proofs/isabelle/*.md`, `docs/content/docs/design/isabelle-proofs/**` | Depends on `ROOT`, theory imports, named theorems, extraction exports, and CI commands. |
| Distribution/operations | `README.md`, `CONTRIBUTING.md`, `deploy/README.md`, `action.yml` | Depends on workflows, release assets, Dockerfiles, runtime limits, and repository policy. |
| Generated project documentation | `modules/codegen/**/README.md.hbs`, `fixtures/golden/codegen/**/README.md` | Template-to-output relation already covered well by byte-exact emitter goldens. |
| Generated/static docs assets | `docs/public/grammar/*.svg`, `docs/public/openapi/url_shortener.yaml`, `docs/lib/cli-runs/*.json` | Can be checked mechanically, but only if the generator itself consumes the real source of truth. |

The research subtree should not be treated as uniformly live. Some pages explicitly defer to a live
reference, for example `research/code_generation_pipeline/generated-project.md:6-9` and
`research/convention_engine/ruleset.md:18-30`; others are decisions of record. The initial pilot
should exclude research prose unless a page explicitly opts into live-source edges.

## Existing coupling mechanisms

### Strong mechanisms worth reusing

1. **Fixture transclusion.** Four worked examples contain empty fences such as
   `docs/content/docs/research/spec_language_design/worked-examples.md:6-15`. The
   `remark-spec-file` plugin reads the named repository file into the AST and fails if it is absent
   (`docs/lib/remark-spec-file.ts:9-20`). This removes copied source entirely.

2. **Playground generation.** `docs/scripts/build-playground-examples.mjs:5-8,30-52` reads eligible
   `fixtures/spec/*.spec`; `:54-63` writes the generated TypeScript used by the playground. The output
   is intentionally ignored (`docs/.gitignore:6`) and rebuilt through the docs scripts
   (`docs/package.json:6-9`), so committed-copy drift is avoided.

3. **Executable CLI snippets.** `<CliRun>` markers in
   `docs/content/docs/pipelines/verification/diagnostics.mdx:26,51,60,65` bind a page to a fixture,
   command, flags, and expected exit. The runner resolves fixture files and hashes the invocation
   (`docs/scripts/run-cli-snippets.mjs:74-121`), executes the real CLI (`:124-177`), and compares
   normalized output with committed goldens (`:285-341`). The site workflow compiles the CLI and
   runs the freshness check (`.github/workflows/deploy-fly.yml:54-73`).

4. **Fixture-to-IR goldens.** Every canonical `.spec` is parsed, built, serialized, and compared to
   a same-named JSON golden (`modules/parser/src/test/scala/specrest/parser/ParseBuildGoldenTest.scala:14-43`).
   A regeneration tool exists at
   `modules/parser/src/test/scala/specrest/parser/tooling/RegenIrGoldens.scala:17-43`.

5. **Emitter-to-project goldens.** Each language/dialect emitter is byte-compared with a full
   checked-in URL-shortener tree. Python demonstrates the pattern at
   `modules/codegen/src/test/scala/specrest/codegen/EmitPythonTest.scala:16-53`; Go and TypeScript use
   the same pattern. This protects 309 of the 350 golden files across the nine target trees, but it
   does not compare those trees with the hand-written target pages.

6. **Verifier and Dafny goldens.** SMT-LIB, Alloy, JSON verification reports, and Dafny skeletons
   are checked by `SmtLibGoldenTest`, `AlloyGoldenTest`, `JsonReportTest`, and
   `GeneratorGoldenTest`. The relevant source-to-artifact behavior is therefore well covered even
   where its prose description is not.

7. **Proof extraction drift gate.** Isabelle source is built, exported to Scala, formatted, and
   diffed against the committed generated file
   (`.github/workflows/isabelle-build.yml:112-120,161-190`). This is a real source-to-generated-code
   edge, not a timestamp heuristic.

8. **Architecture tests.** `build.sbt:103-105` declares the `dependsOn` graph to be mirrored by an
   ArchUnit test; `modules/arch/src/test/scala/specrest/arch/ArchitectureTest.scala:21-57` checks the
   package layering and cycles. It protects architecture, but not the module list described in the
   architecture page.

### Gaps in the current mechanisms

- The internal-link checker scans README plus all 89 content pages and currently passes, but it
  intentionally ignores every URL with a scheme (`docs/scripts/check-links.mjs:21-31,89-93`). The
  docs contain 54 absolute `github.com/HardMax71/spec_to_rest/blob|tree/main` links, so same-repo
  paths written as GitHub URLs escape the check.
- Inline repository paths are auto-linked by `remark-repo-links`, but a missing path is only a
  warning unless `DOCS_STRICT_LINKS=1` (`docs/lib/remark-repo-links.ts:65-100`). The site validation
  workflow does not set that variable. Already-linked paths, `proofs/**`, and `.github/**` also fall
  outside that plugin's conversion surface (`:11-12,67-70`).
- Site validation is path-filtered to `docs/**`, `fixtures/spec/**`, `modules/cli/**`, and deploy
  files (`.github/workflows/deploy-fly.yml:3-20`). Changes under `modules/parser`, `verify`,
  `convention`, `profile`, `codegen`, `testgen`, `synth`, or `proofs` can change CLI-visible output
  without running the CLI-doc freshness job.
- `npm run build` regenerates the railroad SVGs, but the generator hard-codes a second grammar
  instead of reading `Spec.g4` (`docs/scripts/build-railroad.mjs:20-76`). Generation therefore
  faithfully reproduces stale input.
- The general CI and quality workflows deliberately skip docs-only changes
  (`.github/workflows/ci.yml:3-23`, `.github/workflows/changes.yml:23-28`). That is reasonable for
  runtime tests, but means a future drift gate needs its own change graph rather than inheriting the
  current coarse `docs/**` split.

Running the existing checker confirms the gap:

```bash
node docs/scripts/check-links.mjs
# Link check passed (90 files scanned).
```

It passes while at least these referenced files do not exist:

- `modules/codegen/src/main/scala/specrest/codegen/RouteKind.scala`, referenced at
  `docs/content/docs/targets/python/fastapi/postgres.mdx:185-200`.
- `modules/convention/src/main/scala/specrest/convention/dafny/Generator.scala`, referenced at
  `docs/content/docs/design/convention-engine.mdx:94-100`; the implementation is now
  `modules/dafny/src/main/scala/specrest/dafny/Generator.scala:75-77`.
- `.github/workflows/docs.yml`, listed at
  `docs/content/docs/design/architecture.mdx:161-174`.

## Representative documentation-to-code edges

The following 15 edges cover the relation shapes this repository actually has. "Guard" describes
today's mechanism, not the proposed one.

| # | Documentation node | Authoritative/code nodes | Shape and current guard | Audit finding / drift risk |
| ---: | --- | --- | --- | --- |
| 1 | `docs/content/docs/cli.mdx` | `modules/cli/.../Main.scala`, `ExitCodes.scala`, `TestCmd.scala` | One page to command parser, status ADT, and dispatch behavior. No inventory check; only selected executable examples elsewhere. | **Confirmed drift.** The subcommand table at `cli.mdx:11-21` omits public `synth accept` (`Main.scala:458-486`). More importantly, `cli.mdx:147-152,188-200` says missing runners and runner statuses use a 0/1/2 contract, while `TestCmd.scala:21-39,110-122` maps missing runners to exit 1, runner test failure to 1, and unreachable/invalid-profile status 2 to process exit 3. |
| 2 | Four `<CliRun>` blocks in verification diagnostics | Four docs nodes, four fixture specs, the CLI and four `docs/lib/cli-runs/*.json` files | Many-to-many executable edge. Guarded by the snippet checker. | Good pilot exemplar. The validator already emits actionable per-snippet drift, but workflow paths omit most transitive CLI dependencies. Extend the trigger graph rather than replace the runner. |
| 3 | `spec-language.mdx` and `pipelines/parser-implementation.mdx` | `modules/parser/src/main/antlr4/Spec.g4`, parser builder, `preamble.spec` | Prose plus copied grammar snippets to grammar/resource files. Parser tests protect acceptance and AST behavior. | **Confirmed copied-snippet drift.** `parser-implementation.mdx:23-34` omits `REQUIRES_AUTH`, `SECURITY`, and `TEMPORAL` from `lowerIdent`, present at `Spec.g4:372-381`; its lexer-member example at `:102-118` is TypeScript-shaped while the live target is Java at `Spec.g4:3-13,500-502`. Preamble claims are better protected by `PreambleTest.scala:8-15`. |
| 4 | Three railroad diagrams in `parser-implementation.mdx` | `Spec.g4`, `docs/scripts/build-railroad.mjs`, generated SVGs | Grammar to hand-authored generator to generated asset. Current guard proves only script-to-SVG equality during docs build. | **Confirmed severe drift.** The operation diagram encodes `operation Name(params) -> ReturnType { requires Expr; ensures Expr; modifies ... }` (`build-railroad.mjs:37-59`), while the grammar uses an operation block with `input:`, `output:`, `requires:`, `ensures:`, and `requires_auth:` clauses (`Spec.g4:123-163`). The service diagram lists only five members (`build-railroad.mjs:60-75`) while `Spec.g4:30-44` has thirteen. |
| 5 | Convention property tables in `spec-language.mdx` and `design/convention-engine.mdx` | `Spec.g4:203-211`, `Validate.scala`, extracted `parseConventionValue` | Two pages to grammar, property registry, and value validator. No machine comparison. | **Confirmed semantic drift.** `spec-language.mdx:287-308` describes `test_strategy` as a `module:symbol` Hypothesis strategy and `convention-engine.mdx:229-237` gives that form. Live validation accepts only `"live"` or `"redacted"` (`Validate.scala:47-62,277-300`); `module:symbol` belongs to the separate alias/enum `strategy` property. |
| 6 | `design/architecture.mdx` | `build.sbt`, module directories, `Main.scala`, `.github/workflows/*` | One page to several inventories. ArchUnit guards code layering, not prose. | **Confirmed drift.** The project tree at `architecture.mdx:127-145` omits `dafny` and `arch` and lists an incomplete CLI surface; `build.sbt:106-243,297-326` has both modules. The page says ten workflows and names nonexistent `docs.yml` at `:161-174`; 22 workflows are tracked. This is ideal for structural extraction. |
| 7 | Nine target pages under `docs/content/docs/targets/**` | profile registry/targets, three emitters, 77 templates, nine golden trees, three target build workflows | Dense many-to-many edge. Emitters are golden-tested, but pages are independent prose. | **Confirmed drift.** The Python/Postgres file tree at `postgres.mdx:56-115` omits generated `.spec-snapshot.json`, `app/pagination.py`, `app/security.py`, and `app/routers/admin.py`, all present in its golden tree. It also references the removed `RouteKind.scala` at `:185-200`. Emitter churn is high and continued through 2026-07-10. |
| 8 | `docs/public/openapi/url_shortener.yaml` and its embedded target-page viewer | `url_shortener.spec`, Python emitter/OpenAPI implementation, Python/Postgres golden | One published copy to a multi-stage generated artifact. No equality gate. | **Confirmed byte drift.** `python/fastapi/postgres.mdx:224-231` says the snapshot is identical to compile output. `sha256sum` gives docs copy `46e251...a40` and current Python golden `c946b2...bef`; the docs copy contains five stale `maxLength: 10` lines absent from the current golden. |
| 9 | Four worked-example sections | `fixtures/spec/url_shortener.spec`, `todo_list.spec`, `auth_service.spec`, `ecommerce.spec` | Many docs sections to four files through build-time transclusion. | Strong by-construction edge. Preserve this model: no hashes or review stamps are needed because copied content does not exist in source Markdown. The surrounding inferred-method table remains a separate semantic edge. |
| 10 | Playground page and examples dropdown | `fixtures/spec/**`, example-builder script, `/api/compile` route, UI target lists | One page/component to many fixtures and runtime limits. Examples are generated; claims and duplicated UI labels are not. | Mixed. Fixture bodies cannot drift, but target/model display lists and prose limits can. The route keeps target-specific timeouts at `docs/app/api/compile/route.ts:67-73`; the UI duplicates frameworks, DBs, languages, and models at `docs/components/playground.tsx:27-54` with a runtime fallback. |
| 11 | `pipelines/migrations.mdx` | `SchemaDiff.scala`, `MigrationPlan.scala`, three emitters/renderers, schema codec, migration tests | One long reference to a broad subsystem. Unit and integration tests guard behavior, not the table. | High-value semantic edge. The operation matrix at `migrations.mdx:29-47` tracks the extracted migration ADT and renderers. A nuance already absent from the prose is that auto-increment type transitions are rejected before diffing (`SchemaDiff.scala:91-115`, tests at `SchemaDiffTest.scala:78-104`) despite generic type changes being listed as supported. |
| 12 | Verification and concurrency pages | `Config.scala`, `Consistency.scala`, Z3/Alloy backends, benchmark/golden | Several pages to defaults, execution branches, resource lifecycle, and measured output. Tests and a benchmark golden exist. | Mostly aligned today, but fragile: timeout, Alloy scope, parallelism, formula for planned checks, backend allocation, and cancellation are hand-duplicated. Sources include `Config.scala:12-25`, `Consistency.scala:113-129,198-210`, Z3 backend `Backend.scala:73-98`, and Alloy backend `Backend.scala:59-147`. Performance numbers should be regenerated, not hash-attested. |
| 13 | Isabelle session docs plus `proofs/isabelle/README.md` | `proofs/isabelle/SpecRest/ROOT`, 47 theories, extraction workflow | Several docs to a declarative session graph and generated Scala edge. Generated Scala is guarded; docs graph is not. | **Confirmed drift.** `session-layout.mdx:25-45,85-88` says Soundness and Codegen are independent leaf sessions and Codegen has 23 theories. `ROOT:24-40` makes Soundness depend on Codegen and adds `CandidateLowering_Sound`; `ROOT:40-68` lists 24 Codegen theories. The README repeats the sibling claim at `README.md:47-57`. |
| 14 | Structural lint table in `spec-language.mdx` | `Lint.scala`, seven pass objects, diagnostics, fixture/unit tests | One table to a stable code registry. Manually synchronized but recently co-changed correctly. | Good low-noise pilot edge. The table at `spec-language.mdx:181-214` currently matches the seven passes registered at `Lint.scala:5-17`; L07 behavior is explicit in `DroppedOutputs.scala:7-39`. An extractor can compare codes, levels, and short labels without interpreting the full prose. |
| 15 | Install and distribution docs | `action.yml`, `Dockerfile`, `native.yml`, `docker.yml`, deploy route and Dockerfile | One page plus README to multiple release surfaces and matrices. No direct validation. | Medium/high impact. Platform/archive mapping is duplicated between `install.mdx:11-27`, `action.yml:37-60`, and `native.yml:36-53`; container claims map to `Dockerfile:1-23`; playground capabilities and limits map to the route. Prefer extracted inventories and smoke commands over source hashes for these. |

## Concrete drift calibration cases

Any proposed checker should catch these known cases before it is considered useful:

1. `docs/public/openapi/url_shortener.yaml` differs from the canonical generated golden.
2. `build-railroad.mjs` describes an obsolete operation grammar and incomplete service grammar.
3. Proof docs say Soundness and Codegen are siblings, while `ROOT` makes Soundness depend on Codegen.
4. CLI docs describe exit behavior that disagrees with `TestCmd` and omit `synth accept`.
5. Convention docs assign `module:symbol` semantics to `test_strategy`, but validation accepts only
   `live`/`redacted`.
6. Architecture docs report ten workflows and `docs.yml`; the repository has 22 and no `docs.yml`.
7. The Python target page links a deleted source path and omits emitted files already locked by the
   golden tree.

These are useful because they exercise different validator classes; a single file-mtime or
last-commit comparison cannot distinguish them.

## Churn and co-change evidence

For commits from 2026-01-01 through 2026-07-10, treating `docs/**`, the root policy/reference
Markdown files, proof Markdown, and `deploy/README.md` as documentation, the repository has:

| Measure | Count |
| --- | ---: |
| Commits | 393 |
| Commits touching documentation | 195 |
| Commits touching non-documentation | 354 |
| Commits touching both | 156 |
| Documentation-only commits | 39 |
| Non-documentation-only commits | 198 |

Reproduce with:

```bash
git log --since='2026-01-01' --name-only --pretty=format:'@@%H' | awk 'BEGIN{n=0;d=0;c=0;b=0} /^@@/{if(n){if(hd)d++; if(hc)c++; if(hd&&hc)b++} n++; hd=0; hc=0; next} NF{if($0 ~ /^(docs\/|README\.md$|CONTRIBUTING\.md$|proofs\/isabelle\/.*\.md$|deploy\/README\.md$)/) hd=1; else hc=1} END{if(n){if(hd)d++; if(hc)c++; if(hd&&hc)b++} printf "commits=%d doc_touch=%d non_doc_touch=%d both=%d doc_only=%d code_only=%d\n",n,d,c,b,d-b,c-b}'
```

Thus 198 of 354 code-touching commits, about 56%, changed no docs. Commit co-change is useful for
suggesting edges, but too weak for gating. Selected last-change pairs show why:

| Pair | Documentation last change | Source/artifact last change |
| --- | --- | --- |
| Railroad generator / grammar | `5e905356`, 2026-04-26 | `Spec.g4` `e7d3e86b`, 2026-07-03 |
| CLI reference / `Main.scala` | `2cf90a27`, 2026-06-30 | `d16a3ad5`, 2026-07-07 |
| Target page / Python emitter | `412bfb74`, 2026-07-02 | `4cc165c3`, 2026-07-10 |
| OpenAPI docs copy / generated golden | `a8a68e50`, 2026-07-02 | `e7d3e86b`, 2026-07-03 |
| Proof session page / `ROOT` | `bbdfa225`, 2026-06-30 | `b91ed817`, 2026-07-05 |

The dates locate review candidates, but they do not prove drift. For example, a refactor can touch an
emitter without changing user behavior, while a one-line status mapping can invalidate a paragraph.

## Recommended pilot scope for this repository

Start with in-document claim markers and five cheap validators. Claims are owned from the
documentation side, while CI builds the reverse index so a source change finds every dependent doc.
Code should not need backlinks in production source.

### Tier 1: deterministic, low-noise gates

1. **Repository path existence.** Validate inline and explicit same-repository links, including
   absolute GitHub `blob/main` and `tree/main` URLs, `proofs/**`, `.github/**`, and paths inside MDX
   attributes. Enable strict behavior in CI. This immediately catches the three missing paths above.
2. **Generated equality.** Compare `docs/public/openapi/url_shortener.yaml` with the selected canonical
   emitter golden, or better remove the copy and serve/import the golden. Regenerate railroad assets
   and compare output, while separately declaring `Spec.g4 -> build-railroad.mjs` as a semantic edge;
   script-to-SVG equality alone is insufficient.
3. **Structural inventories.** Extract and compare:
   - CLI subcommands and option/default names from real `--help` output.
   - Module names and `dependsOn` edges from `build.sbt` against architecture-doc declarations.
   - Workflow filenames against the CI-surface table.
   - Isabelle sessions/theories/dependencies from `ROOT` against the proof-session page.
   - Lint codes from the pass registry against the structural-lint table.
4. **Existing executable snippets.** Keep `run-cli-snippets.mjs`, but trigger it from declared
   transitive sources rather than only `modules/cli/**`. `Main`, parser, verify, convention, profile,
   and IR changes can all affect these snippets.
5. **Golden-backed target file trees.** Parse fenced `tree` blocks that opt into a named target golden
   and compare path sets. This catches added/removed output files while allowing prose annotations.

These checks should run on the known calibration cases and report the doc node, source node, relation
type, and exact remediation. A generic "hash changed" message is not enough.

### Tier 2: explicit review attestations

For claims that cannot be extracted, allow a doc node to declare one or more source selectors and a
reviewed digest. Selectors should prefer stable symbols or declarative files over line ranges. When a
selected source changes, CI requires either a doc edit or an explicit review acknowledgement that
updates the digest. This is appropriate for:

- Convention-rule explanations and synthesis heuristics.
- Migration safety/limitation prose.
- Verification encoding and cancellation explanations.
- Target-specific operational explanations not derivable from goldens.

The acknowledgement means "reviewed against this source state," not "automatically proven true." It
should record the reviewer-facing reason and source diff, and it should be visible in code review.

### Exclusions for the first pilot

- Do not gate every research page; historical and decision-record prose has different freshness
  semantics. Require explicit opt-in.
- Do not hash whole modules or directories. The Python target page connected to all of
  `modules/codegen/**` would fail constantly on irrelevant refactors. Use selectors and semantic
  inventories.
- Do not use timestamps, newest-commit ordering, or co-change alone as the failure condition. Use
  them to discover likely edges.
- Do not make line numbers the durable identity of an edge. Lines move under formatting. Store a
  doc node ID plus a source path/symbol; line numbers belong only in diagnostics.
- Do not treat generated outputs as fresh merely because their generator ran. The railroad case
  proves that a generator can itself be a stale duplicate.

### Suggested success criteria

- The pilot detects all seven calibration drifts above.
- Static Tier-1 validation completes in under 30 seconds without compiling Scala; executable CLI
  checks can remain a separate heavier job.
- A source-only PR receives a precise list of affected doc nodes through the reverse edge index.
- False positives are tracked by edge and validator type; exemptions require a rationale and, for
  live reference docs, an expiry or owner.
- After an initial report-only period, only deterministic validators gate immediately. Review-digest
  edges graduate to gating after their observed noise rate is acceptable.

## Second-pass findings

A second inventory pass on 2026-07-10, focused on couplings rather than confirmed drift, added
these to the corpus:

1. The landing page is the highest-risk unguarded surface. `docs/content/docs/index.mdx` hard-codes
   a hero terminal with `21/21 consistency checks passed (212ms)`, literal verifier check names
   (`ListAll.preserves.allURLsValid` and siblings), and a hand-pasted router snippet labeled
   "(emitted)". Nothing regenerates any of it; spec, verifier naming, and emitter changes all break
   it silently (UC-10, EC-F1).
2. `docs/lib/targets.ts` derives the playground's framework, database, and language lists by
   shelling `spec-to-rest compile --help`, then falls back to a hardcoded list when parsing fails.
   The fallback rots invisibly: a help-format change flips the site to stale data with no error
   (EC-B1's shape in miniature).
3. Module counts disagree three ways: the architecture page's tree lists eleven modules, the
   ArchUnit test enumerates twelve layers, and thirteen module directories exist. The same page
   attributes the format and lint checks to `quality.yml`; they run in `ci.yml` (UC-01, EC-F1).
4. Version pins duplicate with a real failure edge: scalafmt `3.8.3` is pinned in `.scalafmt.conf`
   and again inside `isabelle-build.yml`, and the proof-extraction diff gate false-positives if the
   two diverge. `build.sbt` pins z3-turnkey `4.13.0.1` while CI apt-installs `z3` and `cvc5`
   unpinned (UC-11).
5. The committed synthesis cache is model-keyed (`claude-fable-5`: 11 entries,
   `gpt-5-mini-2025-08-07`: 20, prompt version `v2`) while the compile default model is
   `claude-sonnet-4-6`, so no committed entry matches the default; bumping the default, the prompt
   version, or the key derivation orphans the whole cache and leaves CI needing a live API key. No
   document records the cache's model keys. A unit test also asserts a vendor's per-million-token
   price (UC-11, UC-16, EC-C2, EC-D1).
6. The site serves `llms.txt` and `llms-full.txt` routes that aggregate the docs for machine
   readers, `AGENTS.md` delegates to `CLAUDE.md`, and `CLAUDE.md` references concrete paths, flags,
   and commands. Agent-facing documentation is a live, unchecked surface here today (UC-12).
7. More prose-maintained aggregates: "M1-M10" convention rules named only in docs (the classifier
   does not use those names), constructor counts ("27 constructors", "23-constructor subset")
   stated against the proof IR, and a hand-numbered 12-step precedence list mirroring the order of
   grammar alternatives (UC-05, EC-F1, EC-F4).
8. The transcript checker's normalization scrubs timings, paths, and core counts, so any
   documented claim inside a scrubbed field can never fail the check. Normalization trades noise
   for blindness and the trade should be declared per field (EC-C1's quiet cousin).

## Bottom line for the idea

This repo supports the idea, but argues against a universal timestamp/hash-pair gate. Hashes are
excellent for generated equality and for recording review of interpretive prose; they are not a
truth oracle. The practical model is a typed, directed graph from documentation nodes to one or many
authoritative source selectors, with CI maintaining the reverse lookup. Each edge chooses its
validator: transclude, regenerate-and-diff, execute, extract-and-compare, or require reviewed-digest
acknowledgement.

The best first targets here are the ones with both high impact and crisp semantics: same-repo paths,
the OpenAPI copy, CLI inventory/snippets, proof-session inventory, lint-code inventory, and target
file trees. They will catch real current defects while producing enough evidence to evaluate whether
the more subjective review-attestation tier is worth gating.
