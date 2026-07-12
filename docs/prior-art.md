# Prior art for CI detection of documentation-code drift

Status: evidence review, not an implementation proposal. Sources were accessed on 2026-07-10. Product capabilities are described as vendor claims unless an independent evaluation is cited.

## Bottom line

There is no general-purpose hash, timestamp, graph, or model that can prove arbitrary natural-language documentation is semantically consistent with a repository. Existing approaches prove narrower propositions:

- Inclusion and generation prove that displayed machine-derived material came from the current source of truth.
- Content hashes prove that selected bytes have or have not changed since a recorded acceptance event.
- Doctests and documentation workflow tests prove that specified examples or procedures work in the tested environment.
- API/schema diffing proves that a machine-readable contract changed, or that a defined compatibility rule was violated.
- Explicit or recovered traceability links identify documentation that may be affected by a code change.
- Documentation coverage proves that a documentation artifact exists for an item, not that its content is correct.
- Changed-file and co-change heuristics identify risk and route review; they do not establish inconsistency.
- Semantic and LLM-based systems can find contradictions beyond syntactic changes, but the strongest recent results still trade substantial recall for precision or rely on small preliminary evaluations.

The central design consequence is that a useful CI gate should report the proposition it actually checked. "The linked source region changed since this paragraph was accepted" is defensible. "This document is stale" usually is not, unless the assertion is executable or generated from the authoritative artifact.

## The landscape in one map

Everything surveyed below sorts into four schools by what they do when documentation and code
disagree:

| School | Representative tools | What stays fresh | What is never checked |
| --- | --- | --- | --- |
| Re-anchor | Swimm; fiberplane/drift | coupled snippets, tokens, paths | the prose around the coupling |
| Regenerate | OpenAPI and SDK doc generators, terraform-docs, Speakeasy, Fern | derived reference material | hand-written narrative; the generator's own inputs |
| Execute | doctest family, Doc Detective, Runme, byexample | embedded examples and procedures | claims not expressible as a run |
| AI-rewrite | Mintlify agent, DeepDocs, DocuWriter, GitBook agent | whatever the model chose to update | whatever it did not; the gate is one reviewer |

The quadrant left open, and targeted by this design, is typed claims made by prose: checked
deterministically where a projection exists, flipped to a suspect state with recorded human
re-attestation where not. Three observations say the quadrant is real rather than empty for lack
of demand. fiberplane/drift ships the mechanism's skeleton (fingerprints, lockfile, re-attest,
reverse lookup) without the claim model. Dosu published a CI recipe hand-rolling freshness scores,
time-to-live contracts, and symbol-drift checks because no product does it. And a Doorstop user
prototyped hash-based code-reference review inside a requirements tool (issue 564); it was never
shipped.

## Assurance matrix

| Mechanism | What a green result establishes | What it does not establish | Typical operational failure |
| --- | --- | --- | --- |
| Generate reference docs from schemas or code | Output is reproducible from the current input, assuming the generator is correct and rerun | The input schema matches runtime behavior; narrative guidance is complete or useful | Generated output is not regenerated, or the schema itself drifts from implementation |
| Include a source file/region in docs | Rendered snippet is the selected current source text | Surrounding prose explains it correctly; the selected region is still the intended one | Line selectors slide, markers disappear, or a renamed/moved symbol becomes unresolved |
| Region/file content hash | Selected normalized bytes equal the last accepted bytes | Semantics are unchanged; prose is true; unlinked code is covered | Cosmetic changes cause noise, or aggressive normalization hides meaningful changes |
| Whole-repository commit ID | The link identifies an immutable repository snapshot | The target region changed after that snapshot | Every unrelated commit makes a naive "HEAD differs" rule red |
| Doctest/compiler-verified example | Example compiles/runs and asserted outputs hold in the CI environment | Unasserted behavior and surrounding prose are correct | Hidden setup, ignored examples, environment skew, nondeterminism |
| End-to-end documentation test | Tested user journey/API call still works for tested contexts | Untested paths, conceptual claims, alternate platforms, or usability are correct | Expensive/flaky setup, credentials, destructive steps, skipped contexts |
| API/ABI/schema diff | A formal contract changed or violated configured compatibility rules | Human-facing docs mention or explain the change; runtime conforms to the schema | Dynamic behavior is outside the model or custom semantics are not represented |
| Explicit documentation-to-code edge | The author declared a dependency and the selected target can be checked | The relation is semantically correct; all relevant dependencies were declared | Authoring burden and link/selector decay |
| Automatically recovered edge | A model or heuristic predicts a relation | The relation is certain; absence means no relation | Ambiguous names, false links, cold start, language limitations |
| Documentation coverage | A required item has documentation above a configured threshold | Accuracy, freshness, clarity, examples, or conceptual coverage | Empty/boilerplate text satisfies the gate |
| Changed-file/path rule | A file in a configured risk area changed and required process ran | The mapped doc actually needs a change, or a token doc edit is sufficient | High false-positive rate or easy "touch the docs" gaming |
| Semantic/LLM inconsistency detector | The detector found evidence of a contradiction according to its model/tests | No report means consistency; the report is necessarily correct | False positives, missing discriminating tests, prompt/model drift, cost |

## 1. Executable documentation and doctests

### Embedded examples

Python's standard [`doctest`](https://docs.python.org/3/library/doctest.html) searches prose for interactive sessions, executes them, and compares actual output with the documented output. The Python documentation explicitly describes this as checking that interactive examples remain up to date and as "executable documentation." Rust's [`rustdoc --test`](https://doc.rust-lang.org/rustdoc/write-documentation/documentation-tests.html) extracts fenced Rust examples, compiles them, and normally runs them; `cargo test` includes documentation tests. Rust also supports modes such as `no_run`, `compile_fail`, and `ignore`, which are useful but weaken or change the proposition being checked. (Accessed 2026-07-10.)

Swift combines source inclusion with compilation more directly. Swift Package Manager builds files in a package's `Snippets` directory, and DocC can reference all or part of those files as displayed examples; the official [DocC snippet documentation](https://www.swift.org/documentation/docc/adding-code-snippets-to-your-content) presents this as a way to keep examples compiling as the package evolves. (Accessed 2026-07-10.)

What these prove is stronger than a timestamp: a particular example can be consumed by the current toolchain and any encoded assertions or expected output hold. They can detect changed names, signatures, types, output, and many behavior changes.

Their blind spots are equally important:

- A successful example only covers its executed path and explicit assertions.
- Prose immediately before or after the example can still be false.
- Examples often hide imports, setup, error handling, or cleanup for readability.
- `ignore`, `no_run`, network mocks, feature flags, and platform skips reduce assurance.
- A test can pass while teaching an obsolete or unsafe practice.
- Testing every version/platform combination can be prohibitively expensive.

### Executable procedures and product walkthroughs

[Doc Detective](https://docs.doc-detective.com/docs/get-started/introduction) is an OSS documentation-content testing framework. It parses specifications or testable actions from docs, then checks browser/UI actions, links, text, screenshots, API responses, or arbitrary scripts. Its [detected-tests mechanism](https://docs.doc-detective.com/docs/test-docs/detected) can derive actions from Markdown or DITA patterns, keeping the test definition closer to the procedure. Results include pass, fail, warning, and skipped states and can be emitted as JSON for CI. (Accessed 2026-07-10.)

This can validate user-observable behavior that no code-region hash can capture: whether a documented button exists, a workflow reaches the expected page, or a CLI/API sequence produces the expected result. It still proves only the chosen journey in the chosen environment. Authentication, rate limits, external dependencies, destructive operations, test-data lifecycle, browser/platform matrices, nondeterminism, and skipped contexts all matter. Conceptual and architectural prose is largely non-executable.

The most defensible use is to treat an executable assertion as a typed documentation dependency. A paragraph that specifies an output can depend on a test oracle; a procedure can depend on an end-to-end test. A generic link from the whole document to an arbitrary source file is weaker.

## 2. Source inclusion and single-source generation

Sphinx's [`literalinclude`](https://www.sphinx-doc.org/en/master/usage/restructuredtext/directives.html) can insert whole source files or select by Python object, line range, or start/end text. mdBook has built-in [`include` and `rustdoc_include` helpers](https://rust-lang.github.io/mdBook/format/mdbook.html) for files, line ranges, and named anchors; `rustdoc_include` can show a focused excerpt while compiling the complete example during `mdbook test`. Asciidoctor supports [tagged include regions](https://docs.asciidoctor.org/asciidoc/latest/directives/include-tagged-regions/). (Accessed 2026-07-10.)

This is prevention rather than detection: the duplicated snippet disappears, so it cannot independently go stale. A docs build can fail if the source path or marker is missing. It is an excellent default for literal code, configuration fragments, CLI help, schema tables, and generated diagrams.

It does not solve semantic prose drift. A current snippet can sit beneath an obsolete explanation. Selectors also define the integrity boundary:

- Absolute line ranges are cheap but brittle under unrelated insertions.
- Text delimiters and region tags are more stable, but add marker maintenance and can collide.
- AST/symbol selectors survive line movement but need language-specific parsing and an overload/rename policy.
- Whole-file includes avoid selector ambiguity but usually overwhelm readers and make small changes noisy.

A dangerous failure mode is silent retargeting: a line range continues to resolve after edits but now displays a different logical block. Named markers or uniquely resolved symbols should therefore be preferred, and a selector identity should be stored separately from its accepted content fingerprint.

Generated reference documentation goes further by making a schema or code model the source of truth. The [OpenAPI Specification](https://spec.openapis.org/oas/latest.html) is explicitly designed as a machine-readable API description usable by documentation, code-generation, and testing tools; [OpenAPI Generator](https://openapi-generator.tech/) supplies documentation generators alongside client/server generators. (Accessed 2026-07-10.) The resulting reference pages cannot drift independently from the OpenAPI input if CI regenerates them and verifies a clean worktree. They can still drift from the running service unless code generation, schema extraction, contract tests, or runtime validation binds implementation to that same input. Handwritten tutorials and rationale remain outside the guarantee.

## 3. Provenance, commit pins, and content hashes

Git already supplies immutable provenance primitives. `git hash-object` computes an object ID from content, defaulting to a blob, and Git's object model identifies blobs and trees by content-derived object IDs; see the official [`git hash-object`](https://git-scm.com/docs/git-hash-object) and [Git user manual](https://git-scm.com/docs/user-manual). GitHub documents that replacing a branch name with a commit ID creates a [permanent link to an exact file or line range](https://docs.github.com/en/repositories/working-with-files/using-files/getting-permanent-links-to-files). (Accessed 2026-07-10.)

These primitives answer different questions:

- A commit permalink answers "what exact repository snapshot did the author inspect?" It deliberately continues showing old code and gives no alert when the branch evolves.
- A full-file blob ID answers "are all bytes of this file identical?" It over-invalidates a paragraph linked to one function.
- A selected-region hash answers "are the accepted selected bytes identical after the checker's defined normalization?" It is the closest mechanical implementation of the proposed drift gate.
- A timestamp answers only an ordering question. Git author/committer dates can be rewritten, cherry-picked, rebased, skewed, or unrelated to when a claim was reviewed. It is useful for service-level reminders, not content identity.

[`snippetdrift`](https://pypi.org/project/snippetdrift/) is unusually direct prior art. Its Markdown sentinel names a source path and line range, records a short SHA-256 fingerprint plus a human-reviewed timestamp, can synchronize the selected lines into a fenced block, and exits nonzero when the current region differs. The package page shows version 0.1.0 released in April 2026, so it should be treated as a small, early OSS implementation rather than mature infrastructure. (Accessed 2026-07-10.)

The crucial logic is asymmetric:

- Hash unchanged is evidence that the selected text did not change. If the document was correct when accepted, this particular source edit did not invalidate it.
- Hash changed is evidence that the selected text changed, not that the document is wrong. The correct action is review, rebind, or update, not an assertion of semantic inconsistency.

Normalization is a policy decision. Raw bytes catch comments, whitespace, formatting, and generated churn. Token or AST hashes reduce noise but can hide comment changes, literal formatting, ordering, annotations, or other behavior meaningful to the document. Retaining both a raw fingerprint and a semantic fingerprint allows CI to classify rather than discard differences.

A whole-commit equality check is generally the wrong granularity: nearly every subsequent commit differs even when the target is untouched. The commit ID is best kept as provenance and as the baseline from which to resolve renames or show a review diff; the actual gate should compare the target node or selected region.

### Tracking a region across versions

The anchoring problem has its own literature, separate from drift detection. LHDiff maps lines
across versions with a content simhash plus context similarity and no parser
([Asaduzzaman et al., ICSM 2013](https://doi.org/10.1109/ICSM.2013.34)). CodeShovel tracks a
method through history by string similarity, following renames and moves, and reproduces complete
histories for 90% of methods (97% of method changes) against a human oracle
([Grund et al., ICSE 2021](https://doi.org/10.1109/ICSE43902.2021.00135)). CodeTracker adds
refactoring-awareness and corrected errors in that oracle
([Jodavi and Tsantalis, FSE 2022](https://doi.org/10.1145/3540250.3549079)). Plain `git log -L`
follows diff hunks and is not rename-durable on its own. The design consequence: assisted anchor
migration is feasible at accuracy levels good enough to propose retargets and not good enough to
apply them silently, which is the migration workflow EC-A3 in
[edge-cases.md](./edge-cases.md) requires.

## 4. Traceability links and documentation-code dependency graphs

### Recovered and learned links in research

Fine-grained linking is longstanding research, and it exposes why naive name matching is insufficient. Dagenais and Robillard's [RecoDoc paper](https://www.cs.mcgill.ca/~martin/papers/icse2012b.pdf) identifies code-like terms in learning resources and resolves them to API elements using document context. On four Java OSS systems, the paper reports average recall and precision of 96%. It also reports that a mechanical method-name match would have failed to find the correct declaration for 89% of mentioned methods because the same method name appeared in many types. The result is strong evidence that context-aware link recovery is feasible in a bounded domain, not evidence that arbitrary repositories or prose can be linked perfectly. (Accessed 2026-07-10.)

The follow-on AdDoc work, ["Using Traceability Links to Recommend Adaptive Changes for Documentation Evolution"](https://doi.org/10.1109/TSE.2014.2347969), discovers coherent sets of code elements documented together and reports violations of those patterns as code and docs evolve. Its retrospective evaluation across four Java OSS projects found that at least half of documentation changes related to existing documentation patterns. That supports graph/pattern-based impact analysis, but the figure is not a detector precision or recall guarantee. (Accessed 2026-07-10.)

DOCER takes a narrower, explainable history-based approach. In ["Detecting Outdated Code Element References in Software Repository Documentation"](https://link.springer.com/article/10.1007/s10664-023-10397-6), a reference is flagged when it existed in both docs and code at the revision when the document was last updated, remains in the document, but its exact whole-word code occurrences fall from positive to zero in the current source. A companion [GitHub Actions paper](https://arxiv.org/abs/2307.04291) runs the check on pull requests. The study found at least one currently outdated reference in 28.9% of 918 analyzed top GitHub projects. (Accessed 2026-07-10.)

DOCER's own limitations are instructive: it does not detect a changed relationship while the identifier remains present, cannot inspect images/video, can mistake changelogs or comments for stale references, and exact matching sacrifices recall. The maintainer follow-up exposed two especially useful false-positive classes: a deleted CMake flag whose documentation remained relevant to users with multiple Python versions, and an identifier removed textually while its functionality remained in program logic. The analysis is main-branch-only; the paper warns that interleaved parallel-branch histories can make branch-exclusive elements appear and disappear and that temporary staleness during ongoing feature work may be acceptable. It demonstrates that git-history provenance can turn an otherwise ambiguous missing name into stronger evidence, but only for a narrow deletion pattern.

Deep just-in-time detectors learn whether a code change invalidates an associated comment. Panthaplackel et al.'s [AAAI 2021 paper](https://ojs.aaai.org/index.php/AAAI/article/view/16119) models comment/code-change pairs and outperforms its baselines, but this class of system depends on known pairs and training distributions; it is a risk classifier, not a proof. (Accessed 2026-07-10.)

### Explicit graph design implications

The evidence suggests that explicit links and inferred links should not have identical semantics:

- An explicit edge is author-declared ground truth for impact analysis, though the author can still be mistaken.
- An automatically recovered edge is a candidate with a confidence score and explanation. It should normally suggest coverage or review, not hard-fail as if certain.
- Historical co-change can propose edges, but a repository where docs were routinely neglected encodes the wrong behavior: lack of co-change may mean missing maintenance, not independence.

For authoring, a documentation-to-artifact edge is sufficient. The checker can derive the reverse index artifact-to-documents for pull-request impact and IDE discoverability. Storing two independent directions creates a second consistency problem. An annotation in code may still be valuable for ownership or discoverability, but should reference the same canonical edge record rather than duplicate it.

A useful edge needs more than two paths. At minimum it needs:

- A stable document anchor, not merely a whole Markdown file.
- Repository identity and target kind: file, region, symbol, schema node, test, generated output, configuration key, command, or external contract.
- A selector and selector version.
- A relation type such as `quotes`, `illustrates`, `asserts-behavior-of`, `generated-from`, `procedure-tested-by`, or `decision-constrains`.
- Accepted target fingerprints and an acceptance commit.
- Owner and policy: blocking, warning, expiry, or review required.
- Resolution status when a target moves, splits, merges, or becomes ambiguous.

Typed relations matter because they change the proof. A `quotes` edge can be mechanically synchronized. An `asserts-behavior-of` edge should prefer an executable oracle. A `decision-constrains` edge may legitimately stay unchanged through many implementation edits and should trigger human impact review rather than byte equality.

## Suspect links in requirements management

The exact mechanic this design proposes for narrative claims, a stored fingerprint that flips to a
review obligation when the target changes, has shipped for years in requirements tools. It is the
strongest prior art in the dossier and it arrives with two decades of recorded failure modes.

### Doorstop

[Doorstop](https://doorstop.readthedocs.io/en/latest/reference/item.html) stores one YAML item per
requirement. A link to a parent carries a stamp: SHA-256 over the parent's identity, text,
references, and link set, in URL-safe Base64. The item's own `reviewed` field holds the same hash
of itself. A link is suspect when the stored stamp differs from the parent's recomputed stamp;
`doorstop clear` re-copies current stamps and `doorstop review` refreshes self-review. Validation
splits severities the way this design does: unresolvable targets are errors, suspect links and
unreviewed changes are warnings. Its issue tracker is a compressed usability study. Links were
originally born suspect and users revolted (issues 173 and 174); the fix trusts links at creation.
A later user was startled that the tool silently refreshed stamps and called it automating too
much (issue 178). Another asked for hash-based review of referenced code regions with
per-reference stamps and built two prototypes (issue 564); it never shipped. The documented remedy
for a stale project is `doorstop review all` followed by `doorstop clear all`, which is
rubber-stamping as an official workflow. Paper:
[Browning and Adams, JSEA 2014](https://doi.org/10.4236/jsea.2014.73020).

### OpenFastTrace

[OpenFastTrace](https://github.com/itsallcode/openfasttrace) stores no fingerprints. A requirement
ID embeds a hand-maintained revision (`req~name~1`), code carries coverage tags naming the revision
they cover, and a semantic change is supposed to bump the revision, flipping every downstream tag
to an Outdated state until each is edited. The edit is the re-attestation and git blame is the
audit trail. Its state vocabulary (Covered, Outdated, Predated, Orphaned, Ambiguous, Unwanted) is
richer than most tools'. The cost is structural: one bump touches every covering file, and the
cosmetic-versus-semantic call is a judgment authors get wrong in both directions. Manual revision
numbers do not transfer to prose documentation; nobody will hand-bump integers per paragraph.

### StrictDoc and Sphinx-Needs

[StrictDoc](https://strictdoc.readthedocs.io/en/latest/) binds requirements to source through
forward relations and reverse `@relation` markers with language-aware scopes (file, class,
function, range), which is the right addressing idea, and stores no content fingerprint, so a
function rewritten under an untouched marker fires nothing.
[Sphinx-Needs](https://www.sphinx-needs.com/) runs filter-based checks over need objects and typed
links at serious scale (automotive programs with more than 100,000 needs; Eclipse S-CORE uses it
for ISO 26262 documentation) and detects dead links, not stale content. Both prove the authoring
formats; neither closes the loop this design closes.

### Commercial suites and the standards floor

Jama Connect lets an administrator choose, per item type and per field, which upstream changes mark
downstream links suspect, which is the noise-control move this design expresses as projections;
triage shows a side-by-side version compare, and the company holds patents on suspect-link
management (US 8,266,591 and relatives) that deserve a legal skim before a commercial attestation
workflow ships. Polarion offers auto-suspect on new links, the default Doorstop users rejected.
Codebeamer makes propagation opt-in per relation and added mass processing of suspected changes
because one-by-one clearing did not scale. Beneath all three, DO-178C and ISO 26262 mandate
bidirectional traceability as certification evidence and require tool qualification (DO-330),
which is why suspect-link workflows are funded at all: compliance, not developer convenience.

### What the record teaches

Five lessons transfer. Treat creation as observation, not trust; show the target diff at acceptance
time, because vetting studies found analysts accept uncertain links with far less scrutiny than
they reject ([Niu et al., FSE 2016](https://homepages.uc.edu/~niunn/papers/FSE16.pdf)); fingerprint
the narrowest stable scope rather than whole files; treat every ecosystem's convergence on bulk
clearing as evidence of ritual compliance, not as a feature request; and split severities between
broken (deterministic, blocking) and suspect (a review obligation). The resulting contract permits
only one-claim acceptance. Typed split and merge lifecycle transactions may name several claims,
but do not serve as bulk acceptance. What does not transfer: automatic trust at creation, manual
revision bumps, completeness matrices demanding every artifact be covered, and one-file-per-item
authoring formats.

## 5. Semantic and API diffing

### Formal contracts

Formal API artifacts enable stronger, domain-specific diffs. [`openapi-diff`](https://github.com/OpenAPITools/openapi-diff) compares OpenAPI 3 specifications, reports endpoint/parameter/request/response changes, and can fail on any change or only incompatible changes. [Buf breaking-change detection](https://buf.build/docs/breaking/) compares current and prior Protobuf schemas and applies configured source, wire, or JSON compatibility rule sets in local development or CI. Buf's documentation explicitly notes that custom options are not generally checked because their semantics are unbounded. (Accessed 2026-07-10.)

These tools prove that a modeled contract changed or violated a formal compatibility rule. They are high-quality triggers for documentation impact:

- A deleted endpoint can require all edges to that OpenAPI operation to be reviewed.
- A renamed CLI option, configuration key, or protobuf field can invalidate exact mentions.
- A compatible addition may require coverage but not invalidate existing prose.
- A behavior change with an identical signature/schema remains invisible.

Language-aware AST or public-API diffing can similarly classify signature, visibility, type, default-value, annotation, and deprecation changes. This is more useful than a raw file timestamp, but it is language/toolchain-specific and struggles with reflection, macros, generated code, runtime configuration, feature flags, and semantic changes inside unchanged signatures.

### Recent semantic research: promising, not mature CI proof

Three recent systems are directly relevant, but their evidence must not be conflated with production maturity.

#### CASCADE (FSE 2026 research tool)

[CASCADE](https://arxiv.org/html/2604.19400) generates unit tests from method documentation, runs them against the existing implementation, then synthesizes an alternative implementation from the same documentation. It only reports an inconsistency when a generated test fails on the existing code but passes on the documentation-derived implementation, and rejects cases where previously passing tests fail on the generated implementation. This dual execution is designed to filter hallucinated tests. (Accessed 2026-07-10.)

On the paper's balanced Java benchmark of 71 inconsistent and 71 corrected pairs, the full system reports precision 0.88, specificity 0.97, and recall 0.21. The ablation is the key result: test generation alone found 41/71 inconsistencies but produced 27 false positives; the full filter reduced this to 15 true positives and 2 false positives, deliberately losing recall. The paper also tested Java, C#, and Rust projects, reported 13 substantial previously unknown inconsistencies, and says 10 were later fixed. Its real-project table also contains many manually filtered false positives, and the authors attribute lower real-world precision to the cheaper model and underspecified/low-quality docs.

What it adds is semantic evidence for observable method behavior even when signatures are unchanged. What it does not provide is a general repo-wide graph, high recall, or proof for architecture/runbook prose. It requires buildable projects, language-specific extraction/execution adapters, testable public methods, LLM calls, and discriminating generated tests. Missing or uncompilable tests become false negatives by design.

#### DocPrism (2025 preprint, ICSE 2026)

[DocPrism](https://arxiv.org/abs/2511.00215) uses a general LLM for post-hoc function/documentation comparison across Python, TypeScript, C++, and Java. Its key idea is Local Categorization and External Filtering: it explicitly filters "under-promises," where good high-level docs omit implementation detail that should not count as inconsistency. (Accessed 2026-07-10.)

In its ablation, the method reduced the flag rate from 98% to 14% and increased accuracy from 14% to 94%. Across 1,615 evaluated pairs in four languages, it reports an average flag rate of 15% and precision of 0.62. The broader extension dataset manually reviewed flagged cases only, so recall and accuracy are unavailable there; language-level function precision ranged from 0.56 to 0.70. This is valuable evidence that a plain "ask an LLM whether docs and code disagree" gate is far too noisy, and that even a purpose-designed filter still leaves roughly 38% of flags false at the reported aggregate precision.

DocPrism is broader than hand-coded patterns and does not require a diff, but it remains function-level, only analyzes already documented functions meeting its extraction criteria, and offers probabilistic findings. Its current evidence supports advisory findings or a second-stage reviewer, not a zero-tolerance blocking gate.

#### ArtifactSync (ICSE 2026 demonstration)

[ArtifactSync](https://das.encs.concordia.ca/pdf/ebube_ICSE2026.pdf) analyzes a target commit, repository tree, README, and diff, then uses hierarchical LLM-guided traversal: file name, structural overview, and finally full content when uncertain. It identifies potentially impacted code, docs, tests, and configuration, assesses inconsistency, and proposes fixes through a CLI/VS Code extension. (Accessed 2026-07-10.)

The evaluation is explicitly preliminary: 20 deliberately injected single-change scenarios across two OSS repositories. It identified the intended impact in all 20 individual scenarios; fixes were fully correct for 18/20. When ten changes per repository were combined into large commits, impact identification fell to 80%, recommendations to 75%, and generated fixes to 65%. The paper's future work proposes persistent memory for learned artifact relationships; the current version reanalyzes each commit.

This is highly relevant architectural prior art for scalable candidate discovery, especially the progressive context strategy. It is a four-page tool-demonstration paper over two repositories, not evidence of mature, deterministic, low-noise CI behavior. The "up to 100% accuracy" headline should always be read with the 20 designed scenarios and the combined-commit degradation beside it.

#### The wider detector record

The 2012-2026 detector literature consistently lands below gate quality and above uselessness.
DRONE checks four types of documented parameter directives against Java code with logic
constraints and achieves high precision exactly because the claims are typed and checkable
([Zhou et al., ICSE 2017](https://doi.org/10.1109/ICSE.2017.11)). Fraco detects comments broken by
identifier renames and found renames to be the dominant silent break
([Ratol and Robillard, ASE 2017](https://doi.org/10.1109/ASE.2017.8115624)). The deep just-in-time
detector reports F1 near 88 on cleaned balanced data and recall 58.3 on the full mixed set
([Panthaplackel et al., AAAI 2021](https://doi.org/10.1609/aaai.v35i1.16119)), missing about four
in ten real inconsistencies. Automatic rewriting is worse: CUP's neural comment updater reaches
16.7% exact-match accuracy ([Liu et al., ASE 2020](https://doi.org/10.1145/3324884.3416581)) and a
token-substitution heuristic beats it on the easy subset
([Lin et al., ICPC 2021](https://doi.org/10.1109/ICPC52881.2021.00013)), which is why this design
never auto-edits prose. A triage-first architecture, deciding whether meaning changed before
attempting anything expensive, measurably lifts both
([Yang et al., TOSEM 2022](https://doi.org/10.1145/3534117)). METAMON grounds LLM judgments in
generated tests and still reports precision 0.72 at recall 0.48
([Lee et al., 2025](https://arxiv.org/abs/2502.02794)). On the authoring side, LLMs suggest
doc-to-code links at F1 79 to 92 when prompted pair-by-pair and collapse to single digits when
asked many-to-many ([Alor et al., 2026](https://arxiv.org/abs/2506.16440)), so link suggestion is
an assisted-authoring feature, not an inference pass. CoDocBench contributes 4,573 mined
code-and-docstring co-change pairs as a public calibration set
([Pai et al., MSR 2025](https://arxiv.org/abs/2502.00519)); an advisory lane should be measured on
it before it comments on anyone's pull request. Behind all of this sits the oldest result in the
file: projects whose comment updates lag their code changes show elevated later defect rates
([Ibrahim et al., JSS 2012](https://doi.org/10.1016/j.jss.2012.04.002)), the empirical reason a
drift signal is worth building at all.

### Consequence for gating

Semantic detectors should initially produce evidence-bearing advisory findings: exact doc span, exact code span/diff, inferred contradiction, confidence, and reproducible test if available. A hard merge block can be justified for deterministic executable failures. Blocking on a probabilistic classification with precision around 0.62 would impose substantial review load; even CASCADE's precision-first 0.88 benchmark configuration misses about four of five known inconsistencies.

## 6. Documentation coverage and structural linting

Rust's [`missing_docs` lint](https://doc.rust-lang.org/stable/nightly-rustc/rustc_lint/builtin/static.MISSING_DOCS.html) detects public items without documentation and can be elevated to a build error. Doxygen can warn on undocumented members, incomplete parameter docs, and documented parameters that do not exist, and can turn warnings into failures; see its [configuration reference](https://www.doxygen.nl/manual/config.html). Rustdoc also checks broken intra-doc links and other structural issues. (Accessed 2026-07-10.)

Coverage is valuable because an edge-based system with zero edges is perfectly green but useless. Suitable metrics include:

- Percent of public API items with reference docs.
- Percent of designated high-risk modules with at least one owning guide/runbook.
- Percent of executable procedures with an associated test.
- Percent of documentation anchors with resolvable dependencies.
- Percent of graph edges whose target and owner are valid.

Coverage must remain a separate dimension from freshness. A boilerplate sentence can satisfy presence; a fully documented symbol can still be wrong. Coverage denominators also require policy: not every private helper deserves documentation, while architecture and operations docs often map to no single symbol.

## 7. Changed-file heuristics, ownership, and co-change

GitHub Actions can [run workflows based on changed path patterns](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/trigger-a-workflow), making rules such as "changes under `modules/api/` run the API-doc checks" straightforward. Danger provides project-specific PR rules and examples that inspect modified files and highlight documentation updates; see the [Danger JS overview](https://danger.systems/js/). GitHub [`CODEOWNERS`](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/about-code-owners) can automatically request and, with branch protection, require approval from owners when matching files change. (Accessed 2026-07-10.)

These are process controls, not consistency proofs. They are attractive because they require no annotations and work on day one. They are also coarse:

- A private refactor can trigger irrelevant doc review.
- A behavior change outside the configured path can escape.
- Requiring "some `.md` changed" is gameable by a trivial edit.
- Renames and directory reorganizations silently invalidate mappings.
- Owners can approve without inspecting every downstream claim.

Repository-mining research infers traceability from files that historically change together. This can bootstrap candidate edges and prioritize review, but it has a cold-start problem and a feedback-loop problem: when documentation has historically failed to change with code, co-change data learns that the artifacts are unrelated. Co-change should therefore be a suggestion signal, never the sole basis for declaring a document safe.

A practical role for changed-file heuristics is as the broad outer net: run the graph checker and semantic analyzers only for likely affected areas, request the right humans, and require an explicit "no documentation impact" acknowledgement for high-risk changes. Exact edges and executable checks can then provide narrower, stronger evidence.

## 8. Commercial and OSS systems close to the idea

### Swimm

Swimm is the clearest commercial precedent for code-coupled documentation. In its official [continuous-documentation CI description](https://swimm.io/blog/continuous-documentation-through-continuous-integration-with-swimm), Swimm says it checks code snippets, "smart tokens," and "smart paths" against the latest repository, uses git history to relocate or auto-synchronize inconsequential changes, and fails verification when changes require human attention. It requires a complete clone because its algorithms analyze history. Swimm's current enterprise material still advertises [documentation checks that alert or block merges](https://swimm.io/enterprise-documentation-platform). (Accessed 2026-07-10.)

This validates product demand and the workflow shape: explicit code coupling, history-aware relocation, automatic sync for safe edits, and CI escalation for ambiguous edits. The available evidence is vendor documentation rather than an independent precision/recall study, and the matching/auto-sync algorithm is proprietary. Its strongest guarantees appear to concern coupled snippets/tokens/paths; vendor language such as "100% up to date documents" should not be read as a formal guarantee for arbitrary prose.

A deeper pass on Swimm's own materials sharpens the picture. Auto-sync stores per-element line
markers, token text, and context, and re-anchors by a signal-based decision over the full Git
history; shallow clones are unsupported for exactly that reason. The pull-request check has three
outcomes: verified, auto-syncable (surfaced as a one-click accept that commits the fix), and
outdated (a human must reselect in the editor). The company raised $27.6M in 2021 ($33.3M total)
and has announced no round since; the recurring adoption complaint in reviews and forum threads is
workflow friction, not detection quality. Both facts are design inputs: commit the fingerprints so
checking needs no history, and point the one-click gesture at attestation rather than auto-editing.

### snippetdrift

`snippetdrift`, described above, is the closest small OSS implementation of path/range + accepted hash + CI failure + explicit human acceptance. Its transparent mechanism has an easy-to-state proof, but fixed line ranges and snippet-only scope are major limitations. Its early 0.1.0 status means scalability, rename handling, and long-term maintenance are not
established, and a second search during the July 2026 revision found no repository presence for it
at all; it appears to be a PyPI-only artifact, which sharpens the maturity caveat.

### Doc Detective

Doc Detective covers executable procedures rather than docs-to-source hashes. It is complementary: a graph can route changed implementation to a procedure, while the procedure's test determines whether user-visible behavior still matches.

### fiberplane/drift

The closest open-source implementation of this design's skeleton, found in the July 2026 second
pass. [drift](https://github.com/fiberplane/drift) (MIT, written in Zig, v0.10.1 in June 2026,
around 119 stars) binds Markdown docs to code with `@./path#Symbol` anchors, stores an XxHash3
fingerprint of a tree-sitter-normalized AST projection per anchor in a repository-root
`drift.lock`, fails CI when fingerprints diverge, re-attests with `drift link`, scopes CI runs
with `--changed`, and answers reverse lookups with `drift refs`. It has no typed claims, no
command-output, graph, or URL targets, no suspect-versus-broken distinction, and no attestation
trail beyond the lockfile's git history. As validation it is ideal: the fingerprint-lockfile
mechanic works without history, in a small fast binary. As competition it defines the deadline:
the differentiating layer is everything above the lockfile.

A later July 2026 check added two material facts. Fiberplane's current README explicitly says
nothing prevents a user from relinking without reviewing prose, the same trust boundary identified
in this dossier. Fiberplane also reports a randomized serializer experiment in which changing the
lock format reduced spurious merge conflicts from roughly 44% to roughly 25%; see the
[Fiberplane engineering blog index](https://blog.fiberplane.com/blog/). The workload and harness
need independent reproduction, but the result is direct evidence that committed relationship state
has operational cost that serialization alone does not remove.

### ryanwaits/drift

A second active OSS project invalidates the earlier “empty quadrant” wording.
[`ryanwaits/drift`](https://github.com/ryanwaits/drift) (MIT, TypeScript, product site showing
v0.42.0 in July 2026) targets TypeScript libraries, SDKs, and CLIs. It exposes fifteen structural,
semantic, example, and prose checks; extracts a portable API representation; validates examples;
ratchets documentation coverage; emits JSON with source positions; supports monorepos and agents;
and ships a GitHub Action. Its scope is narrower than a format- and language-neutral governed
evidence protocol, but it already owns much of the deterministic TypeScript/documentation surface
the second-pass dossier described as future work. Its strongest marketing phrase—finding every doc
that is now wrong—is a vendor/project claim rather than a demonstrated completeness result.

The implication is build-versus-extend, not merely feature differentiation. A new standalone core
must demonstrate a requirement that neither project can support without architectural breakage;
otherwise wrapping, contributing, or staying repository-specific is the cheaper outcome. See
[market-reassessment.md](./market-reassessment.md).

### Dosu's freshness-score recipe

[Dosu's engineering blog](https://dosu.dev/blog/score-documentation-freshness-in-ci) describes a
hand-rolled CI pipeline: a per-page freshness score built from deterministic signals (git age of
the page versus its referenced sources, frontmatter time-to-live contracts, symbol-level checks
that mentioned functions still exist with matching signatures), a service-level gate over the
score distribution with a bypass label, and a middle band routed to an LLM for a non-blocking
verdict at an estimated five to fifteen cents per pull request. It is a recipe attached to their
AI-maintainer product, not a packaged tool, and it is the clearest demand evidence in this file: a
sophisticated team assembled precisely the deterministic-plus-advisory split this design proposes,
by hand, because nothing ships it.

### The AI-rewrite school

Mintlify's agent watches nominated code repositories and opens documentation pull requests from a
prompt. DeepDocs scans pushes on a watched branch and PRs updates to stale READMEs, API pages, and
tutorials. DocuWriter's autopilot reviews merged diffs and suggests regenerated pages. GitBook's
agent ingests support and issue signals and proposes edits, while its OpenAPI references re-poll
their source on a schedule. All of them place a generative model where this design places a
verifier, and a human reviewer is the only gate on the result. They are complementary downstream
consumers of a claim graph (a suspect claim is a good prompt) and unsafe substitutes for one:
nothing in the school flags the page the model never touched, and reviewer approval of plausible
text is the same acceptance bias the link-vetting literature documents.

### Runme and the executable-runbook line

[Runme](https://runme.dev) runs fenced code blocks in Markdown as notebook cells from an editor,
the CLI, or CI, which turns runbooks into executable artifacts and catches command rot the way
doctests catch example rot. Same boundary as Doc Detective: it validates that steps run, not that
the narrative around them is true.

### Snippet verifiers

[embedme](https://github.com/zakhenry/embedme) and
[MarkdownSnippets](https://github.com/SimonCropp/MarkdownSnippets) merge marked source regions
into Markdown and offer verify modes that fail CI when the document no longer matches the source.
embedme's anchor is a path plus line range, with the classic failure: shifted lines either fail on
unrelated edits or silently re-target. Together they confirm both the demand for verified
inclusion and the cost of line-based anchors. [mdox](https://github.com/bwplotka/mdox) extends the
family to command output: fenced blocks declare a command, `mdox fmt --check` re-runs it and fails
CI on divergence, a working transcript-claim precursor.

### Generic docs-as-code tooling

Sphinx, mdBook, Asciidoctor, OpenAPI generators, link checkers, spell/style linters, and site builders eliminate many mechanical forms of rot. They generally do not infer that prose became false after an implementation change. They should be integrated as typed validators, not presented as a complete drift solution.

## Attestation via committed artifact, freshness regimes, and diagram checks

### The api-report pattern

The mainstream world already practices change-triggered human attestation, under a different name,
wherever a public API surface is extracted to a committed file that CI regenerates and diffs.
[Microsoft API Extractor](https://api-extractor.com/pages/setup/configure_api_report/) writes an
`.api.md` report; local builds update it with a warning, production builds fail until the
regenerated report is committed, and the diff drags the semantic change into code review, with
CODEOWNERS routing the right approvers. Kotlin's
[binary-compatibility-validator](https://github.com/Kotlin/binary-compatibility-validator) and
.NET's [PublicApiAnalyzers](https://github.com/dotnet/roslyn-analyzers/blob/main/src/PublicApiAnalyzers/PublicApiAnalyzers.Help.md)
(RS0016 fires in the IDE; shipped and unshipped files record maturity) do the same for their
ecosystems, and [cargo-public-api](https://github.com/cargo-public-api/cargo-public-api) runs it
as a snapshot test. The recorded failure modes transfer verbatim: a formatter rewrote API
Extractor's report and broke comparison (rushstack issue 1856), the same normalization trap as
EC-A2; warnings that should have been failures let API changes slip through Azure's SDK pipeline
(azure-sdk-for-js issue 4282); and Kotlin's single global dump file produces merge conflicts by
construction, the storage lesson behind this design's line-per-claim, shardable ledger.
[Revapi](https://revapi.org) adds one more fragment: silencing an acknowledged break requires a
recorded justification, attestation-with-reason inside a contract checker.
[cog](https://cog.readthedocs.io/en/latest/) contributes the checksum trick: it appends a
fingerprint to each generated region and refuses to overwrite a region whose content no longer
matches it, detecting unattested hand edits.

### Generated-docs check modes and presence enforcement

[terraform-docs](https://terraform-docs.io/reference/terraform-docs/) injects generated module
reference between markers and fails CI with `--output-check` when the committed page differs;
HashiCorp's [tfplugindocs](https://github.com/hashicorp/terraform-plugin-docs) validates doc
structure and schema-versus-docs file coverage;
[TypeDoc](https://typedoc.org/documents/Options.Validation.html) fails builds on unresolvable
typed links, which makes symbol references loud on rename where path anchors stay silent.
[changesets](https://github.com/changesets/changesets) and
[towncrier](https://towncrier.readthedocs.io/en/stable/) enforce that every change ships a
documentation artifact, and [Danger](https://danger.systems/js/) generalizes per-PR presence
rules. All of these check presence or derived equality; none evaluates content truth.

### Freshness-date regimes

Google's g3doc attaches freshness metadata (an owner and a reviewed date) and emails the owner
when a document goes unreviewed too long; the Software Engineering at Google chapter credits the
visible last-reviewed byline with increased adoption. Microsoft Learn requires `ms.date` and
`ms.author` in every page's frontmatter and drives internal review cadence from them. Kubernetes
SIG Docs sweeps for pages untouched for a year and files issues at their owners. Backstage
TechDocs records ownership and defers freshness entirely. The family verdict is uniform: an owner,
a date, a nag, nothing that fails. These regimes are the strongest evidence that dates without
gates do not hold the line, and their owner-plus-reviewed-date record is still the right shape for
the attestation record itself.

### Diagrams versus reality

[ArchUnit](https://www.archunit.org/userguide/html/000_Index.html) can require code dependencies
to adhere to a committed PlantUML component diagram, the one mainstream case of validating a
picture against the build. [dependency-cruiser](https://github.com/sverweij/dependency-cruiser)
encodes architecture rules as CI checks and generates diagrams from the real graph, and
[Structurizr](https://docs.structurizr.com/java/component) extracts the model from code and treats
diagrams as views. Two viable postures, verify the drawing or derive it, and both stop at
dependency topology; the prose around the diagram is out of scope for all three, which is where
graph claims pick up.

## 9. Implications for a new CI checker

The prior art favors a layered model rather than one global "drifted pair" predicate:

1. Eliminate duplication where possible through generation and inclusion.
2. Represent the remaining relationships as canonical, typed documentation-to-artifact edges and derive reverse lookup.
3. Store both immutable provenance and target-level accepted fingerprints; do not compare whole commits as if they were region hashes.
4. Resolve stable selectors first, then classify target changes with raw and semantic diffs.
5. Attach executable validators to behavior claims and procedures.
6. Use schema/API/AST diffs for high-confidence structured impact.
7. Use recovered links, changed paths, co-change, and semantic models to find missing edges and prioritize review.
8. Measure graph/documentation coverage so an empty graph cannot pass as success.
9. Make "changed, needs review," "broken reference," "executable contradiction," and "probabilistic inconsistency" distinct result types with different gate policies.
10. Record a human acceptance event explicitly. Updating a doc timestamp or touching a Markdown file is not equivalent to reviewing a changed dependency.
11. Never auto-edit prose from a detector. The best published comment updater reaches 16.7% exact-match accuracy; flag and request re-attestation instead.
12. Wrap the mechanisms in this file rather than reimplementing them, and emit their marker formats where they already work. The new contribution is the claim layer and the attestation record, not another snippet synchronizer.

The safest initial hard failures are missing targets, selector ambiguity, generated-output drift, failed doctests/procedures, formal contract violations, and changed exact fingerprints without an acceptance decision. Semantic models are best used to enrich those failures and discover unmodeled relationships until project-specific evaluation demonstrates a tolerable false-positive rate.

## Source catalog

All links accessed 2026-07-10.

### Official specifications and tool documentation

- Python Software Foundation, [`doctest`: Test interactive Python examples](https://docs.python.org/3/library/doctest.html).
- Rust project, [The rustdoc book: Documentation tests](https://doc.rust-lang.org/rustdoc/write-documentation/documentation-tests.html).
- Swift project, [Adding Code Snippets to Your Content](https://www.swift.org/documentation/docc/adding-code-snippets-to-your-content).
- Doc Detective, [Introduction](https://docs.doc-detective.com/docs/get-started/introduction) and [Detected tests](https://docs.doc-detective.com/docs/test-docs/detected).
- Sphinx, [Directives: `literalinclude`](https://www.sphinx-doc.org/en/master/usage/restructuredtext/directives.html).
- mdBook, [Including files and `rustdoc_include`](https://rust-lang.github.io/mdBook/format/mdbook.html).
- Asciidoctor, [Include Content by Tagged Regions](https://docs.asciidoctor.org/asciidoc/latest/directives/include-tagged-regions/).
- Git, [`git hash-object`](https://git-scm.com/docs/git-hash-object) and [user manual/object storage](https://git-scm.com/docs/user-manual).
- GitHub, [Getting permanent links to files](https://docs.github.com/en/repositories/working-with-files/using-files/getting-permanent-links-to-files), [workflow path filters](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/trigger-a-workflow), and [`CODEOWNERS`](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/about-code-owners).
- OpenAPI Initiative, [OpenAPI Specification](https://spec.openapis.org/oas/latest.html), and OpenAPI Generator, [project documentation](https://openapi-generator.tech/).
- OpenAPITools, [`openapi-diff`](https://github.com/OpenAPITools/openapi-diff).
- Buf, [Detecting breaking changes](https://buf.build/docs/breaking/).
- Rust project, [`missing_docs`](https://doc.rust-lang.org/stable/nightly-rustc/rustc_lint/builtin/static.MISSING_DOCS.html).
- Doxygen, [Configuration and documentation warnings](https://www.doxygen.nl/manual/config.html).
- Danger, [Danger JS](https://danger.systems/js/).

### Products and OSS implementations

- Swimm, [Continuous Documentation through continuous integration](https://swimm.io/blog/continuous-documentation-through-continuous-integration-with-swimm) and [Enterprise documentation platform](https://swimm.io/enterprise-documentation-platform). These are vendor claims.
- `snippetdrift`, [PyPI project page](https://pypi.org/project/snippetdrift/), version 0.1.0. Package documentation, not an independent evaluation.
- Doc Detective, official docs linked above.

### Research

- B. Dagenais and M. P. Robillard, [Recovering Traceability Links between an API and Its Learning Resources](https://www.cs.mcgill.ca/~martin/papers/icse2012b.pdf), ICSE 2012.
- B. Dagenais and M. P. Robillard, [Using Traceability Links to Recommend Adaptive Changes for Documentation Evolution](https://doi.org/10.1109/TSE.2014.2347969), IEEE TSE 2014.
- W. S. Tan, M. Wagner, and C. Treude, [Detecting Outdated Code Element References in Software Repository Documentation](https://link.springer.com/article/10.1007/s10664-023-10397-6), Empirical Software Engineering 29(5), 2024 (published online 2023), and [GitHub Actions companion](https://arxiv.org/abs/2307.04291).
- S. Panthaplackel et al., [Deep Just-In-Time Inconsistency Detection Between Comments and Source Code](https://ojs.aaai.org/index.php/AAAI/article/view/16119), AAAI 2021.
- T. Kiecker et al., [CASCADE: Detecting Inconsistencies between Code and Documentation with Automatic Test Generation](https://arxiv.org/html/2604.19400), FSE/PACMSE 2026 research paper and tool.
- X. Xu et al., [DocPrism: Local Categorization and External Filtering to Identify Relevant Code-Documentation Inconsistencies](https://arxiv.org/abs/2511.00215), 2025 preprint associated with ICSE 2026.
- E. Alor et al., [ArtifactSync: Automated Repository Synchronization through Hierarchical Change Impact Analysis](https://das.encs.concordia.ca/pdf/ebube_ICSE2026.pdf), ICSE 2026 Demonstrations. Preliminary tool demonstration.

### Requirements-management mechanics (second pass, 2026-07-10)

- Doorstop, [item reference](https://doorstop.readthedocs.io/en/latest/reference/item.html) and [validation](https://doorstop.readthedocs.io/en/latest/cli/validation.html); issues [173](https://github.com/doorstop-dev/doorstop/issues/173), [174](https://github.com/doorstop-dev/doorstop/issues/174), [178](https://github.com/doorstop-dev/doorstop/issues/178), [564](https://github.com/doorstop-dev/doorstop/issues/564); J. Browning and R. Adams, [Doorstop: Text-Based Requirements Management Using Version Control](https://doi.org/10.4236/jsea.2014.73020), JSEA 2014.
- OpenFastTrace, [user guide](https://github.com/itsallcode/openfasttrace/blob/main/doc/user_guide.md).
- StrictDoc, [user guide](https://strictdoc.readthedocs.io/en/latest/).
- Sphinx-Needs, [documentation](https://sphinx-needs.readthedocs.io/en/latest/) and [project site](https://www.sphinx-needs.com/).
- Jama, [clear suspect links](https://help.jamasoftware.com/en/manage-content/coverage-and-traceability/relationships/clear-suspect-links.html); Codebeamer, [suspected links](https://support.ptc.com/help/codebeamer/r2.1/en/codebeamer/user_guide/ug_suspected_links.html) and [mass processing](https://support.ptc.com/help/codebeamer/r2.1/en/codebeamer/user_guide/ug_mass_process_suspected_changes.html).
- N. Niu et al., [Gray Links in the Use of Requirements Traceability](https://homepages.uc.edu/~niunn/papers/FSE16.pdf), FSE 2016.
- S. Maro et al., [Traceability maintenance: factors and guidelines](https://dl.acm.org/doi/10.1145/2970276.2970314), ASE 2016.
- [ReqToCode](https://arxiv.org/pdf/2603.13999), 2026 preprint on embedding traceability structurally in code.

### Products and mechanisms (second pass, 2026-07-10)

- fiberplane, [drift repository](https://github.com/fiberplane/drift) and [announcement](https://fiberplane.com/blog/drift-documentation-linter/).
- Dosu, [Score documentation freshness in CI](https://dosu.dev/blog/score-documentation-freshness-in-ci).
- Swimm, [How Auto-sync works](https://swimm.io/blog/how-does-swimm-s-auto-sync-feature-work) and [GitHub App CI docs](https://docs.swimm.io/continuous-integration/github-app/); TechCrunch, [Series A coverage](https://techcrunch.com/2021/11/08/swimm-nabs-27-6m-series-a-to-include-up-to-date-documentation-in-every-release/).
- Mintlify, [agent automation docs](https://www.mintlify.com/docs/guides/automate-agent); [DeepDocs](https://deepdocs.dev/); [DocuWriter](https://www.docuwriter.ai/); GitBook, [GitBook Agent](https://www.gitbook.com/features/ai/gitbook-agent).
- [Runme](https://runme.dev/); [embedme](https://github.com/zakhenry/embedme); [MarkdownSnippets](https://github.com/SimonCropp/MarkdownSnippets); [mdox](https://github.com/bwplotka/mdox); [cog](https://cog.readthedocs.io/en/latest/).
- Microsoft, [API Extractor report configuration](https://api-extractor.com/pages/setup/configure_api_report/); [rushstack issue 1856](https://github.com/microsoft/rushstack/issues/1856); [azure-sdk-for-js issue 4282](https://github.com/Azure/azure-sdk-for-js/issues/4282).
- Kotlin, [binary-compatibility-validator](https://github.com/Kotlin/binary-compatibility-validator); .NET, [PublicApiAnalyzers help](https://github.com/dotnet/roslyn-analyzers/blob/main/src/PublicApiAnalyzers/PublicApiAnalyzers.Help.md); [cargo-public-api](https://github.com/cargo-public-api/cargo-public-api); [Revapi](https://revapi.org/revapi-site/main/index.html).
- [oasdiff](https://github.com/oasdiff/oasdiff) and its [breaking-changes taxonomy](https://github.com/oasdiff/oasdiff/blob/main/docs/BREAKING-CHANGES.md); [japicmp](https://github.com/siom79/japicmp).
- [terraform-docs](https://terraform-docs.io/reference/terraform-docs/); [tfplugindocs](https://github.com/hashicorp/terraform-plugin-docs); [TypeDoc validation](https://typedoc.org/documents/Options.Validation.html).
- [lychee](https://github.com/lycheeverse/lychee); [muffet](https://github.com/raviqqe/muffet); [MkDocs validation](https://www.mkdocs.org/user-guide/configuration/#validation); Sphinx, [nitpicky mode](https://www.sphinx-doc.org/en/master/usage/configuration.html#confval-nitpicky).
- [changesets](https://github.com/changesets/changesets); [towncrier](https://towncrier.readthedocs.io/en/stable/).
- Google, [Software Engineering at Google, chapter 10 on documentation](https://abseil.io/resources/swe-book/html/ch10.html); Microsoft Learn, [metadata reference](https://learn.microsoft.com/en-us/contribute/content/metadata); Kubernetes SIG Docs, [stale-content thread](https://groups.google.com/g/kubernetes-sig-docs/c/iYjPhoTeYsw); [Backstage TechDocs](https://backstage.io/docs/features/techdocs/).
- [ArchUnit user guide](https://www.archunit.org/userguide/html/000_Index.html); [dependency-cruiser](https://github.com/sverweij/dependency-cruiser); [Structurizr component finder](https://docs.structurizr.com/java/component).

### Research (second pass, 2026-07-10)

- B. Fluri et al., [Do Code and Comments Co-Evolve?](https://doi.org/10.1109/WCRE.2007.21), WCRE 2007; journal extension [Software Quality Journal 2009](https://doi.org/10.1007/s11219-009-9075-x).
- W. Ibrahim et al., [On the Relationship between Comment Update Practices and Software Bugs](https://doi.org/10.1016/j.jss.2012.04.002), JSS 2012.
- Y. Zhou et al., [DRONE: Analyzing APIs Documentation and Code to Detect Directive Defects](https://doi.org/10.1109/ICSE.2017.11), ICSE 2017; [TSE 2020 extension](https://doi.org/10.1109/TSE.2018.2872971).
- I. K. Ratol and M. P. Robillard, [Detecting Fragile Comments](https://doi.org/10.1109/ASE.2017.8115624), ASE 2017.
- Z. Liu et al., [CUP: Automating Just-In-Time Comment Updating](https://doi.org/10.1145/3324884.3416581), ASE 2020; B. Lin et al., [HebCUP](https://doi.org/10.1109/ICPC52881.2021.00013), ICPC 2021; [TSE 2022 extension](https://doi.org/10.1109/TSE.2022.3185458).
- Z. Yang et al., [On the Significance of Category Prediction for Code-Comment Synchronization](https://doi.org/10.1145/3534117), TOSEM 2022.
- S. Subramanian, L. Inozemtseva, and R. Holmes, [Live API Documentation](https://doi.org/10.1145/2568225.2568313), ICSE 2014.
- J. Lee, G. An, and S. Yoo, [METAMON](https://arxiv.org/abs/2502.02794), 2025 preprint.
- K. Pai et al., [CoDocBench](https://arxiv.org/abs/2502.00519), MSR 2025.
- H. N. Dau et al., [DocChecker](https://arxiv.org/abs/2306.06347), EACL 2024 demo.
- E. Alor, S. Khatoonabadi, and E. Shihab, [Evaluating the Use of LLMs for Documentation to Code Traceability](https://arxiv.org/abs/2506.16440), 2026.
- M. Asaduzzaman et al., [LHDiff: A Language-Independent Hybrid Approach for Tracking Source Code Lines](https://doi.org/10.1109/ICSM.2013.34), ICSM 2013.
- F. Grund et al., [CodeShovel: Constructing Method-Level Source Code Histories](https://doi.org/10.1109/ICSE43902.2021.00135), ICSE 2021.
- M. Jodavi and N. Tsantalis, [CodeTracker: Accurate Method and Variable Tracking in Commit History](https://doi.org/10.1145/3540250.3549079), ESEC/FSE 2022.
- G. Antoniol et al., [Recovering Traceability Links between Code and Documentation](https://doi.org/10.1109/32.988497), TSE 2002.
