# Edge cases that kill naive designs

Date: 2026-07-10

Status correction (2026-07-11): the observations remain part of the acceptance corpus; several
original consequences were too broad. In particular, formatter immunity is selector-specific,
automatic observation is not attestation, governed identity is explicit, and unsupported external
or historical scopes cannot look clean. Normative resolutions live in the implementation-readiness
package linked from [README.md](./README.md).

An edge case here is a condition observed in user zero (or in its recorded maintenance history)
that breaks an obvious implementation of the tool: hash a file, compare a timestamp, flag a diff.
Each entry states the observation and the design consequence it forces. The consequences are
reflected in [design.md](./design.md); the observations are verified in
[repo-audit.md](./repo-audit.md) or marked as historical episodes from the maintainer's notes.

| ID | Edge case | Naive design it breaks |
| --- | --- | --- |
| EC-A1 | Regeneration reorders identical code | raw content hashing |
| EC-A2 | Formatter and import-organizer churn | raw content hashing |
| EC-A3 | Renames, splits, and merges | path-keyed selectors |
| EC-A4 | Line ranges silently retarget | line-range selectors |
| EC-A5 | Same-named files and symbols | bare-name selectors |
| EC-B1 | The generator is itself a stale copy | "regeneration passed" as freshness |
| EC-B2 | Generated code is off-limits to edits | fix-it-here diagnostics |
| EC-B3 | Multiple candidate anchors per fact | linking docs to implementation internals |
| EC-C1 | The verification environment is stale | trusting transcript reruns |
| EC-C2 | Environment keys change claim meaning | environment-blind fingerprints |
| EC-C3 | Evidence is slow or nondeterministic | per-PR execution of all validators |
| EC-D1 | External facts rot with zero repo diff | diff-triggered checking |
| EC-D2 | Deployed artifacts lag the tree | comparing docs against main |
| EC-E1 | Docs governed in a different repository | single-repo scope assumptions |
| EC-E2 | Docs wrong at the moment of attestation | treating attestation as truth |
| EC-E3 | Rubber-stamping is the cheapest path | counting attestations as review |
| EC-E4 | High fan-in files invalidate everything | per-claim alerting |
| EC-E5 | CI path filters hide the coupling | reusing the repo's CI change graph |
| EC-F1 | Hand-written counts and totals | prose-only review |
| EC-F2 | Identifier survives, semantics change | identifier-presence checks |
| EC-F3 | True but incomplete prose | equality-only checks |
| EC-F4 | Warnings coupled to invisible semantics | linking prose only to what it names |

## Anchoring and identity

### EC-A1: Regeneration reorders identical code

When user zero split its Isabelle proofs into four sessions, the extracted Scala file came out
semantically identical and completely reordered; the maintainers had to regenerate and reformat it
with a pinned formatter to keep the diff gate meaningful. A claim anchored to that file by raw
content hash would have flipped to suspect on a change that altered nothing.

Consequence: fingerprints need declared normalization layers (post-format, order-insensitive
declaration sets, AST shape) selected per target kind, and generated targets default to structural
rather than byte identity. The raw hash is kept alongside for forensics, never as the sole gate.

### EC-A2: Formatter and import-organizer churn

User zero runs an import organizer in CI and rejects unorganized imports; formatting passes sweep
whole modules without touching behavior. Under whole-file byte hashes, each sweep would re-suspect
every claim in the module.

Consequence: same normalization machinery as EC-A1, plus a policy default: a change that a
formatter round-trip erases is never grounds for suspicion.

### EC-A3: Renames, splits, and merges

User zero's history includes a deliberate identity collapse (three core names became the canonical
names of merged concepts) and a four-way proof-session split. A docs page still links a file
deleted in an earlier refactor. Path-keyed selectors turn every such refactor into a broken claim,
and the wrong recovery (silently retargeting to the most similar new file) is worse than the
breakage.

Consequence: broken selectors are a first-class state with a migration workflow: the tool proposes
candidate retargets from rename detection and content similarity, a human confirms, and the
migration is recorded on the claim. Automatic retargeting is never silent.

### EC-A4: Line ranges silently retarget

The audit itself cites evidence by line span, and those spans were correct on the audit date only.
An insertion above any span shifts it onto different code while the selector keeps resolving, which
is the worst failure shape: no error, wrong target.

Consequence: line ranges are display metadata, never claim identity. Region selectors use explicit
markers, symbol resolution, or quote-plus-context anchoring, and a resolved region that no longer
contains its recorded context degrades loudly to broken rather than quietly to wrong.

### EC-A5: Same-named files and symbols

User zero has two `Backend.scala` files (Z3 and Alloy backends) and repeated object names across
modules. A bare-name selector is ambiguous the day it is written.

Consequence: selectors are qualified (path plus symbol path), ambiguity at declaration time is a
hard error, and ambiguity that appears later (a new same-named symbol) flips the claim to broken
instead of guessing.

## Generated-artifact chains

### EC-B1: The generator is itself a stale copy

User zero's railroad diagrams regenerate on every docs build, and the generator embeds a
hand-copied second grammar that no longer matches the real one. Regeneration succeeds forever;
the output is faithfully wrong. Freshness of a generation step proves derivation from the step's
input, not from the truth.

Consequence: derivation claims name the full input set (source of truth, generator code, templates,
pinned tool versions), and the tool flags a derivation whose declared input is not itself anchored
to the authoritative artifact. "Generated" is a provenance statement, not an assurance level.

### EC-B2: Generated code is off-limits to edits

User zero forbids hand edits to the extracted Scala under its generated directory; fixes must go
through the proof sources. A diagnostic that says "update this file" is actively harmful when the
file is machine-owned.

Consequence: claims record whether their anchor is machine-owned, and remediation messages route to
the declared upstream (the proof theory, the template) instead of the anchor. The reverse index
must distinguish "this file affects these docs" from "edit this file to fix these docs".

### EC-B3: Multiple candidate anchors per fact

A documented output-file tree in user zero could anchor to the emitter code, the templates, or the
committed golden tree. Anchoring to emitter internals would re-suspect the page on every refactor;
the golden is byte-stable, already reviewed, and changes exactly when the described output changes.

Consequence: the tool's guidance and linting prefer the nearest stable derived artifact over
implementation internals, and the docs for claim authors make anchor choice an explicit, teachable
decision, because wrong-anchor noise is the main way authors lose faith in the tool.

## Verification environment

### EC-C1: The verification environment is stale

A recorded episode from user zero's maintenance notes: CLI documentation transcripts kept passing
because the checker ran against a stale native binary; the drift only surfaced when the binary was
rebuilt. The transcript check was green and meaningless.

Consequence: transcript and probe claims fingerprint the toolchain that produced them (binary
digest, tool versions) and record it in the attestation. A green transcript with a mismatched
environment fingerprint is reported as unverified, not as fresh.

### EC-C2: Environment keys change claim meaning

User zero's committed synthesis cache is keyed by LLM model identifier, and a recorded trap: the
cache was built with one model while the compile default was another, so "the cache is fresh"
and "the cache matches what compile produces" quietly diverged.

Consequence: claims about produced artifacts include the producing configuration in their selector
set. Two artifacts that differ only by producing environment are different evidence, and the claim
must say which one it vouches for.

### EC-C3: Evidence is slow or nondeterministic

User zero's proof build takes five to seven minutes and gates commits touching proof files; one
verification obligation tipped past a 300-second solver budget when an invariant grew. Any design
that reruns all executable evidence per PR either becomes the slowest job in CI or gets disabled.

Consequence: validators declare a cost class. Cheap deterministic claims run per-PR; expensive ones
run scheduled or on-demand, and their claims carry the age and fingerprint of the last successful
run, visible in reports. Flaky evidence is demoted to advisory automatically when its pass-rate
history says so.

## External world

### EC-D1: External facts rot with zero repo diff

User zero's docs and configs name vendor model identifiers, solver versions, and dozens of external
URLs. No commit to the repository invalidates a vendor's rename. Diff-triggered checking never
fires.

Consequence: external claims carry a time-to-live and a scheduled probe (URL resolution, version
lookup where an API exists). Expiry produces the same suspect state as a code change, so the review
queue is uniform. This is the one place the tool inherently needs wall-clock time; everywhere else
time is banned as an identity signal.

### EC-D2: Deployed artifacts lag the tree

User zero's playground deploys the latest release, by hand, after each release; between the release
and the manual deploy, or when the deploy is forgotten, the docs describe behavior production does
not have. Docs comparing themselves to `main` are comparing against the wrong universe.

Consequence: claims carry a scope (main, release line, deployed environment). Release-scoped claims
compare against tags; environment-scoped claims are external claims probing the environment's
reported version. The state vocabulary distinguishes "docs ahead of production" from "docs wrong".

## Process and people

### EC-E1: Docs governed in a different repository

User zero's agent-governance files are symlinks into a separate dotfiles repository, and its deploy
notes describe another platform's resources. Claims whose subject and evidence never share a commit
cannot be checked atomically.

Consequence: cross-repo selectors pin the foreign side by content digest and are evaluated on a
schedule; the state vocabulary says "foreign side moved, re-attestation pending" rather than
pretending per-PR freshness. Single-repo mode stays the default so the common case pays no
complexity.

### EC-E2: Docs wrong at the moment of attestation

User zero's architecture page was edited the day before the audit and was already wrong when
edited (the workflow count and a nonexistent filename predate that edit). A fingerprint accepted
that day would have blessed a false page and then correctly flagged the two workflows added later.

Consequence: the product never claims attestation implies truth; states are named for what they
prove (reviewed-at-fingerprint, changed-since-review). First attestation is treated as review, with
the same diff-and-reason UX as re-attestation, and coverage dashboards report "attested" rather
than "verified". This limitation is structural and shapes all product language.

### EC-E3: Rubber-stamping is the cheapest path

User zero's culture already includes one-command golden regeneration; the equivalent gesture here
(re-attest everything, no reading) is the gate's cheapest bypass, and the suspect-link literature
from requirements engineering reports exactly this failure at scale.

Consequence: acceptance is always an explicit one-claim transaction with that claim's evidence and
reason. There is no `accept all` or equivalent bulk path. The only legal multi-claim transactions
are typed lifecycle `split` and `merge` operations with complete predecessor/successor mappings;
they do not make successor claims accepted. Attempts to automate repeated acceptances remain a
ritual-compliance signal for the pilot rather than a supported shortcut.

### EC-E4: High fan-in files invalidate everything

User zero's grammar file backs the DSL reference, the parser page, the convention tables, and the
railroad diagrams; its core IR types back more. One edit to such a file can suspect dozens of
claims across many pages with one root cause.

Consequence: reporting groups by root change, not by claim; one fan-out event is one review task
listing affected claims. Fan-out size is surfaced before enforcement (a claim graph lint), because
a hundred-claim blast radius is a modeling smell that selector narrowing should fix.

### EC-E5: CI path filters hide the coupling

User zero's docs-freshness job triggers on `docs/**`, `fixtures/spec/**`, and the CLI module only,
while parser, verifier, and IR changes also alter CLI-visible output. The repo's own CI change
graph, built to save compute, systematically under-triggers doc checks.

Consequence: the tool computes its own trigger set from declared claim selectors instead of
inheriting the repository's path filters, and its cheap deterministic pass runs unconditionally,
with path-based skipping reserved for the expensive validator classes.

## Content shapes

### EC-F1: Hand-written counts and totals

"Ten workflows" and "23 theories" both appear in user zero's docs; both are wrong; both were
correct once. Humans cannot maintain embedded aggregates.

Consequence: counts get value claims derived from the same selector that defines the counted set,
and the authoring lint flags bare numerals adjacent to inventory claims as candidates. This is the
single cheapest drift class to eliminate outright.

### EC-F2: Identifier survives, semantics change

User zero documents a property as accepting `module:symbol` strategies; the validator accepts only
`live` or `redacted`. Every token in the sentence still exists in the codebase, so
identifier-presence checking (the DOCER approach, and the classic grep audit) passes while the
sentence is false.

Consequence: value claims target semantic surfaces (accepted-value sets, defaults, signatures)
extracted from code, not name occurrence. Where extraction is impossible the claim is honestly
narrative, suspect-on-change, because presence checking would report safety it cannot deliver.

### EC-F3: True but incomplete prose

User zero's target page tree shows only correct files and omits four that ship. Everything stated
is true; the claim fails by omission. Hash- and equality-style checks of the stated items cannot
see it.

Consequence: set-shaped claims (inventory, tree, graph) default to two-sided comparison, and
one-sided intent ("these are highlights, not the full list") must be declared explicitly, which
also makes the page's promise legible to readers.

### EC-F4: Warnings coupled to invisible semantics

User zero's docs warn about operator precedence, a fact of the grammar that no identifier in the
warning names. If precedence changes, no textual link connects the diff to the warning. The claim's
subject is a behavior, not a symbol.

Consequence: narrative claims may select code by semantic locus (a grammar rule, a config schema
node, a function) even when the prose shares no tokens with it; authoring support suggests such
selectors from maintainer knowledge rather than string matching. This is the class that keeps
narrative claims necessary no matter how good extraction gets.
