# v0 contract review: decisions to freeze before implementation

Date: 2026-07-11.

Status: candidate contract analysis, not the normative v0. The later synthesis in
[pre-implementation-review.md](./pre-implementation-review.md) supersedes this file's proposed
content-derived governed identity, trust-on-edit attestation, automatic orphan removal, and
attribution-as-gating defaults. The remaining artifact, endpoint-snapshot, hashing, migration, and
machine-output analysis is input to a future RFC.

Final disposition: [issue-closure-matrix.md](./issue-closure-matrix.md) maps C-01 through C-16 to
the accepted contracts, and [implementation-readiness.md](./implementation-readiness.md) is the
current authorization. This file's JSONL lock, local downgrade, pinned-history v0, and refresh/`ok`
prescriptions are rejected historical alternatives.

## Verdict

The product design is directionally sound, but its persisted contract is not ready to implement yet. The dossier has settled the product semantics—change impact, not truth—but several descriptions still disagree at the exact points that are hardest to migrate later:

- Relationship identity is described once as content-derived and elsewhere as path/section/kind-derived.
- The proposed canonical fingerprint combines the relationship into one digest, while diagnostics and migration require the accepted state of each endpoint.
- The state table treats mutually compatible facts such as “verification passed” and “review required” as exclusive enum cases.
- Authority and invalidation direction are explained together, although they answer different questions.
- `assure.lock` is called an attestation trail, but the representation of an attestation event is not specified.
- Selector-engine versions are included in fingerprints, but upgrade and migration behavior is not specified.
- The persisted digest is variously SHA-256, BLAKE3, or XxHash3.
- Version scope is a required concept but has no serializable shape.
- Human, JSON, SARIF, and exit-code outputs are promised without a stable result model.

Do not write the ledger or public report types until the MUST decisions below are accepted. Parser and extraction spikes that discard all state are safe; a persisted prototype is not.

## Decision register

| ID | Decision | Timing | Proposed v0 default |
| --- | --- | --- | --- |
| C-01 | Separate artifact identity, selector intent, selector resolution, and projection snapshot | **MUST decide before code** | Four distinct types; current content never participates in artifact or selector identity |
| C-02 | Define relationship arity and identity | **MUST decide before code** | Exactly one document-block subject and one-or-more dependency endpoints; content-addressed relationship-instance ID |
| C-03 | Choose combined versus per-endpoint baselines | **MUST decide before code** | Persist every endpoint snapshot; derive one combined attestation seal; acceptance is atomic |
| C-04 | Define the status lattice | **MUST decide before code** | Orthogonal fact axes plus derived labels; policy disposition is not a state |
| C-05 | Separate authority from invalidation direction | **MUST decide before code** | Store both fields independently and validate combinations |
| C-06 | Represent attestation transitions | **MUST decide before code** | Current-state event envelope in the lock, chained to the prior event; Git history is the v0 event store |
| C-07 | Version selectors and migrate baselines safely | **MUST decide before code** | Logical selector ID excludes engine version; every snapshot pins engine and projection schema; no silent re-baseline |
| C-08 | Specify reproducible hashing and encoding | **MUST decide before code** | Full SHA-256 over domain-separated bytes; exact RFC 8785 for structured values; LF-only JSON Lines ledger |
| C-09 | Specify policy composition | **MUST decide before code** | Facts first; repo rules per-key last-match; local downgrade; org floor as final monotone clamp; attribution separately controls gate contribution |
| C-10 | Specify version scope | **MUST decide before code** | Tagged scope union; implement co-versioned workspace and pinned local revision in v0 |
| C-11 | Freeze machine report and exit semantics | **MUST decide before code** | Deterministic JSON v1 is normative; human and SARIF are projections; exit 0/1/2 |
| C-12 | Freeze document discovery and path rules | **MUST decide before code** | Use the OP-17 prose/agent-file set, not “every non-code text file” |
| C-13 | Cross-repository, environment, and external-TTL evaluation | Safe to defer | Reserve scope tags; parse unsupported kinds as explicit `unsupported`, never as clean |
| C-14 | Multi-subject relationships and partial endpoint acceptance | Safe to defer | Prohibit both in v0 |
| C-15 | Append-only standalone audit log, signatures, and reviewer identity proofs | Safe to defer | Git-backed audit only; make that limitation explicit |
| C-16 | Plugin ABI, LLM lane, and arbitrary probes | Safe to defer | Built-in deterministic selectors only; reserve namespaced mechanism IDs |

## Contract principles

The following rules should be normative, not implementation commentary:

1. A declaration states intent. A resolution is what one evaluator found in one tree. A projection is the canonical evidence bytes produced from that resolution. A baseline is an attested set of projection snapshots. These are different objects.
2. Every persisted digest names its algorithm, domain, and schema version.
3. Facts are computed without policy. Policy maps facts to dispositions. A report includes both.
4. A relationship can carry several simultaneous facts. No single enum may erase evidence.
5. A changed dependency proves impact, not falsity. The schema must not contain a state named `true`, `correct`, or unqualified `up-to-date`.
6. No automatic refresh operation may alter an existing accepted endpoint snapshot. Only an explicit attestation or explicit schema migration may do so.
7. Missing history may reduce diff quality, but it must not change the equality result.
8. Unknown, unsupported, ambiguous, skipped, and unevaluated are not successful outcomes.

These principles are the direct transfer from the prior art. Doorstop shows why a suspect link is a review obligation; the api-report family shows why a committed diff is an accepted workflow; fiberplane/drift shows that a committed fingerprint can avoid history for checking; Swimm shows that history is still useful for re-anchoring and rich diffs; CASCADE and DocPrism show why probabilistic judgments must remain separate from deterministic facts. See [prior-art.md](./prior-art.md) and [failure-modes.md](./failure-modes.md).

## C-01: canonical artifact and selector model

**Timing: MUST decide before code.**

The model should have four layers.

### Artifact reference

An artifact reference identifies a thing independently of its current content.

| Field | v0 type | Contract rule |
| --- | --- | --- |
| `artifact_kind` | `document`, `repository-file`, `repository-tree`, `named-check` | Closed v0 enum; extensions use a namespaced value |
| `repository` | literal `self` | Cross-repository identities are reserved, not implemented |
| `locator` | tagged object | Canonical repository-relative path, root, or named-check ID |
| `version_scope` | `ScopeSpec` | Part of artifact identity; defined in C-10 |
| `artifact_id` | derived digest | Derived only from the preceding semantic fields |

Line, column, heading text, current blob ID, current content hash, and display URL are not artifact identity. They belong to resolution or diagnostics.

Repository paths use `/`, are relative to the repository root, contain no `.` or `..` segments, preserve case, and must be valid UTF-8 in v0. A tracked non-UTF-8 path may be a dependency only through a future byte-path selector; v0 reports it as unsupported. Symlinks are artifacts whose content is the link target. The resolver never follows a link outside the checkout.

### Selector specification

A selector states which projection of an artifact matters.

| Field | v0 type | Contract rule |
| --- | --- | --- |
| `selector_kind` | tagged string | Examples: `document-block`, `path-exists`, `file-content`, `path-set`, `symbol-source` |
| `selector_schema` | positive integer | Version of the selector's arguments and semantic meaning |
| `artifact_id` | artifact reference | The thing being selected |
| `parameters` | canonical JSON object | No floats; unknown keys rejected by that selector schema |
| `cardinality` | `exactly-one`, `one-or-more`, `zero-or-more` | Resolution must report a cardinality violation explicitly |
| `projection_kind` | tagged string | Defines the canonical output contract, not merely how to locate it |
| `selector_id` | derived digest | Includes logical selector fields, but not engine implementation version |

The selector ID must remain stable when a parser implementation is upgraded without changing selector semantics. The engine version belongs in the result snapshot, where it can force migration without pretending that the author selected a different thing.

The same artifact can have several selectors. A module inventory should use `path-set`; prose about a function body may use `symbol-source`; a link claim may use `path-exists`. This is the projection lesson from API/schema diff tools and from the false-positive record of whole-file hashes.

`document-block` has one extra identity projection because day-zero trust-on-edit depends on it. The extractor emits a `unit_identity_digest` over the canonical block bytes under a separately versioned `unit_identity_schema`. The subject selector may use structural location hints to find the block, but those hints are not accepted evidence. A meaningful change to the identity projection creates a new unit; a parser-only engine upgrade that emits identical identity bytes does not. Changing the identity schema itself is a relationship-identity migration, not an ordinary endpoint-baseline migration.

### Selector resolution

Resolution is ephemeral run output:

- resolution status: `resolved`, `missing`, `ambiguous`, `unsupported`, or `error`;
- the zero-or-more concrete resolved artifact locators;
- current display locations;
- the selector engine ID, engine version, and projection-schema version;
- the canonical projection digest and optional raw digest;
- a bounded display summary suitable for a focused diff;
- the actual resolved version identifier for each endpoint.

Never persist “missing” as if it were an artifact. Persist the selector intent, then recompute its resolution. This preserves the distinction between a broken relationship and a relationship that intentionally selected an empty path set.

### Projection snapshot

A projection snapshot is the accepted or current result for one endpoint:

| Field | Required | Purpose |
| --- | --- | --- |
| `selector_id` | yes | Connect snapshot to logical selector |
| `resolution_status` | yes | Only `resolved` can be attested in v0 |
| `projection_digest` | yes | Validity comparison |
| `raw_digest` | when available | Forensics; never the sole semantic gate when a normalized projection exists |
| `selector_engine` | yes | Engine ID and exact version/digest |
| `projection_schema` | yes | Meaning of projection bytes |
| `resolved_scope` | yes | Actual revision/tree used during evaluation |
| `member_summary` | optional, bounded | Selected member IDs for explainability; includes count and explicit truncation |
| `display_summary` | optional, bounded | Focused evidence for humans; never used for equality |

Full resolved members and projection bytes are run output, not necessarily ledger content. The v0 ledger commits their digest and bounded summaries. Consequently, exact added/removed-member or body diffs require either Git history or a future projection body store; C-03 makes that limitation machine-visible.

Artifact IDs and selector IDs are declarations. Projection snapshots are observations. Mixing them would make engine upgrades look like artifact changes and make content changes rewrite graph identity.

## C-02: canonical relationship model and identity

**Timing: MUST decide before code.**

The v0 relationship is a directed hyperedge with exactly one subject endpoint and one-or-more dependency endpoints.

| Field | v0 rule |
| --- | --- |
| `relationship_schema` | Integer `1` |
| `relationship_id` | Content-addressed instance ID described below |
| `subject` | Exactly one `document-block` endpoint |
| `dependencies` | Non-empty, deduplicated set of selector endpoints |
| `relation_type` | `describes` for inferred day-zero links; tagged enum reserves future kinds |
| `authority` | Explicit enum from C-05 |
| `invalidation` | Explicit enum from C-05 |
| `assurance_mechanism` | `attestation`, `existence`, `equality`, or a namespaced extension |
| `scope` | Version scope from C-10 |
| `origin` | `inferred`, `declared`, or `managed`; not part of identity |
| `declaration_location` | Display metadata only |
| `current_attestation` | Event envelope from C-06, if one exists |

Zero-link documents are legal. Zero-dependency relationships are not. A document with no relationships is a coverage observation about the document, not a degenerate hyperedge that can turn green.

### Identity default

The current dossier deliberately makes a meaningful documentation edit a new unit identity, so that editing the block clears its old staleness without an extra gesture. Preserve that v0 behavior, but name it honestly: the ID is a relationship-instance ID, not a permanent logical-claim ID.

Derive it from:

- canonical document path;
- normalized document-block projection digest;
- relation type, authority, invalidation, assurance mechanism, and version scope;
- sorted logical dependency selector IDs;
- an ordinal only among exact duplicates of all preceding identity inputs.

Do not include policy, owner, current line/heading, origin, selector engine version, projection result, attestation, or timestamps.

This resolves the dossier's two conflicting definitions. Section anchors remain an addressing and display view. Claim kind and dependency intent participate in identity. A meaningful subject edit or retarget creates a new instance. A target-content change does not.

The cost must be explicit: v0 has no durable logical claim identity across meaningful prose edits. Refresh removes the old instance and adds the new one; Git shows the transition. Unique exact-content document moves may record a `supersedes` migration, but heuristic lineage is not truth. Stable authored IDs and heuristic lineage tracking are safe to defer.

Automatic retargeting rules differ by endpoint:

- A byte-identical document file rename may migrate automatically only when the old-to-new match is unique, and the migration is recorded.
- A dependency path/symbol rename may be proposed but never accepted silently, even at high similarity. This is the CodeShovel/CodeTracker and Swimm lesson: re-anchoring accuracy is useful for candidates, not authority.

## C-03: baseline granularity

**Timing: MUST decide before code.**

Persist baselines per endpoint and derive a combined seal. Do not persist only the single combined fingerprint shown in the current design sketch.

For every attestation, store:

- the accepted subject projection snapshot;
- one accepted projection snapshot per dependency selector;
- validator/environment snapshots where the mechanism requires them;
- one `attestation_seal` derived over the canonical relationship semantics and all accepted snapshots.

Per-endpoint state is required for all of the product's claimed advantages: focused diffs, root-cause grouping, fan-out analysis, selector migration, added/removed dependency diagnosis, and machine-readable evidence. A single digest can say “something changed” and nothing more.

The combined seal still matters. It detects hand edits to one snapshot, binds the accepted set atomically, and lets `verify-lock` validate internal consistency cheaply.

### Atomicity default

An attestation accepts the entire relationship at one logical event. v0 must not support partial endpoint acceptance. Mixed-epoch hyperedges are difficult to explain and no longer mean “this set of evidence was reviewed together.” `assure ok --target X` may accept many relationships in one batch, but it creates one complete event per relationship with a shared deterministic batch ID.

### Checkability versus diff availability

A digest is sufficient to check equality and insufficient to reconstruct the old text. The dossier currently promises both no history and a focused target diff; those guarantees cannot both hold for arbitrary file-content selectors unless accepted projection bodies are stored.

Proposed v0 default:

- Checking remains offline and digest-only.
- The lock stores bounded summaries for structured projections such as path sets and public shapes.
- PR attribution always has a base-versus-candidate diff and can show the change introduced by that PR.
- A full since-attestation file/body diff is best-effort, using Git history to locate the commit where the attestation event entered the lock.
- Machine output reports `baseline_diff_availability` as `available`, `requires-history`, or `unavailable`.
- `assure ok` must say when it cannot reconstruct the old projection; unchanged-doc and bulk acceptance still require a reason.

If an exact historical diff in every shallow checkout is a product requirement, the alternative is a content-addressed projection store, which contradicts the one-small-lock and ledger-size goals. That is safe to defer, but the v0 UX must not promise the unavailable diff.

## C-04: status lattice

**Timing: MUST decide before code.**

Do not implement `RelationshipStatus` as one enum. A relationship may simultaneously be review-required, verification-passed, partly unsupported, historical, pre-existing, and report-only. The canonical result is a product of fact axes.

| Axis | v0 values | Notes |
| --- | --- | --- |
| Declaration | `valid`, `invalid` | Schema/identity/config facts |
| Resolution, per endpoint | `resolved`, `missing`, `ambiguous`, `unsupported`, `error` | Preserve every endpoint result |
| Baseline | `absent`, `present` | Absence is not clean |
| Delta, per endpoint | `equal`, `changed`, `unknown` | `unknown` when comparison cannot be completed |
| Baseline provenance | `bootstrap`, `authored-or-edited`, `explicit-attestation`, `schema-migration` | Prevent mass init from masquerading as human review |
| Validation, per validator | `not-applicable`, `not-run`, `passed`, `failed`, `error`, `skipped` | Independent of attestation delta |
| Scope | `matched`, `unavailable`, `mismatch` | A wrong release is not a broken selector |
| Lifecycle | `active`, `historical`, `planned` | Classification fact, not enforcement |
| Attribution | `introduced`, `worsened`, `pre-existing`, `resolved`, `not-applicable` | Computed from base/candidate fact sets |
| Disposition | `ignore`, `report`, `fail` | Derived by policy, never stored as evidence |

Derived human labels are projections over these facts, not persisted truth:

- `broken-selector`: any required endpoint is missing or ambiguous.
- `unsupported`: a required endpoint or mechanism cannot be evaluated.
- `verification-failed`: any blocking deterministic validator failed.
- `review-required`: the subject instance has a baseline and at least one dependency projection changed.
- `new-unbaselined`: no baseline exists for the current relationship instance.
- `baseline-current`: all accepted endpoint projections equal current projections.
- `clean-attested`: `baseline-current` with explicit or authored/edited provenance and no unresolved facts.
- `verification-passed`: all executed validators passed; it can coexist with `review-required`.

`probable-broken` and `generated-reference` are finding kinds produced during inference, not assurance states. `exempt` is a policy decision, not an observed property. `fresh` should not appear in the JSON vocabulary because users will read it as semantic freshness; `baseline-current` says exactly what is known.

For a hyperedge, combination is conservative: retain the set of endpoint facts, union changed/broken endpoint sets, and derive the highest applicable diagnostic label for display. Never let one passed validator mask a broken selector or unknown evaluation. This is a partial order, not a total “health score,” which preserves the assurance distinctions emphasized throughout [prior-art.md](./prior-art.md).

## C-05: authority and invalidation direction

**Timing: MUST decide before code.**

Authority answers “which side wins when the artifacts disagree?” Invalidation answers “which change creates a review obligation for which side?” They are correlated but not identical and must be separate fields.

Use these v0-compatible enums:

| Field | Values |
| --- | --- |
| `authority` | `subject`, `dependencies`, `joint`, `none` |
| `invalidation` | `dependencies-to-subject`, `subject-to-dependencies`, `bidirectional`, `none` |

Validated defaults by relation type:

| Relation | Authority | Invalidation | Default mechanism |
| --- | --- | --- | --- |
| Inferred prose describes code | `dependencies` | `dependencies-to-subject` | Attestation |
| Normative specification constrains code | `subject` | `subject-to-dependencies` | Validator or attestation |
| Generated output derives from inputs | `dependencies` | `dependencies-to-subject` | Regenerate/equality |
| Two artifacts claim equivalence | `joint` | `bidirectional` | Two-input validator |
| Historical snapshot | `none` | `none` | Integrity at pinned scope |

v0 only needs to create the first and historical forms, but the two independent fields must exist now. A generic stored arrow from docs to code is merely declaration ownership; it is not enough to infer authority. Reverse navigation remains a derived index and never creates another relationship record.

## C-06: attestation event representation

**Timing: MUST decide before code.**

An attestation is a state transition, not just a new hash. Its logical event envelope should contain:

| Field | v0 rule |
| --- | --- |
| `attestation_id` | Domain-separated digest of the canonical event payload |
| `previous_attestation_id` | Optional; creates a transition chain |
| `relationship_id` | Current relationship instance |
| `decision` | Enum below |
| `subject_snapshot` | Required accepted endpoint snapshot |
| `dependency_snapshots` | Required complete sorted set |
| `validator_snapshots` | Required when the relationship declares validators |
| `reason` | Required for unchanged and bulk acceptance; optional for bootstrap and subject edit |
| `batch_id` | Optional deterministic ID shared by bulk operations |
| `supersedes_relationship_id` | Optional, only for explicit/machine-recorded migration |
| `attestation_seal` | Combined digest from C-03 |

Decision values:

- `bootstrap`: initial mass baseline; explicitly not evidence of individual human review.
- `authored-or-edited`: a new subject instance merged and trusted under the trust-on-edit policy.
- `accepted-unchanged`: dependencies changed, prose did not, and a person explicitly accepted it.
- `accepted-updated`: explicit acceptance accompanying a relationship retarget or managed update.
- `retargeted`: a dependency selector changed with human approval.
- `schema-migration`: mechanical representation migration under the C-07 constraints.

Bulk is a property (`batch_id` plus batch scope), not a weaker decision enum. Every affected relationship receives its own complete event.

### Actor and time default

Do not put an authoritative actor or wall-clock timestamp into the reproducible lock in v0. A local contributor can spoof either, and a timestamp makes identical acceptance non-reproducible. The Git commit that changes the lock supplies the v0 event envelope for authorship and time; branch protection and review supply organizational authority. The JSON report may add SCM actor/reviewer/time metadata as explicitly unverified or provider-verified run context, but it is not part of content validity.

This means the v0 audit trail is Git-backed, not self-contained and not cryptographically attested. Say so. Signed app events, Sigstore identities, and a standalone append-only audit service are safe to defer.

### Physical v0 event storage

Store the current event envelope with each active relationship record. Updating an attestation replaces that record line; `previous_attestation_id` points backward and Git history contains the prior line. Removing an orphan removes its line, and that deletion is the retirement record in Git. A future standalone event log can consume the same event schema without changing relationship semantics.

The refresh lane may add new records and remove orphan records. It MUST NOT modify an existing record. `assure ok` and `assure migrate` are the only commands that may change one. This preserves OP-19's never-update invariant.

## C-07: selector versioning and migration

**Timing: MUST decide before code.**

Every selector has three versions with different meanings:

1. `selector_schema`: the authored argument/intent schema.
2. `selector_engine`: the implementation that resolves the selector.
3. `projection_schema`: the canonical bytes the engine emits.

Only the first and the logical parameters participate in `selector_id`. Every baseline snapshot pins all three so an upgrade cannot reinterpret an old digest silently.

Upgrade rules:

- If only implementation code changes and old/new engines emit byte-identical canonical projections on the same current tree, the result remains comparable.
- If projection semantics change, emit `migration-required`; do not label the relationship impacted or clean until migration is resolved.
- A metadata-only `schema-migration` event may update an existing clean baseline only when the old engine can prove that the current old projection still equals the accepted old digest and the new projection is derived from that same current tree.
- If the relationship was already impacted under the old engine, migration MUST NOT turn it clean. Keep the old evaluator available, or require explicit human re-attestation.
- If the new binary cannot evaluate the pinned old schema, report `unsupported`/`migration-required`; never create a new baseline automatically.
- Changes to selector intent or arguments create a new selector ID and require a `retargeted` event, not a schema migration.

The subtle requirement is retention: a binary that changes a persisted projection schema should carry the previous evaluator or an explicit migration function for at least the supported migration window. Storing only old digests makes arbitrary migration impossible. This needs a compatibility policy before the first release, even if v0 ships only schema `1`.

Document moves and code refactors are locator migrations, not selector-engine migrations. Exact unique document moves may be machine-recorded; dependency retargets need human approval.

## C-08: reproducible hashing contract

**Timing: MUST decide before code.**

Choose SHA-256 for every persisted identity and seal in v0. BLAKE3 or XxHash3 may key local caches, but no cache digest appears as a portable artifact ID. SHA-256 matches Doorstop and the broader content-addressed ecosystem; algorithm agility remains explicit.

Define one primitive:

```text
H(domain, value) = SHA-256(UTF-8(domain) || 0x00 || JCS(value))
```

Where `JCS` is a frozen RFC 8785-style canonical JSON encoding and the contract forbids floating-point values. Use distinct domains such as:

- `assure/artifact-id/v1`
- `assure/selector-id/v1`
- `assure/unit-id/v1`
- `assure/relationship-id/v1`
- `assure/projection/v1`
- `assure/attestation/v1`
- `assure/attestation-seal/v1`

Every serialized digest is `sha256:` followed by 64 lowercase hexadecimal characters. Never persist truncated digests.

Canonicalization defaults:

- JSON object keys use canonical ordering; arrays are ordered unless the field contract explicitly defines a set.
- Set-shaped values sort by bytewise UTF-8 canonical key and reject duplicates.
- Metadata strings are NFC-normalized. Repository file-content projections do not Unicode-normalize source bytes unless that projection schema explicitly says so; visually equivalent code points can be semantically distinct.
- Text projections normalize CRLF and CR to LF and preserve all other bytes/code points, trailing whitespace, and final-newline presence unless a stronger selector-specific projection says otherwise.
- Binary projections hash raw bytes.
- Repository paths use canonical `/` separators and case-sensitive UTF-8 bytes.
- Path-set projections include path and Git mode; symlink projections include link-target bytes. A submodule entry includes its pinned object ID and is unresolved if required content is unavailable.
- Parser/token/AST projections name grammar/query and projection-schema versions. Comments remain included for v0 token projections because documentation can depend on them.
- The ledger, configuration digest, and report have their own schema domains. Never hash a serialization whose version is implicit.

Publish golden hash vectors before the implementation is considered compatible. At minimum cover empty sets, Unicode, CRLF, a symlink, executable-bit change, duplicate rejection, sorted dependencies, and a selector-engine upgrade. Git's SHA-1/SHA-256 transition is exactly why no contract may assume a Git object ID is the persisted fingerprint format.

## C-09: policy layering

**Timing: MUST decide before code.**

Policy consumes findings after facts are computed. It cannot change resolution, projection equality, attestation provenance, or validation results.

Use the ordered disposition lattice:

```text
ignore < report < fail
```

Composition algorithm:

1. Classify facts into finding kinds using built-in deterministic rules.
2. Start with built-in default dispositions.
3. Apply repository `[policy]` defaults.
4. Apply matching repository `[[rule]]` entries in file order, per key, last matching value wins.
5. Apply an allowed local skip as a one-step downgrade only.
6. Apply the organization floor as `max(current, floor)`; this is the final monotone clamp.
7. Apply run-context attribution: in pull-request mode, a `fail` finding that is wholly pre-existing is reported in the cleanup section rather than failing this PR; default-branch mode enforces the configured disposition on all current findings.

Every output finding records the built-in default, matching repo rule ID/location, local downgrade, organization clamp, attribution, and effective disposition. A user must be able to answer “why did this fail?” without reproducing rule evaluation manually.

Proposed defaults:

| Finding kind | Default |
| --- | --- |
| Invalid declaration or lock corruption | `fail` |
| Missing/ambiguous active local selector | `fail` |
| Deterministic verification failure | `fail` |
| Dependency changed since baseline | `report` |
| Unsupported or migration-required | `report`; `fail` only on a protected claimed surface |
| Probable broken reference | `report` |
| Generated/gitignored absent reference | `ignore`, always tallied |
| New unbaselined inferred relationship | `ignore` on PR, tallied; refresh creates baseline after merge |
| Heuristic/LLM suspicion | `report` at most |

Unknown future finding kinds default to `report`, not `ignore` and not `fail`.

`exclude` is discovery control, not a fourth disposition. It cannot combine with other keys. Organization-protected paths force scanning and at least `report`, so repository configuration cannot walk them out of scope. Ignored and skipped counts always appear in summaries.

The dossier's `local_override = false`, protected-path set, and per-key gitignore-style matching fit this algorithm. The important addition is that policy never rewrites the fact vector and that the organization floor is unambiguously last.

## C-10: version scopes

**Timing: MUST decide before code.**

Version scope belongs in the artifact/selector semantics and attestation snapshot, not only in policy prose. Use a tagged union from the first schema version.

| Scope kind | Meaning | v0 support |
| --- | --- | --- |
| `co-versioned-workspace` | Resolve subject and dependencies in the evaluated candidate tree | Full support; default |
| `pinned-local-revision` | Resolve against an immutable local Git object/revision | Full support when the object is available |
| `release-line` | Resolve against a configured moving release ref | Reserved; report unsupported |
| `foreign-content` | Resolve another repository/artifact pinned by digest | Reserved; report unsupported |
| `deployed-environment` | Resolve a running environment/version endpoint | Reserved; report unsupported |

The actual resolved revision/tree identifier is stored per projection snapshot. A mutable branch name is not an attested revision. `main` is not a special validity scope; pull requests evaluate the merge candidate and default-branch runs evaluate their checkout.

Historical documents use `pinned-local-revision` and `invalidation = none`. If the pinned object is absent from a shallow checkout, the fact is `scope = unavailable`, not `broken-selector`. The checker may request an explicit fetch in a non-blocking enrichment lane, but core equality cannot silently fall back to current main.

Release/audience/platform/feature variants are real, as [edge-cases.md](./edge-cases.md) and Docusaurus prior art show. Safe v0 default: preserve optional string labels as report metadata, but do not give them comparison semantics until a later scope schema defines them. Do not allow a free-form label to affect validity implicitly.

## C-11: ledger, reporting, and interoperability

**Timing: MUST decide before code.**

### Physical lock format

Use canonical JSON Lines for `assure.lock`:

- first line: one `meta` record with ledger schema, digest contract, and minimum reader version;
- remaining lines: one current relationship/baseline record each;
- relationship lines sorted by full `relationship_id`;
- each line independently canonical JSON, LF terminated, with a final LF;
- no comments, generated timestamps, host paths, random IDs, or run-specific ordering;
- unknown fields rejected for the current major schema;
- `verify-lock` recomputes every derived ID and seal, checks sort/uniqueness, and rejects unsupported schema versions.

Declarations remain in documentation. Named-check definitions and policy remain in the root TOML. The lock stores the accepted selector set and current attestation snapshot for each relationship. The reverse index and caches are derived and uncommitted.

The lock proves internal consistency, not authenticity. Git history and review policy are the v0 authenticity boundary. Protect the lock, policy, and selector configuration with ownership.

### Normative JSON result

`assure check --format json` is the machine contract. It must include:

- report schema/version and tool version;
- evaluation mode, base/candidate identifiers when applicable, and ledger digest;
- every evaluated relationship ID and origin;
- the complete fact axes from C-04;
- changed, broken, unsupported, and validation endpoint details;
- derived labels;
- every finding's kind, attribution, policy trace, and effective disposition;
- primary document location and related artifact locations as display metadata;
- attestation provenance/decision and baseline-diff availability;
- coverage observations for documents with zero relationships;
- counts of ignored, skipped, excluded, unsupported, and truncated findings;
- deterministic summary and exit class.

Arrays sort by stable ID. Diagnostics sort by relationship ID, finding kind, then canonical location. The deterministic report contains no current timestamp, hostname, random run ID, ANSI text, or absolute checkout path. Optional run metadata belongs in a separate non-deterministic envelope.

An illustrative relationship result shape is:

```json
{
  "relationship_id": "sha256:...",
  "origin": "inferred",
  "relation_type": "describes",
  "facts": {
    "baseline": "present",
    "baseline_provenance": "explicit-attestation",
    "changed_endpoints": ["sha256:..."],
    "resolution": [
      {"selector_id": "sha256:...", "status": "resolved"}
    ],
    "validation": []
  },
  "labels": ["review-required"],
  "findings": [
    {
      "kind": "dependency-changed",
      "attribution": "introduced",
      "configured_disposition": "fail",
      "effective_disposition": "fail",
      "policy_trace": ["builtin", "repo:rule-3"]
    }
  ],
  "baseline_diff_availability": "available"
}
```

The public schema should use additive minor changes and a new major schema for changed meaning. Consumers must ignore unknown additive fields but reject unknown major versions and unknown enum values that affect validity.

### SARIF projection

SARIF is a projection of the JSON facts, not a second analysis model:

- one `ruleId` per finding kind;
- primary location at the document unit;
- dependency changes as `relatedLocations`;
- full relationship ID in `partialFingerprints` so findings survive line movement;
- SARIF level derived from effective disposition;
- `baselineState` derived from attribution, not from line comparison;
- ignored tallies in run properties, not silently discarded.

This follows SARIF's stable-fingerprint lesson in [sources.md](./sources.md).

### Exit codes and streams

Freeze these before users script the CLI:

- `0`: evaluation completed and no effective `fail` finding exists.
- `1`: evaluation completed and at least one effective `fail` finding exists.
- `2`: invocation, configuration, ledger, schema, or analysis failure prevented a complete trustworthy evaluation.

JSON/SARIF goes to stdout; logs and progress go to stderr. An unsupported selector on an unprotected advisory relationship is a completed evaluation and follows policy; a parser crash or unreadable ledger is exit `2`.

Human text is not a parsing API. Existing doctest, link-checker, contract-diff, and architecture-test adapters should eventually emit a namespaced evidence-result shape that feeds this same JSON model, but the process/plugin ABI is safe to defer.

## C-12: discovery boundary

**Timing: MUST decide before code.**

The dossier alternates between “every non-code text file” and the narrower OP-17 set. Implement the narrower, reproducible set:

- prose extensions: `.md`, `.mdx`, `.markdown`, `.rst`, `.adoc`, `.txt`, `.org`;
- doc-named extensionless files such as `README`, `CONTRIBUTING`, and `CHANGELOG`, including conventional suffix variants;
- agent files: `CLAUDE.md`, `AGENTS.md`, `.cursorrules`, `llms.txt`;
- explicitly opted-in files through root policy.

Source files and configuration formats are dependency targets, not documents, unless explicitly opted in. Built-in excluded trees are `node_modules`, `vendor`, `third_party`, `dist`, `build`, minified assets, dependency lockfiles, and `assure.lock`. Tracked files take precedence over gitignore classification. Generated absent references are classified only after tree resolution fails.

Document bytes must be valid UTF-8 in v0. An invalid document is an unsupported/invalid-document finding, not silently skipped. Parser fallback to plain text is explicit in the report. Parser and block-projection versions participate in unit projection snapshots and therefore in migration rules.

This boundary is part of coverage semantics: a green result must state how many tracked candidate documents were scanned, excluded, unsupported, unlinked, and evaluated.

## Safe deferrals and reserved seams

The following features should not delay v0, but the schema choices above leave a clean seam for each.

### Cross-repository and external evidence

Reserve artifact repository IDs and scope tags, but do not perform network access in the PR checker. Future foreign and deployed scopes produce the same endpoint snapshots through a scheduled, unprivileged evaluator. TTL is an invalidation source and needs wall-clock metadata in that future evaluator; it is not part of core content identity.

### Strong reviewer identity and standalone audit

Git-backed current-state events are enough to dogfood. They are not enough for certification, detached exports, or cryptographic claims. A future app/service may append signed event envelopes using the same `AttestationEvent` payload. Do not market v0 as auditor-grade traceability.

### Multi-subject and partial acceptance

One document block to many evidence selectors covers the day-zero product. Cross-artifact equivalence can be represented later as a validator with multiple inputs. Partial endpoint acceptance is intentionally absent; if practical experience demands it, it needs a new attestation schema and UI, not an optional flag.

### Stable authored claim IDs

Content-addressed relationship instances implement trust-on-edit with zero authoring. They do not preserve logical lineage across meaningful edits. A future adjacent declaration may supply an explicit logical ID, but it should coexist with the immutable relationship-instance ID rather than replace it.

### Selector/plugin ecosystem

Built-in path existence, file content, path set, document block, and carefully selected symbol projections are sufficient to validate the state model. Arbitrary shell probes, plugin process protocols, external validators, and LLM findings can adopt a versioned `EvidenceResult` after the core JSON model stabilizes. No repository-controlled code executes in the v0 checker.

### Alternate digest algorithms and projection body store

Persisted SHA-256 is adequate. Algorithm agility is encoded but need not be exercised. Storing accepted projection bodies for history-free diffs should wait for the OP-29 size measurements and real acceptance-UX evidence.

## Recommended v0 cut

The smallest implementation that exercises the contract without prematurely implementing the growth model is:

1. Single repository, co-versioned workspace plus pinned local historical scope.
2. The C-12 document set with format-specific block extraction and plain-text fallback.
3. One subject block and one-or-more inferred local dependency selectors.
4. Path-existence and file-content projections; path-set only where already inferable from a named check.
5. Per-endpoint snapshots, atomic attestation seal, current-state JSONL lock.
6. Bootstrap, authored/edited, and accepted-unchanged event decisions.
7. Fact-vector evaluation, attributed base/candidate findings, and policy trace.
8. Human, deterministic JSON, and SARIF output with exit codes 0/1/2.
9. Read-only `check`, add/remove-only `refresh`, explicit `ok`, `verify-lock`, and schema-aware `migrate` commands.
10. No network, arbitrary probe execution, LLM, cross-repo access, silent retargeting, partial acceptance, or semantic truth claim.

Before calling this contract implemented, golden tests should prove:

- formatter-only block changes preserve unit identity where the declared block projection promises that property;
- a meaningful doc edit creates a new relationship instance and the old one is reported as retired before refresh;
- target-content changes preserve relationship ID and change only the relevant endpoint delta;
- a multi-endpoint attestation cannot update one accepted endpoint alone;
- refresh never changes an existing record;
- base/candidate attribution never labels a pre-existing finding as introduced;
- policy composition is monotone under the organization floor;
- migration never turns an already impacted relationship clean;
- canonical hashing matches published vectors across platforms;
- JSON and lock output are byte-identical across repeated runs and traversal orders;
- `verification-passed` and `review-required` can coexist in one result;
- a zero-link document is counted but creates no green relationship;
- a historical pinned selector never falls back to current workspace content.

## Final recommendation

Accept the dossier's high-level design, with three contract corrections:

1. Replace the single relationship-state enum with an orthogonal fact lattice and policy projection.
2. Replace the single combined baseline with per-endpoint accepted snapshots plus a combined seal.
3. Treat attestation, migration, refresh, and policy as explicit state transitions with separate authority, rather than as convenient ways to rewrite hashes.

Those corrections preserve the day-zero product while preventing its lockfile from becoming the next stale, ambiguous artifact in the repository. They also encode the most important lesson from every prior-art family: generated and executable evidence can prove narrow facts; content fingerprints can prove change; only an explicit, scoped event can say that somebody accepted the relationship between them.
