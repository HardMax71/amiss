# Pre-implementation experiments: measured answers and remaining gates

Date: 2026-07-11.

This report addresses the empirical, parser, discovery, history, scale, storage, workflow, and
resource questions raised by [pre-implementation-review.md](./pre-implementation-review.md). It is
one-repository calibration, not external validation. Reproducible scripts and raw outputs are in
[experiments/](./experiments/README.md).

Contract reconciliation (2026-07-11): this file reports what the experiment could extract; it is
not the compatibility contract. [scanner-v0-spec.md](./scanner-v0-spec.md) deliberately narrows the
stable lane further: inline/plain-text inference is absent, and raw-HTML, heading-anchor,
site-route, and fence semantics remain unsupported without a conformance-tested adapter. The
per-claim state layout in [normative-core-spec.md](./normative-core-spec.md) is the selected X-06
test candidate because it eliminates the measured disjoint-write hot spot, but it is not
authorized as a stable repository format until X-06 passes. These decisions preserve the
measurements below without promoting the experiment's broad extractor into a product promise.

## Verdict

The read-only structural scanner is feasible. The stateful assurance product is still correctly
blocked.

The measurements sharpen the implementation boundary:

- Scan all 109 tracked UTF-8 Markdown/MDX files for structural references, but do not infer current
  code impact indiscriminately across generated and historical Markdown.
- Native local links, same-repository GitHub links, and the repository's four source-bearing fences
  are precise enough for a structural lane. The current tree has exactly two broken references in
  that class, both same-repository GitHub links.
- Repository-rooted inline paths are useful discovery and unsafe enforcement. Only 5 of the 16
  currently missing high-confidence occurrences were actionable current references; 11 were
  intentional generated, historical, or planned names.
- Repeated `[assure]` reference definitions are conclusively rejected. Parsers preserve both
  definition nodes, but every reference with that label resolves to the first destination.
- Block identity does not rescue trust-on-edit. Three of five known broken cases had their
  containing block edited while the reference remained broken.
- A committed automatic-observation writer would be busy: the surviving current implementation
  graph projects impacts onto 184 of 393 first-parent commits. Batching reduces commits, not the
  semantic problem.
- A single sorted JSONL ledger becomes a conflict hot spot under modest multi-claim updates. One
  file per ClaimId avoids disjoint-update conflicts by construction, but its filesystem and review
  costs are unmeasured. It is the design candidate, not a released compatibility commitment, until
  X-06 exercises those costs.
- The disposable Node scanner is fast enough for calibration on user zero, with local p95 wall time
  of 4.875 seconds, but it uses about 180 MiB at peak and emits a 1.95 MiB diagnostic JSON file.
  That is evidence for a spike, not yet for the intended cheap pre-commit product.
- No current workflow evaluates merge groups, and 21 of 26 checkout steps use the provider's
  shallow default. A new always-run workflow and event-level tests are required; an existing lane
  cannot simply be reused.

The correct next implementation remains a stateless, initially report-only experiment. It should
persist nothing and call no repository code. After base/candidate semantics and rollout gates pass,
`enforce` blocks every current native deterministic structural failure except an exact active
external debt/waiver match; attribution is diagnostic. Inferred impact remains report-only.

## What was measured

| Area | Method | Raw result |
| --- | --- | --- |
| Discovery and references | Git index inventory plus non-evaluating Markdown/MDX source parse | [current-scan.json](./experiments/current-scan.json) |
| Inline-path precision sample | Census of missing repository-rooted candidates plus a hash-selected ambiguous sample, manually labeled | [inline-path-sample.json](./experiments/inline-path-sample.json) |
| Parser/directive behavior | CommonMark, GFM, MDX, math, compilation-without-evaluation, and corpus parse matrix | [directive-matrix.json](./experiments/directive-matrix.json) |
| History | First-parent churn, surviving-current-graph projection, and five known-case transition replays | [history-replay.json](./experiments/history-replay.json) |
| Ledger size and writer churn | Actual synthetic serialization over discovered block hyperedges plus explicitly stated body-size assumptions | [ledger-simulation.json](./experiments/ledger-simulation.json) |
| Physical merge behavior | Seeded, semantically disjoint Git three-way merges | [merge-conflict-simulation.json](./experiments/merge-conflict-simulation.json) |
| CI readiness | Lexical workflow/event/checkout/pin/permission audit plus local Git availability | [workflow-audit.json](./experiments/workflow-audit.json) |
| Runtime and memory | Ten separate local Node processes on a warm filesystem | [runtime-benchmark.json](./experiments/runtime-benchmark.json) |

No network, external repository, hosted runner, or pull-request provider was used.

### Complete result-artifact inventory

| Artifact | Denominator and method | Limitation carried into the conclusion |
| --- | --- | --- |
| [experiments/README.md](./experiments/README.md) | Reproduction order and script/result manifest | Instructions assume the current Node 20 docs dependencies and full local history |
| [current-scan.json](./experiments/current-scan.json) | 1,155 tracked index entries; 109 conservative and 120 broad discovery candidates; source parse without evaluation | Current clean worktree only; no index/worktree divergence or hostile Git modes |
| [directive-matrix.json](./experiments/directive-matrix.json) | Six focused fixtures across four parse profiles and two non-evaluating compile formats; all 109 current documents across four profiles | No real GitHub renderer, full Fumadocs plugin execution, linter, fuzzing, or non-Markdown adapter |
| [inline-path-sample.json](./experiments/inline-path-sample.json) | Census of 16 missing repository-rooted occurrences plus 20 of 43 ambiguous misses selected by pre-label SHA-256 order | Manual one-repository labels; the ambiguous sample is deterministic, not a population estimate |
| [workflow-audit.json](./experiments/workflow-audit.json) | All 22 tracked workflows, 26 checkout uses, and every lexical event/pin/permission block | Static YAML text cannot reveal repository rulesets, enabled merge queues, or provider runtime payloads |
| [history-replay.json](./experiments/history-replay.json) | 393 first-parent commits; 61 surviving current implementation relations; five known current broken cases | Surviving-graph and whole-path bias; no feature-branch histories, owner labels, full historical graph, or semantic drift replay |
| [ledger-simulation.json](./experiments/ledger-simulation.json) | 179 grouped current block hyperedges, 240 endpoints, two synthetic encodings, and one explicit body-size scenario | Not a schema, not a large-repository measurement, and not acceptance behavior |
| [simulated-minimal.jsonl](./experiments/simulated-minimal.jsonl) | All 179 grouped hyperedges in the compact synthetic encoding | Observation specimen only; no compatibility or authenticity meaning |
| [simulated-detailed.jsonl](./experiments/simulated-detailed.jsonl) | All 179 grouped hyperedges with verbose synthetic endpoint snapshots | Deliberately over-complete sizing specimen, not proposed storage |
| [merge-conflict-workload.json](./experiments/merge-conflict-workload.json) | Seed, 250 ClaimIds, 100 trials per update count, and representative disjoint selections | Synthetic uniform updates do not reproduce an observed acceptance distribution |
| [merge-conflict-results.tsv](./experiments/merge-conflict-results.tsv) | Raw results from 300 `git merge-file --diff3` trials | Single-file textual merge only |
| [merge-conflict-sharded-results.tsv](./experiments/merge-conflict-sharded-results.tsv) | Three representative real repository merges for disjoint one-file-per-ClaimId updates | One representative per workload; the zero-conflict extension relies on zero overlapping paths and no rename/case collision |
| [merge-conflict-simulation.json](./experiments/merge-conflict-simulation.json) | Summary and Wilson intervals over the seeded Git workload | No same-claim CAS, filesystem-scale, cross-platform, or production-serializer measurement |
| [runtime-benchmark.tsv](./experiments/runtime-benchmark.tsv) | Raw results from ten scanner and ten existing-checker processes | Warm local filesystem on one machine |
| [runtime-benchmark.json](./experiments/runtime-benchmark.json) | p50/p95 wall, scanner self-time, RSS, and output-size summary | Not a hosted, cold-cache, adversarial, or large-repository benchmark |

The scripts that generate each artifact are listed beside them in the experiment README. Generated
JSON includes verbose raw records so the summaries above can be recomputed rather than trusted.

## 1. Discovery scope is two lanes, not one allowlist

### Exact inventory

The conservative structural scope contains:

| Measure | Value |
| --- | ---: |
| Tracked Markdown/MDX | 109 files |
| Bytes | 897,123 |
| Lines under the experiment's counting rule | 15,355 |
| Valid UTF-8 | 109 of 109 |
| Fumadocs content pages | 89 |
| Pages under `docs/content/docs/research/` | 50 |
| Generated golden-tree READMEs | 9 |

The broader proposed discovery rule finds 120 files. Its eleven additions are not eleven useful
documents:

- three `README.md.hbs` generator templates;
- four synthesis system-prompt programs;
- three SMT error golden outputs;
- `version.txt`, containing a scalar release value.

Two broad rules caused this: treating every `.txt` as prose, and treating every filename beginning
with `README` as a document even when it is a template input. This is an observed category error,
not a hypothetical edge case.

### Measured extractor coverage and the later stable-v0 correction

The experiment originally proposed `.md`/`.mdx` plus `generated` and `scope-unresolved` lifecycle
classifiers. The contract review rejected those ungrounded classifiers and expanded only the
deterministic filename surface. Stable v0 uses tracked `.md`, `.mdx`, `.markdown`, and the exact
extensionless basenames in scanner-v0-spec; suffix variants such as `README.md.hbs` do not inherit
document status. It publishes discovered, scanned, unsupported, excluded, unlinked, and separate
opaque-region/byte counters, but makes no generated/current/historical lifecycle claim.

The 50 research pages, nine generated READMEs, and proof logs remain evidence for that restraint,
not fields an implementation must invent. Literal supported links are still resolved wherever the
document set includes them. Inference and lifecycle classification remain outside stable v0.

## 2. Explicit structural references have a clean initial boundary

The source parser extracted 1,108 link-like constructs:

| Class | Occurrences | Current result |
| --- | ---: | --- |
| Same-repository GitHub `blob`/`tree` links | 55 | 53 resolved, 2 missing |
| Site-root local routes/assets | 172 | 172 resolved |
| Document-relative local links | 17 | 17 resolved |
| Repository-rooted `file=` fences | 4 | 4 resolved |
| Same-document anchors | 9 | Not evaluated as file references in this experiment |
| External targets | 851 | Deliberately not fetched |

The two missing explicit references are:

- `convention-engine.mdx:99` to the removed convention-module `Generator.scala`;
- `convention-engine.mdx:203` to the removed convention-module `Naming.scala`.

All four source-bearing fences resolve when their repository-rooted adapter semantics are applied.
Treating those attributes as document-relative produced four false failures in the first draft of
the experiment; this is direct evidence that fence meaning belongs to a site adapter, not a global
URL rule.

Seventy-one of 109 documents contain at least one explicit repository file/route reference; 38 do
not. That is a scope count, not a coverage score. Many good documents legitimately contain no such
reference, and one incidental link does not govern the surrounding prose.

### Decision

The disposable measurement extractor recognized:

- native Markdown links and definitions with local destinations;
- same-repository GitHub `blob` and `tree` URLs for a recognized immutable or current ref;
- literal `href`/`src` values in parsed HTML or MDX nodes;
- configured literal `file=`/`src=` fence metadata.

This measurement does not authorize HTML/JSX attributes or fence metadata as stable v0 structural
classes; those require separately selected adapters and fixtures. Stable v0 covers only the native
Markdown/MDX constructs and same-repository target forms in
[scanner-v0-spec.md](./scanner-v0-spec.md). Expression-valued MDX attributes, network URLs,
ambiguous branch/ref parsing, and inferred prose tokens never become “resolved” through guessing.
Existing breakage must be fixed or explicitly registered as exact externally reviewed debt; it does
not automatically become debt. A current-tree miss offers a rename candidate only as a suggestion.

## 3. Inline paths are valuable evidence against making inline paths blocking

After placeholder filtering and conservative extension/path matching, the experiment found:

| Inline candidate state | Occurrences |
| --- | ---: |
| Binding candidates | 494 |
| Resolved | 370 |
| Ambiguous | 65 |
| Missing | 59 |
| Placeholder/non-binding | 18 |

The 59 misses split into 16 repository-rooted occurrences and 43 ambiguous/basename occurrences.
Every repository-rooted miss was reviewed. A deterministic SHA-256 ordering selected 20 of the 43
ambiguous misses before labeling.

| Stratum | Reviewed | Actionable current broken references | Fraction |
| --- | ---: | ---: | ---: |
| Missing repository-rooted census | 16 | 5 | 31.25% |
| Hash-selected ambiguous sample | 20 | 0 | 0% |
| Combined reviewed set | 36 | 5 | 13.89% |

The non-actionable repository-rooted occurrences were former proof paths in historical speedup and
status records, explicitly retired workflow/tree paths, one planned retirement, and an expected
native-image build output. The ambiguous sample was dominated by generated target-project files and
historical proof basenames; it also included `TargetLanguage.Go`, a symbol selected because `.Go`
resembles a file extension.

The useful side of inference is real. It confirmed the two `RouteKind.scala` occurrences and found
three additional current broken occurrences in `.github/PULL_REQUEST_TEMPLATE.md`: two references
to the old `proofs/isabelle/SpecRest/IR.thy` location and one to the old monolithic
`Soundness.thy`.

### Decision

- Inline paths produce advisory findings with the exact reason they matched.
- Repository-rooted syntax is a confidence tier, not a hard-fail tier.
- Basename resolution is a candidate-discovery convenience. A unique basename today is not stable
  identity and is never persisted as authored intent.
- Placeholders, generated output names, reader-created files, historical text, and symbols require
  explicit classes; “missing in the Git tree” is insufficient.
- Promotion to a governed relation requires an explicit declaration and stable ClaimId, not an
  accept-flow choice hidden in a lock.

The 31.25% figure is not a product precision estimate. It is a one-repository counterexample strong
enough to reject blocking this class without lifecycle and origin semantics.

## 4. Parser and directive findings

### Duplicate definitions fail semantically

The repeated fixture placed two `[assure]: ...` definitions under different headings and referenced
the label in both sections. CommonMark, GFM, and both MDX profiles preserved two definition nodes in
source order. Rendering resolved both references to the first destination:

```text
modules/a/Foo.scala#Foo
modules/a/Foo.scala#Foo
```

The second definition is present in the AST and ineffective as a reference target. A map keyed by
normalized identifier would lose it; a renderer follows first-definition semantics. Placement under
a heading creates no native scope.

Unique labels such as `[assure:first]` and `[assure:second]` resolved to their respective versioned
destinations in every profile. This establishes only a necessary syntax property. It does not yet
approve an `assure:` URI grammar.

### Frontmatter requires an explicit adapter

Without a frontmatter extension, all four parser profiles interpreted the opening `---` as a
thematic break and the metadata lines as a heading/content sequence. A checker that assigns section
scope on that tree can attach declarations to a fictional heading. Frontmatter must be removed or
parsed before block/section ownership is computed.

### File extension and site plugins matter

Applying the basic MDX grammar indiscriminately to all 109 files failed on four valid repository
documents:

- two `.mdx` pages with TeX braces, fixed by the site's math extension;
- `.github/PULL_REQUEST_TEMPLATE.md`, whose HTML comment is invalid MDX syntax;
- `proofs/isabelle/SPEEDUP.md`, containing text that MDX treats as a malformed JSX name.

With extension dispatch—CommonMark/GFM for `.md`, MDX plus the configured math extension for
`.mdx`—all 109 current files parse. This is the minimum compatibility rule: parser selection is by
adapter and extension, never “MDX is a superset, parse everything as MDX.”

The JSX/ESM fixture confirms the inverse risk. CommonMark/GFM do not expose MDX ESM and JSX nodes;
the MDX parser does. Literal JSX attributes and expression attributes remain distinguishable. MDX
compilation preserved import/expression source without executing either sentinel assignment. The
scanner still should not compile in CI: source parsing is the smaller threat surface, and compile
success is not proof that repository plugins were not executed.

### Directive gate

No directive becomes public until all of these pass:

1. A unique stable ClaimId is mandatory and duplicate IDs are errors.
2. One declaration extracts to byte-identical semantics and stable spans in the exact supported
   CommonMark/GFM, GitHub rendering, Fumadocs/MDX, and lint configurations.
3. Frontmatter, tables, footnotes, CRLF, Unicode, math, literal JSX, expression JSX, ESM, HTML,
   fences, and malformed input have fixtures.
4. Declarations render no visible output and cannot be confused with ordinary reference use.
5. Source parsing imports, evaluates, and executes nothing from the repository.
6. Unsupported syntax produces an explicit analysis result, never a clean relation.
7. Fuzzing and size/nesting/time limits show that malformed input cannot crash, hang, truncate into
   a pass, or cross a source-evaluation boundary.

This experiment covered local CommonMark/GFM/MDX behavior. It did not execute the full Fumadocs
plugin pipeline, run GitHub's renderer, run markdownlint, fuzz, or test reStructuredText/AsciiDoc.
The directive gate therefore remains closed.

## 5. History directly rejects trust-on-edit

### Repository churn

The first-parent mainline from 2026-01-01 through 2026-07-10 contains 393 commits under the
experiment's `.md`/`.mdx` document classification:

| Measure | Commits |
| --- | ---: |
| Documentation touching | 188 |
| Non-documentation touching | 379 |
| Both | 174 |
| Documentation only | 14 |
| Non-documentation only | 205 |

These counts intentionally differ from the dossier's earlier wider `docs/**` classification. This
replay uses the proposed scanner's document paths, while the earlier audit counted non-Markdown docs
site implementation/assets as documentation changes. The classification must accompany every
co-change number.

### Surviving-current implementation graph replay

Projecting the 61 currently resolved explicit relations whose targets are not documents backward
over those changed-path lists gives:

| Measure | Value |
| --- | ---: |
| Unique surviving current implementation targets | 51 |
| Commits with at least one projected impact | 184 |
| Relation-impact events | 773 |
| Target changed without document-file co-change | 735 |
| Target and document file co-changed | 38 |
| Fan-out p50 / p95 / max | 4 / 8 / 15 |

This is a workload estimate, not 735 defects. It uses whole-path changes, does not reconstruct the
relationship graph at each historical commit, and excludes relationships deleted before HEAD.
Those limitations bias it in both directions. It nevertheless disproves the assumption that an
inferred code-impact lane will be quiet enough to block before human labeling.

### Five known broken cases

The replay followed the two native broken links and three unique actionable inline targets through
commits touching their document or old path. All five were valid references before their target
disappeared, and all five remained in the document at the disappearance commit. A stateless
base/candidate structural checker could have caught all five at the responsible change.

After breakage:

| Event | Count |
| --- | ---: |
| Document-file edits while the case remained broken | 22 |
| Containing-block edits while the case remained broken | 3 |

File-level trust-on-edit would have implicitly cleared obligations 22 times in this five-case set.
Block-level content identity reduces that exposure substantially and still fails three times: the
Naming paragraph and the two pull-request-template proof blocks changed while their targets
remained absent. The replay's blank-line block approximation is conservative enough for these
paragraph/list cases; a production parser must replay its exact block model.

### Decision

- A subject edit records `subject-changed` or an optional weak co-change fact. It never advances an
  attestation.
- A stateless structural transition evaluates base and candidate and catches target disappearance
  regardless of later document edits.
- A stateful obligation, if later justified, survives every subject edit until explicit acceptance
  or governed retirement.
- Historical replay must reconstruct declarations per revision before any claimed precision,
  recall, or actionability number is published.

## 6. Ledger cardinality, size, and writer pressure

The current explicit file-reference set groups into 179 document-block hyperedges with 240
deduplicated dependency endpoints and 117 unique targets. This is already evidence that pairs are
not the right presentation unit: 248 occurrences collapse into fewer block-level review units.

Two synthetic representations were serialized:

| Representation | Records | Raw bytes | gzip-9 bytes | Mean raw bytes/record |
| --- | ---: | ---: | ---: | ---: |
| Minimal automatic observation | 179 | 63,471 | 17,474 | 355 |
| Verbose per-endpoint automatic observation | 179 | 271,600 | 57,401 | 1,517 |

At the observed detailed mean, 100,000 records are about 152 MiB and one million about 1.52 GiB
before compression. Those are linear extrapolations, not measured large-repository ledgers.

A projection-body scenario used explicit assumptions of 500 bytes per subject body and 2,048 bytes
per dependency projection. It produced 853 KiB when bodies repeat per relation and 601 KiB when
target bodies are content-addressed and deduplicated. The 29.5% modeled saving depends entirely on
those assumptions. It answers neither retention nor privacy.

Writer pressure is more concerning than current size. Replaying the surviving implementation graph
gives this ceiling if automatic observations were committed whenever selected targets changed:

| Writer schedule | Possible commits in the 393-commit period |
| --- | ---: |
| Per merge | 184 |
| Daily batch | 61 |
| Weekly batch | 14 |

There were 773 endpoint-impact events. Rewriting one detailed line per event would create roughly
2.35 MiB of delete/add JSONL churn under the synthetic mean. An automatic writer is not required
for the stateless scanner, so the recommended v0 values are zero state records, zero bot commits,
and zero repository-state conflicts.

Batching can make a writer less annoying. It cannot turn an automatic observation into an
attestation or disambiguate which merge a batched obligation belongs to.

## 7. Physical layout: line-per-claim JSONL is not enough

The merge experiment used 250 claims. In each trial, branch A and branch B updated uniformly chosen,
strictly disjoint ClaimId sets. There were no semantic same-claim conflicts. Git performed a
three-way textual merge of one lexicographically sorted JSON record per line.

| Updates per branch | Trials | Single JSONL conflicts | Rate | Wilson 95% interval |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 100 | 0 | 0% | approximately 0–3.70% |
| 5 | 100 | 18 | 18% | 11.70–26.67% |
| 20 | 100 | 99 | 99% | 94.55–99.82% |

Separate canonical files per ClaimId had no overlapping paths by construction. One real repository
merge per workload validated clean tree merges for the generated representative selections.

The result does not prove one-file-per-claim is the answer. It proves that “sorted and line per
claim” does not make a global ledger conflict-safe: Git's textual hunks include neighboring
context, and multi-record updates collide despite disjoint logical records.

The sharded result is conditional on no same-claim update, rename, case-folding collision, or
directory/file conflict. It says nothing about 100,000 files, checkout/status cost, inode use,
Windows behavior, code-review usability, atomic split/merge transactions, or repository-host
limits.

### Decision

Do not publish a stable JSONL, TOML, or one-file-per-claim compatibility format from this
experiment alone. The normative design selects one file per ClaimId as the X-06 candidate because
it is the only tested shape that removes disjoint logical updates from one textual merge surface.
Before that selection can become an implementation/release contract, a governed-claim pilot must
compare at least:

- one global canonical file;
- deterministic hash-bucket or docs-subtree shards;
- one file per ClaimId;
- external observation storage while authored declarations remain in Git.

Run the actual serializer over observed ordinary one-claim acceptances and closed split/merge
transaction sizes. Same-claim concurrent acceptance must still fail compare-and-swap; the goal is
to eliminate spurious conflicts, not real ones. Bulk/multi-claim acceptance remains unsupported.

## 8. Runtime, memory, and output

Ten separate local processes scanned the 109-file corpus on a warm filesystem:

| Measure | Mean | p50 | p95 / max |
| --- | ---: | ---: | ---: |
| External wall time | 2.772 s | 2.535 s | 4.875 s |
| Post-import scanner self-time | 2.379 s | 2.175 s | 4.202 s |
| Max RSS | 169.7 MiB | 171.3 MiB | 175.6 MiB |
| Verbose JSON output | 1.949 MiB | 1.949 MiB | 1.949 MiB |

The existing specialized link checker averaged 145 ms over its 90-file scope. It is much narrower:
it does not parse same-repository GitHub URLs as internal, emit resolution attempts, enumerate
inline candidates, or preserve per-block evidence.

The experiment passes the dossier's loose “under 30 seconds” calibration target on this machine.
It also exposes three implementation requirements:

1. The default result cannot dump every external link, path attempt, and candidate. Human output
   must be compact; full experimental JSON belongs in an artifact with explicit byte/finding caps.
2. A Node parser stack using roughly 180 MiB is acceptable for a disposable site-compatible oracle,
   not automatically for a ubiquitous pre-commit hook.
3. Runtime claims require cold hosted-runner measurements, adversarial inputs, and larger
   repositories. A warm developer machine is not the product environment.

No cache is justified on user zero. Correctness, bounded parsing, and a compact output model come
before persistent caching.

## 9. Workflow, base, and merge readiness

The workflow audit found:

| Fact | Value |
| --- | ---: |
| Workflows | 22 |
| Workflows with `pull_request` | 14 |
| Workflows with `merge_group` | 0 |
| Workflows with path filters | 12 |
| Checkout steps | 26 |
| Checkout steps using provider-default depth | 21 |
| Full-history checkout steps | 5 |
| Explicit depth-two checkout steps | 0 |
| Checkout steps with `persist-credentials: false` | 26 |
| Checkout steps pinned to full commit SHA | 26 |
| External Action uses with mutable/short pins | 0 |

The existing links workflow has good read-only permissions, an immutable checkout pin, and disabled
credential persistence. It is path-filtered and uses the shallow default, so it cannot become the
code-impact lane without changing its trigger and base contract. Keeping it separate is cleaner.

The local checkout is full and has `HEAD^`; that proves only that local history replay was possible.
Repository files do not reveal whether a merge queue is enabled, and no workflow demonstrates the
provider's merge-group payload. Exact base/candidate behavior is therefore not ready.

### Required event test matrix

Before a required check, run controlled provider tests for:

- same-repository pull request;
- fork pull request with read-only token and no secrets;
- pull request after its base advances;
- merge-group candidate containing more than one queued change;
- default-branch push;
- shallow checkout with base fetched explicitly;
- unavailable base, which must report unattributed analysis or exit `2`, never a clean comparison.

For every event, record the provider event name, supplied base SHA, supplied candidate SHA, checked
tree SHA, fetched objects, permissions, and exit class. Pass only when the checked candidate equals
the tree eligible to merge, the base is explicit when attribution is claimed, no path filter can
skip the job, and analysis failure cannot produce exit `0`.

## 10. Issues resolved now versus gates still open

| Review issue | Empirical resolution |
| --- | --- |
| Universal text-file discovery | Rejected. Use Markdown/MDX structural scope plus exact plain-text opt-ins. |
| Structural reference classes | Initial native/local/GitHub/fence boundary measured and viable. |
| Inline path enforcement | Rejected. Advisory only; lifecycle and generated/historical origin are necessary. |
| Repeated `[assure]` declarations | Rejected by parser and rendering behavior. Unique stable IDs remain mandatory. |
| Frontmatter and MDX uniformity | Rejected. Adapter/plugin/extension dispatch is mandatory. |
| Trust-on-edit | Rejected by three observed block-edit-while-broken transitions. |
| Automatic refresh writer | Excluded from v0; measured write pressure adds no reason to restore it. |
| Ledger size | User-zero cardinality and synthetic size are now known; external scale remains open. |
| One global line-oriented ledger | Not safe by assumption; measured spurious conflict rate rises sharply with batch size. |
| Sharded physical state | Promising for disjoint updates, not selected until filesystem and atomicity tests. |
| Fast-check feasibility | Passes local loose latency target; memory/output need product-specific work. |
| Existing CI reuse | Rejected; a separate always-run base/candidate/merge-group lane is needed. |

### Mapping to the pre-implementation Gate A–D checklist

| Gate | Evidence now available | Status after this work | What still prevents passage |
| --- | --- | --- | --- |
| Gate A: discard-state scanner | Exact conservative document/reference boundary; current corpus parses under extension/plugin dispatch; scanner is repository-read-only; local resource baseline; strict bounded report/error shapes and exits are specified | **Partial—enough to implement the disposable experiment, not enough to require it in CI** | Candidate/local/index snapshot semantics need Git fixtures; strict JSON/schema/cross-field parsing, parser/resource failures, truncation, and provider base/candidate behavior remain unimplemented and untested |
| Gate B: persisted observation ledger | User-zero cardinality, two sizing specimens, writer-pressure scenarios, and logical-layout conflict data | **Blocked** | No large unrelated repository, observed need for cross-merge obligations, canonical encoding/digest vectors, migration implementation, product serializer, or acceptable physical layout |
| Gate C: governed claims | Duplicate-label syntax rejected; unique stable ClaimId necessity reinforced; trust-on-edit empirically rejected | **Blocked** | Directive RFC has not passed real renderers; lineage under move/duplicate/split/merge is unmeasured; definition/observation/acceptance/lifecycle types and base-transition meta-findings are not implemented or tested; ownership is absent |
| Gate D: required narrative gate | Current workload/fan-out estimate and five known structural histories establish what must be calibrated | **Blocked** | No external prospective shadow run, owner actionability labels, real merge queue, fork/provider identity, concurrent acceptance CAS, protected ownership, complete adversarial suite, or fail-closed production parser |

Two absences are external gates, not backlog details: no unaffiliated-repository shadow run has been
performed, and no real GitHub `merge_group`/fork/provider event has been exercised. Local history
and static workflow text cannot substitute for either.

Known-drift coverage is also partial. The replay covers five structural path failures and shows
when a base/candidate resolver would have caught them. It does not replay the seven semantic
calibration drifts—OpenAPI inequality, railroad grammar, proof topology, CLI behavior, convention
semantics, workflow inventory, and generated target-tree omissions—through their appropriate
deterministic validators. Those validators remain separate experiments, beginning with OpenAPI
equality.

Finally, no result here measures logical lineage ambiguity. The history pass did not reconstruct
block moves, exact duplicates, splits, merges, or declaration deletion at every revision. It
therefore supplies evidence against content-derived identity but cannot validate a replacement
lineage algorithm. Governed identity remains explicit and stable by design; inferred lineage may
only suggest migrations.

## 11. Precise future experiments and pass/fail gates

Unmeasured work is converted here into falsifiable gates. Thresholds for human actionability and
maintenance cost must be pre-registered by each pilot team; the tool must not invent a universal
percentage after seeing results.

### X-01: exact historical graph replay

Method: reconstruct supported documents, blocks, references, targets, and selector projections at
every relevant commit on main and selected feature/merge histories. Label alerts by reference class
and have domain owners classify actionability.

Pass:

- every finding is reproducible from recorded base/candidate object IDs;
- native-link seeded deletion/addition/move mutations are all detected;
- parser or missing-object states are explicit;
- the report separately exposes survivorship bias, unsupported history, and scope;
- no subject edit can discharge a simulated governed obligation without an explicit transition.

Fail or narrow: any reference class whose pre-registered actionability target is missed stays
advisory; any history mode that guesses missing objects is removed from correctness semantics.

### X-02: prospective external shadow run

Method: report-only runs on user zero and two or three unaffiliated repositories for enough real
code/doc/refactor/merge activity. Review every finding. Randomly audit clean and unlinked pages so
precision cannot hide escaped drift or empty coverage.

Pass:

- native structural findings are deterministic and locally reproducible;
- each team meets its pre-registered actionability, latency, and maintenance-cost thresholds for a
  class before promoting that class;
- discovered, scanned, unsupported, excluded, unlinked, external, and separate opaque-MDX/HTML
  denominators are reported;
- random audits do not contradict the product's stated coverage boundary.

Fail or pivot: keep inference as discovery, or ship only structural and deterministic validators.
No amount of user-zero data satisfies this external gate.

### X-03: parser, renderer, and directive RFC

Method: run the complete fixture matrix through exact supported versions of GitHub/CommonMark,
Fumadocs/MDX, linters, and any claimed additional format. Parse source in an isolated process; fuzz
the real production parser boundary.

Pass:

- one unique declaration yields identical semantics and source ownership everywhere claimed;
- rendered output is empty;
- frontmatter and MDX constructs cannot steal scope;
- no repository ESM, JSX expression, plugin, include, or config is executed;
- malformed, oversized, and deeply nested input terminates within stated limits and never passes
  after a crash, timeout, or truncation.

Fail or narrow: market format-specific adapters. Do not claim “any text file.”

### X-04: worktree, index, and Git-object modes

Method: temporary repositories covering clean checkout, staged/unstaged divergence, untracked and
ignored docs, deletion, rename, intent-to-add, conflict stages, symlink escape, submodule, sparse
checkout, LFS pointer, non-UTF-8 path, shallow base, and both supported object formats.

Pass:

- each mode reads only its declared snapshot;
- clean worktree and index results are identical;
- NUL-safe paths and modes round-trip;
- unsupported Git states are distinct and cannot produce success;
- repository status and bytes are unchanged after `check`.

### X-05: production resource envelope

Method: benchmark cold processes on hosted Linux, macOS, and Windows runners over small, medium, and
large corpora; add maximum-size, nesting, JSX, table, link-count, and adversarial parser fixtures.

Pass:

- the pilot team's pre-registered wall, memory, and output budgets hold at p95;
- every configured limit yields an explicit bounded result and correct exit class;
- default output is useful below the output cap, with totals preserved when details truncate;
- no persistent cache is required for correctness.

Fail or narrow: reduce formats/reference classes, shard scheduled audits, or isolate parser workers.

### X-06: ledger serializer and physical layout

Entry condition: an X-08 disposable harness has shown that at least one design partner chooses and
services durable carried obligations over the stateless alternative. X-06 is not an entry
condition for that harness. Until this condition passes, physical layout remains deliberately
unselected.

Method: serialize the proposed per-endpoint model for user zero and at least one genuinely large
repository. Replay ordinary one-claim acceptances,
closed split/merge transaction sizes, and concurrent branches across global, bucketed, subtree,
per-claim, and external layouts on
case-sensitive and case-insensitive filesystems.

Pass:

- byte-deterministic output and golden vectors across platforms;
- zero spurious conflicts under the accepted disjoint-update workload target;
- same-claim predecessor conflicts fail compare-and-swap;
- closed split/merge lifecycle transactions remain atomic; bulk acceptance stays unsupported;
- checkout, status, diff, review, and repository-size costs meet pre-registered budgets;
- schema migration never turns an impacted claim current.

Fail or pivot: keep the scanner stateless or place observations in an external service. Do not shard
merely to preserve a lockfile assumption.

### X-07: real CI event semantics

Method: install an experimental read-only job in a sandbox repository and exercise the event matrix
above, including an actual merge queue and fork.

Pass:

- exact candidate SHA is always evaluated;
- attribution uses an explicit available base or says unavailable;
- credentials, network, secrets, and writes are absent;
- all Actions/binaries are immutable and verified;
- a missing base, parser failure, timeout, or truncated scan cannot report clean.

### X-08: governed-claim pilot

Method: after X-01 through X-05, X-07, and the entry conditions in implementation-readiness, add a few
explicit stable-ID claims with local acceptance through ordinary review. Keep all harness state in
an isolated disposable directory or in memory; do not select a stable repository layout. Exercise
subject edit, dependency edit, retarget, move, deletion, retirement, split, merge, engine
migration, policy weakening, and concurrent acceptance.

Pass:

- observation, attestation, validation, lifecycle, trust, waiver, and policy remain independent;
- only explicit valid acceptance advances attestation;
- every lifecycle and policy reduction is visible in the candidate diff;
- final merge candidate invariants hold regardless of blame;
- owners meet their pre-registered review burden and actionability targets.
- the design partner explicitly chooses or rejects durable carried obligations relative to the
  stateless alternative; only a positive choice opens X-06.

Fail or pivot: retain structural/deterministic checks and drop governed narrative assurance.

## Recommended implementation order after these results

1. Freeze only the experimental scanner's supported input/reference classes and exit behavior.
2. Build the CLI/schema/Git-acquisition scaffold, hostile fixtures, complete parser-profile corpus,
   and conformance harness.
3. Only after the corpus goldens pass, build the read-only base/candidate structural evaluator with
   compact experimental output.
4. Complete Git-state fixtures and the provider event sandbox before enabling a required job.
5. Run historical replay and external shadow mode; keep inline/current-impact inference advisory.
6. Add the OpenAPI equality validator as a separate deterministic lane.
7. Revisit durable observations only if shadow data shows value that stateless comparison cannot
   provide.
8. Revisit governed claims and physical storage only after the directive, lifecycle, trust,
   concurrency, and serializer gates pass.

The experiments do not weaken the product thesis. They remove three attractive but unsafe shortcuts:
broad text discovery, trust-on-edit, and a supposedly conflict-free global lock. The smaller scanner
is now better specified, and every remaining leap has a test that can kill, narrow, or justify it
before it becomes compatibility debt.
