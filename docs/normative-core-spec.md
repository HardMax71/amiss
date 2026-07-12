# Normative core specification

Date: 2026-07-12.

Status: pre-implementation contract. This specification supersedes the identity, automatic
refresh, trust-on-edit, single-status, JSONL lock, and attribution-as-safety rules proposed in
[design.md](./design.md), [open-problems.md](./open-problems.md), and
[v0-contract-review.md](./v0-contract-review.md). Document-directive spelling is governed separately
by [directive-rfc.md](./directive-rfc.md); the discard-state scanner's now-published report/control
wire contract is governed separately by [machine-contracts.md](./machine-contracts.md).

The key words **MUST**, **MUST NOT**, **REQUIRED**, **SHOULD**, **SHOULD NOT**, and **MAY** are to be
interpreted as normative requirements. A capability marked unsupported is part of the contract:
an evaluator MUST report it as unsupported and MUST NOT derive a successful result from it.

## 1. Product boundary

The product has four evidence layers. They share artifact and selector primitives, but they do not
share identity, trust, or completion semantics.

| Layer | Persistent identity | Persisted state | What a successful result means | Initial disposition |
| --- | --- | --- | --- | --- |
| Structural reference | `ObservationId` | None | The explicitly named target resolves in the evaluated tree | Every current supported failure may fail unless exact active external debt/waiver applies |
| Impact observation | `ObservationId` | None | A selected projection changed, or a subject co-changed, between two trees | Advisory |
| Governed narrative claim | Explicit `ClaimId` | Reviewed claim record | An explicit acceptance covers exactly the current declaration and endpoint snapshots | Unsupported until a declaration adapter is enabled; then policy-controlled |
| Deterministic relation | Explicit `ClaimId` | Definition and, where needed, evidence record | The declared deterministic predicate passed in its stated environment | May fail only for supported hermetic validators |

These rules follow:

1. A structural or inferred observation MUST NOT be promoted to a governed claim merely by writing
   it into state.
2. Automatic extraction, initialization, co-change, and refresh MUST NOT create or advance an
   acceptance.
3. Content-derived identifiers MUST be used only for observations and immutable evidence
   snapshots. They MUST NOT identify governed claims.
4. A governed claim MUST have an explicitly authored stable `ClaimId`.
5. Deterministic validation and narrative acceptance are independent evidence axes. One MUST NOT
   clear or conceal the other.
6. The required check MUST evaluate the exact candidate tree that may merge. Attribution is
   diagnostic and MUST NOT excuse an invalid protected claim in that tree.
7. The only successful repository-wide summary is “no blocking findings in the evaluated scope.”
   The summary MUST also disclose its coverage and unsupported counts.

The first implementation SHOULD contain only the stateless structural and impact layers. The
governed layer may be enabled only after its declaration adapter, state transition verifier, and
adversarial tests in section 17 are complete. The contracts below exist now so that a later
governed pilot does not invent incompatible state.

## 2. Core type system

### 2.1 Repository paths

A `RepoPath` is a non-empty UTF-8 string with these properties:

- `/` is the only separator;
- it is relative to the repository root;
- it contains no empty, `.`, or `..` segment;
- it contains no NUL, backslash, leading slash, or trailing slash, and is already decoded rather
  than an encoded URI spelling; a literal `%` is a legal Git-path byte and is preserved;
- comparison is bytewise and case-sensitive on every platform;
- it names a path in a Git tree, not a host-filesystem spelling.

Input adapters MAY accept platform or URI spellings, but they MUST convert them to `RepoPath` or
return a typed parse error before resolution. They MUST NOT silently case-fold or Unicode-normalize
paths. A Git path outside the scanner `RepoPath` domain is `UNREPRESENTABLE_PATH`, incomplete exit
2; a future governed schema cannot downgrade it to an unsupported clean fact. Symlinks are entries,
not transparent redirects. A version 1 selector MUST NOT follow a symlink to obtain target content.

### 2.2 Governed identifiers

A `ClaimId` is an authored repository-global logical identifier. Its version 1 grammar is:

```text
[a-z][a-z0-9]*(?:[.-][a-z0-9]+)*
```

It is between 3 and 128 ASCII bytes. Examples are `docs.expr-precedence` and
`public-api.retry-policy`. A `ClaimId`:

- MUST be unique across every active declaration and tombstone;
- MUST remain unchanged when prose, headings, paths, selectors, or line numbers change;
- MUST NOT be generated from content, a location, an ordinal, or a Git object ID;
- MUST never be reused after retirement, split, or merge;
- is compared byte-for-byte and has no aliases.

`ClaimKey` is the layout-neutral storage-key digest
`HJ("assure/claim-key/v1", {"claim_id": ClaimId})`. It is not a second identity; every record's
embedded `ClaimId` must reproduce its logical map/database key. Only if X-06 selects a path-based
storage RFC must a physical pathname additionally reproduce that `ClaimKey`.

An `EndpointId` is unique within one claim and has the same grammar with a maximum of 64 bytes.
`subject` is reserved for the subject endpoint. Dependency endpoint IDs are stable handles such as
`grammar` or `cli-schema`. Retargeting an endpoint preserves its `EndpointId` and changes its
`SelectorId`, making the transition explicit.

### 2.3 Observation identifiers

An `ObservationId` is a reproducible diagnostic fingerprint for ungoverned extraction:

```text
HJ("assure/observation-id/v1", {
  "schema": "assure/scanner-observation-id-input/v1",
  "adapter_id": AdapterId,
  "adapter_contract_digest": Digest,
  "document": RepoPath,
  "source_construct": SourceConstruct,
  "structural_address": StructuralAddress,
  "source_projection_digest": Digest,
  "extracted_intent": CanonicalExtractedIntent
})
```

Line and column are excluded. Scanner v0's closed `StructuralAddress` and target-intent shapes are
published in [machine-contracts.md](./machine-contracts.md#adapter-observation-and-build-provenance);
another adapter requires a new input schema rather than an untyped JSON extension.
`source_projection_digest` makes a meaningful edit produce a new observation. Observation IDs are
therefore allowed to churn under edits, moves, splits, parser changes, or ambiguity.

An `ObservationId` MUST NOT be used as:

- a `ClaimId`;
- an acceptance target;
- a waiver target for a governed finding;
- ownership or lifecycle identity;
- proof that two observations across trees are the same logical claim.

Base-to-candidate observation correlation is a diagnostic matcher with outcomes `exact`,
`candidate`, `ambiguous`, and `none`. An unchanged-subject impact label may be derived only for an
`exact` pair or an unambiguous one-to-one `candidate` pair whose source-projection digests are
byte-equal under the same adapter contract. The latter exists for structural-address churn or an
exact document rename and remains an advisory correlation, never claim identity, acceptance, or
review proof. A `candidate` pair with unequal projections and every `ambiguous`/`none` result may
not produce an unchanged-subject label; all non-exact outcomes MUST expose their reason.

### 2.4 Content-addressed identifiers

`ArtifactId`, `SelectorId`, definition digests, snapshot digests, acceptance seals, record seals,
transaction IDs, and logical-ledger roots are content-addressed values under section 8. Content
addressing of these immutable values does not weaken the rule that `ClaimId` is authored and
stable.

## 3. Declaration boundary

### 3.1 Authored intent versus machine state

A governed `ClaimDefinition` is authored intent. It MUST be recoverable from a visible declaration
in the governed document plus any named check that declaration explicitly references. The state
directory MUST NOT be the only place containing any of these facts:

- `ClaimId` declaration;
- relation kind or completion mode;
- subject intent;
- dependency selector intent;
- scope;
- validator binding;
- pruned or expanded dependency membership intended by the author.

Native links and explicit paths are self-declaring ungoverned structural observations. Bare tokens,
co-change, similarity, symbol guesses, or model output are candidates only. Promoting one requires
an authored declaration; `accept` MUST NOT perform promotion.

The core is syntax-neutral. A future governed document adapter emits a semantic `ClaimDefinition` and an exact
source span. A declaration spelling MUST NOT be treated as supported until its adapter has a
versioned grammar and renderer/parser conformance suite. In particular, repeated unqualified
`[assure]:` reference definitions are invalid and MUST NOT be interpreted by proximity. A file
that appears to contain a known-but-disabled declaration syntax produces
`unsupported-declaration-syntax` only in that future governed ADT. Current scanner v0 instead uses
its sole representable rule: every reserved definition contributes
`unsupported-capability: governed-claim`, as scanner-v0 and directive-rfc specify.

### 3.2 Claim definition

The canonical semantic shape is:

```json
{
  "schema": "assure.claim/v1",
  "claim_id": "docs.expr-precedence",
  "relation": {"kind": "describes"},
  "scope": {"kind": "candidate-tree", "repository": "self"},
  "subject": {
    "endpoint_id": "subject",
    "selector": {}
  },
  "dependencies": [
    {"endpoint_id": "grammar", "selector": {}}
  ],
  "validators": []
}
```

The omitted selector objects are defined in section 4. The following are normative:

- `schema`, `claim_id`, `relation`, `scope`, `subject`, `dependencies`, and `validators` are
  required.
- Unknown fields are rejected for major version 1.
- There is exactly one subject.
- Dependency `EndpointId` values are unique and the canonical form sorts them bytewise.
- Most relation constructors require one or more dependencies; section 5 defines the exceptions.
- Validator IDs are unique and sorted in canonical form.
- Every endpoint artifact scope MUST exactly equal the claim scope in version 1. Endpoint-specific
  scopes and mixed-scope claims are unsupported.
- Source path, source span, headings, renderer URL, and inferred confidence are diagnostic metadata,
  not fields in this semantic object. The document path inside the subject's `ArtifactSpec` is
  semantic selector intent. Moving that artifact therefore changes the definition while preserving
  `ClaimId`.
- Named-check and validator references are expanded for hashing into sorted
  `{id, definition_digest}` bindings. `DefinitionDigest` is
  `HJ("assure/claim-definition/v1", {"claim": ClaimDefinition,
  "named_check_bindings": [...], "validator_bindings": [...]})`. A referenced root definition
  change therefore changes `DefinitionDigest`; an ID alone cannot hide executable or selector
  intent changes.
- Policy, owner resolution, current projections, acceptance, waiver, and attribution MUST NOT be
  embedded in a definition.

Two declarations with the same `ClaimId` are an invalid repository even if their bytes are equal.
A definition whose relation kind changes under an existing `ClaimId` is not a migration; the old
claim MUST be retired and a fresh `ClaimId` created.

## 4. Artifact, selector, and validator model

### 4.1 Artifact specification

An `ArtifactSpec` identifies a thing independently of current content:

| Field | Rule |
| --- | --- |
| `schema` | Literal `assure.artifact/v1` |
| `kind` | Closed value: `document`, `repository-file`, `repository-tree`, or `named-check` |
| `repository` | Literal `self` in version 1 |
| `locator` | Kind-specific canonical object |
| `scope` | A `ScopeSpec` from section 6 |

Document and file locators contain one `RepoPath`; tree locators contain one root `RepoPath`; a
named-check locator contains a root-config ID using the `EndpointId` grammar. `ArtifactId` is
`HJ("assure/artifact-id/v1", ArtifactSpec)`.

Current bytes, line numbers, headings, Git blob IDs, display URLs, resolution status, and parser
versions MUST NOT participate in artifact identity.

### 4.2 Selector specification

A `SelectorSpec` states the authored projection intent:

| Field | Rule |
| --- | --- |
| `schema` | Literal `assure.selector/v1` |
| `kind` | Closed selector kind |
| `selector_schema` | Positive safe integer |
| `artifact` | Complete `ArtifactSpec` |
| `parameters` | Strict canonical JSON object; unknown fields rejected per selector schema |
| `cardinality` | `exactly-one`, `one-or-more`, or `zero-or-more` |
| `projection` | Closed projection kind and positive projection schema |
| `path_semantics` | `identity` or `locator` |

`SelectorId` is `HJ("assure/selector-id/v1", SelectorSpec)`. Engine implementation identity is
excluded. Any change to selector kind, schema, artifact, parameters, cardinality, projection
contract, or path semantics changes `SelectorId` and is a governed retarget, not an engine
migration.

Version 1 reserves these selectors:

| Selector | Allowed artifact | Resolution/projection | Initial support |
| --- | --- | --- | --- |
| `document-region` | `document` | Exactly the region governed by the adjacent unique declaration; conservative text projection | Supported only by an enabled conformance-tested document adapter |
| `path-exists` | `repository-file` or `repository-tree` | Entry kind, canonical path, and mode; no target content | Supported |
| `file-content` | `repository-file` | Raw entry bytes and conservative text or binary projection | Supported for regular tracked UTF-8 files and raw binary bytes |
| `path-set` | `repository-tree` | Sorted matching paths and Git modes | Supported for bounded literal/glob parameters |
| `file-set-content` | `repository-tree` | Sorted path, mode, and content-projection tuples | Deferred |
| `text-region` | `repository-file` | Explicit marker-delimited bytes | Deferred |
| `captured-value` | `repository-file` or `named-check` | Typed scalar from a declarative parser/query | Deferred |
| `symbol-source` | `repository-file` or `repository-tree` | Qualified language symbol projection | Deferred |
| `public-shape` | `repository-file` or `repository-tree` | Versioned exported-interface projection | Deferred |
| `probe-output` | `named-check` | Hermetic evidence output | Deferred |

A deferred selector is a valid known declaration but resolves to `unsupported`. It MUST NOT fall
back to whole-file content, substring search, a similar path, or another selector. An unknown
selector kind makes the declaration schema unsupported and the evaluation incomplete.

`path_semantics = identity` means a path change intentionally breaks or retargets the selector.
Native references and `path-exists` use this value. `path_semantics = locator` permits an explicit
migration proposal, never an automatic retarget. Similarity and exact content are evidence shown
to a reviewer, not authority.

### 4.3 Resolution

Resolution is run output, not authored intent. Every endpoint produces:

- one status: `resolved`, `missing`, `ambiguous`, `unsupported`, or `error`;
- zero or more fully canonical resolved members;
- actual scope-resolution facts;
- selector contract and exact engine implementation identity;
- a projection and raw digest when resolution succeeded;
- a bounded summary and an explicit truncation flag for display only.

Cardinality is checked after matching. A cardinality violation is `ambiguous` when too many members
were found and `missing` when too few were found. A selector for an intentionally empty set MUST
declare `zero-or-more`; an empty `one-or-more` selection is missing.

`error` means the evaluator failed while evaluating a supported selector. It is not a policy
finding that may become clean; it makes the run incomplete. `unsupported` means evaluation
completed and established that the installed capability cannot evaluate the known selector.

### 4.4 Validator specification and provenance

A `ValidatorSpec` contains a stable validator ID, a closed kind, complete input `EndpointId` set,
an output contract, engine digest, environment contract, network/secrets policy, resource limits,
and cost class. Its complete canonical definition digest is bound into `DefinitionDigest` as
specified in section 3.2.

The initial protected process supports only built-in, non-executing `projection-equality` and
`existence` validators. Arbitrary commands, repository plugins, generators, browser workflows,
service probes, and imported executable results are deferred. A deferred validator yields
`validation = unsupported`; the relation is not complete.

A future `hermetic-regeneration` validator may claim complete declared-input reproducibility only
if undeclared filesystem inputs and network access were technically unavailable. Otherwise its
result MUST be named `observed-invocation-reproducibility`, not complete derivation. Validation
results retain engine, executable, environment, input, network, secrets, timeout, and output
digests. A pass from one environment MUST NOT be reused under another environment contract.

## 5. Closed relation algebra

Relation kind is a closed algebraic data type. Authority, invalidation, legal arity, cycle rules,
and completion are derived from its constructor; arbitrary combinations are not stored.

| Kind | Authority | Invalidation | Required shape | Completion | Initial support |
| --- | --- | --- | --- | --- | --- |
| `reference` | Subject names a target | Declaration/target resolution | One document subject; one or more existence dependencies | Every required endpoint resolves | Supported for native observations; governed form after adapter enablement |
| `describes` | Dependencies are evidence for prose | Subject, dependency projection, selector, scope, or definition change | One document subject; one or more dependencies | A complete current explicit acceptance | Governed beta |
| `generated-from` | Dependencies and generator govern output | Any input, generator, environment, or output change | One output subject; one or more dependencies; one hermetic-regeneration validator | Current hermetic regeneration/equality passes | Deferred |
| `constrains` | Subject is normative | Subject or implementation change | One normative subject; one or more implementation dependencies; explicit `completion = validator` or `completion = acceptance` | Named conformance validator passes, or a complete implementation acceptance is current, according to the constructor | Deferred unless its chosen built-in validator is supported |
| `equivalent` | Joint | Any endpoint change | One distinguished reporting subject; one or more peers; deterministic two-input validator | Current validator passes | Supported only for built-in projection equality |
| `historical-at` | Immutable historical scope | Subject/declaration change; not current-tree change | One document subject; one or more dependencies in one immutable scope | Pinned scope resolves and an explicit acceptance is current | Deferred while immutable scope is unsupported |

Additional rules:

- `describes` acceptance MUST snapshot the subject and every dependency. Editing the subject is not
  completion; it invalidates the old acceptance.
- A validator pass MAY coexist with `describes = review-required` and MUST NOT discharge it.
- `constrains(completion = acceptance)` records an implementation-side acceptance with the same
  atomic snapshot rules as `describes`; it does not pretend to be a formal proof.
- `reference` does not react to target content changes because its projection is existence.
- `historical-at` requires an immutable revision. It MUST NOT fall back to `candidate-tree`.
- Only `generated-from` derivation edges are required to form a DAG. A cycle among them is
  `forbidden-cycle`. `equivalent`, mutual `constrains`, `reference`, and `describes` cycles are
  represented as strongly connected components for reporting and are not schema errors merely
  because they are cycles.
- Unknown relation constructors make the declaration schema unsupported and the run incomplete.

## 6. Scope model

`ScopeSpec` is a closed tagged union. The exact supported candidate shape is
`{"kind":"candidate-tree","repository":"self"}`; unknown or omitted fields are invalid:

| Kind | Required identity | Version 1 behavior |
| --- | --- | --- |
| `candidate-tree` | Repository `self`; exact candidate Git tree supplied by run context | Supported and the only default |
| `immutable-revision` | Repository ID plus full immutable commit/tree object ID | Recognized but deferred; `unsupported-scope` if selected |
| `release-line` | Protected mapping ID plus resolved immutable revision | Deferred; never resolve a mutable branch implicitly |
| `environment-observation` | Named probe, immutable result digest, environment digest, observation expiry | Deferred and scheduled only |
| `external-observation` | Named source, immutable result digest, source digest, observation expiry | Deferred and scheduled only |

`ScopeDigest` is `HJ("assure/scope/v1", ScopeSpec)`. The actual base and candidate Git object IDs are
run provenance and are included in reports. They are not acceptance validity keys and an
acceptance MUST NOT name the commit that contains its own state update. Validity is bound to the
scope contract and the complete endpoint snapshots.

Version 1 governed claims MUST use `candidate-tree`. Conventional `versioned`, `releases`,
`historical`, `archive`, `proposals`, or locale trees without an explicit supported scope produce
`scope-unresolved`. Inferred impact for them is advisory and MUST NOT compare them to current code.
Structural reference checking may still run within their evaluated tree.

Changing scope under an existing `ClaimId` is an explicit `migrate` transition and invalidates the
acceptance. Changing a live scope to a weaker or deferred scope also emits the unsuppressible
`scope-weakened` meta-finding. A mutable tag, branch, `latest`, or wall-clock time MUST NOT be used as
a content identity.

## 7. Endpoint snapshots and atomic acceptance

### 7.1 Projection snapshot

An accepted `EndpointSnapshot` contains exactly these validity fields:

| Field | Rule |
| --- | --- |
| `schema` | Literal `assure.endpoint-snapshot/v1` |
| `endpoint_id` | Current stable endpoint handle |
| `selector_id` | Recomputed from authored selector intent |
| `resolution_digest` | Digest of the complete sorted resolved-member identities and modes |
| `projection_digest` | Digest of canonical selected evidence |
| `raw_digest` | Digest of complete raw selected evidence; required for a governed acceptance |
| `scope_digest` | Current declared scope digest |
| `selector_schema` | Authored selector semantics version |
| `projection_schema` | Canonical projection semantics version |
| `engine_contract` | Engine ID and semantic contract version |
| `engine_implementation` | Exact binary/component digest for forensics |

Only `resolved` endpoints may be snapshotted. Resolution status itself is therefore not stored as a
successful snapshot. Counts, selected-member excerpts, source spans, old/new snippets, and
truncation are report metadata and MUST NOT affect equality.

`ResolutionDigest` is `HJ("assure/selector-resolution/v1", sorted complete member identities)`.
For one regular file, `raw_digest` is `HB("assure/raw-evidence/v1", file bytes)`. For a set, each
member first receives that byte digest, then the aggregate raw digest is
`HJ("assure/raw-evidence-set/v1", sorted [{path, mode, member_raw_digest}])`. An
`EndpointSnapshotDigest` is `HJ("assure/endpoint-snapshot/v1", EndpointSnapshot)`. The acceptance
embeds the complete snapshots, not only their digests, so their endpoint mapping remains
diagnosable.

For a singleton regular file, `raw_digest` deliberately covers only that file's bytes; its path and
Git mode are already covered by `resolution_digest`. For a multi-member selector, the aggregate
`raw_digest` additionally covers every member path and mode so member boundaries cannot be
repartitioned without changing the digest. In both cases the pair
`(resolution_digest, raw_digest)` covers the complete selected paths, modes, and bytes. The raw
digest exists so an engine migration can prove that evidence bytes did not change even when the old
projection was lossy. A selector that cannot produce that complete pair is ineligible for governed
acceptance in version 1.

### 7.2 Acceptance event

An `AcceptanceEvent` contains:

- schema `assure.acceptance/v1`;
- `ClaimId`;
- current `DefinitionDigest` and `ScopeDigest`;
- exactly one subject snapshot;
- one snapshot for every dependency endpoint, sorted by `EndpointId`;
- the complete sorted validator-definition digest set;
- decision `reviewed-new`, `reviewed-updated`, or `reviewed-unchanged`;
- a non-empty reason of at most 1,000 UTF-8 bytes after rejecting all-control-character input;
- `predecessor_acceptance_seal`, absent only for the first acceptance;
- provenance and trust from section 7.3.

`AcceptanceSeal` is `HJ("assure/acceptance-seal/v1", AcceptanceEvent without any field named
acceptance_seal)`. The seal binds the declaration and the complete endpoint set atomically.

Partial endpoint acceptance is prohibited. Version 1 accepts exactly one claim per `accept`
invocation. Bulk or multi-claim acceptance is unsupported because it creates a rubber-stamp and
cross-record transaction surface without strengthening review. Added, removed, or changed endpoint
intent changes `DefinitionDigest`; an older acceptance cannot match it. If any endpoint cannot be
resolved, snapshotted, or validated under command preconditions, the command writes no record.

A digest is sufficient for equality but cannot reconstruct an old diff. Every result reports
`baseline_diff_availability` as `available`, `history-required`, or `unavailable`. The UI MUST NOT
say that a reviewer saw a diff when it was unavailable. Storing projection bodies is outside
version 1.

### 7.3 Acceptance provenance and trust

Provenance answers how the transition occurred. Trust answers what can authenticate it.

| Provenance | Meaning | May create a narrative acceptance? |
| --- | --- | --- |
| `explicit-review` | A user explicitly ran `accept` over the displayed current snapshots | Yes |
| `mechanical-engine-migration` | A supported equivalence-preserving representation migration ran | It may carry an existing acceptance forward under section 11; it cannot create the first one |

Automatic extraction, init, scan, and co-change have observation provenance `automatic`. That is
an `ObservationProvenance`, not an acceptance provenance or trust level, and the value is forbidden
in `AcceptanceEvent`.

| Trust | Required evidence | Contract meaning |
| --- | --- | --- |
| `self-asserted` | Canonically valid local event | The repository contains a structurally valid assertion; actor identity and attention are not proven |
| `provider-verified` | A verifiable immutable provider receipt over the exact committed `AcceptanceSeal`, the acceptance commit/candidate that introduced it, and the eligible reviewer rule | The configured provider verifier authenticated review of that committed acceptance |
| `service-signed` | A service signature over that provider-verified receipt and exact committed `AcceptanceSeal` | The configured service authenticated the receipt; it did not create a second acceptance |

The local CLI creates only `self-asserted` events. It MUST NOT store an authoritative actor or
timestamp. An optional actor hint is report metadata and MUST be labeled untrusted. Git authorship
and review provide repository context, not cryptographic proof of who understood the claim.

`repository-reviewed` is a review-context fact meaning that provider branch policy required
ordinary repository review. It is not an acceptance trust class and does not prove that eligible
document and evidence owners reviewed this claim. Review context therefore cannot upgrade
`self-asserted` to `provider-verified`.

The committed `AcceptanceEvent` records `self-asserted` as its base trust. `provider-verified` and
`service-signed` are derived run facts only from a separately supplied receipt. If such a receipt
claims either level and the configured verifier cannot verify it, effective trust is `unverified`,
a `trust-evidence-invalid` finding is emitted, and the acceptance cannot satisfy that trust
requirement. Unknown trust kinds are schema errors. Trust policy does not change whether endpoint
snapshots are structurally current.

A governed narrative claim is eligible for a blocking profile only when its current acceptance has
at least `provider-verified` trust. A `service-signed` receipt qualifies only when it authenticates
the same provider-verified review facts. A current `self-asserted` acceptance, with or without
`repository-reviewed` context, may be reported and piloted but MUST NOT satisfy a blocking
narrative gate.

A provider or service receipt is a trust overlay on one exact committed `AcceptanceSeal`. It does
not replace the repository `AcceptanceEvent`, change `previous_record_seal`, advance
`predecessor_acceptance_seal`, or create a second CAS chain.

The receipt's candidate is the immutable **acceptance candidate**: the commit/tree in which that
exact seal entered protected history. It is not the later **evaluation candidate** whose endpoints
are being checked. On every later run, the evaluator separately binds the current
repository/ref/evaluation candidate and re-derives section 12.2. A valid receipt may therefore
continue to authenticate an unchanged, still-current acceptance across unrelated commits. It is
invalidated when the seal, claim definition, endpoint set, accepted projections, eligible-reviewer
rule, provider protection context, or receipt validity changes; unrelated candidate identity alone
does not invalidate it. A receipt that instead claims to approve the current evaluation candidate
must use a different, future final-tree authorization contract and cannot be interpreted as this
acceptance receipt.

Initialization creates active, unattested records only. Co-change may be reported as
`subject-cochanged`; it is not an acceptance provenance. There is no `fresh-by-construction`,
`authored-or-edited`, or implicit-review acceptance state.

## 8. Canonical encoding and hashing

### 8.1 Digest primitives

All persisted IDs and seals use SHA-256. Fast non-cryptographic hashes MAY key a local cache but
MUST NOT appear in persisted state, reports as evidence IDs, CAS tokens, or signatures.

Two domain-separated primitives exist:

```text
HJ(domain, value) = SHA-256(UTF-8(domain) || 0x00 || JCS(value))
HB(domain, bytes) = SHA-256(UTF-8(domain) || 0x00 || bytes)
```

Domains are lowercase printable ASCII with no NUL. Serialized digests are `sha256:` followed by 64
lowercase hexadecimal characters. Digests are never truncated in state or machine output.

`JCS` is RFC 8785 canonical JSON with verified errata, restricted further:

- duplicate object keys, invalid UTF-8, lone surrogate escapes, and a leading BOM are rejected;
- only integer tokens in `[-9007199254740991, 9007199254740991]` are allowed;
- `-0`, decimal fractions, and exponent notation are rejected;
- schemas SHOULD use strings or bounded unsigned integers rather than generic numbers;
- set-shaped arrays have a field-specific canonical key, are sorted by that key's canonical UTF-8
  bytes, and reject duplicates;
- strings are not NFC/NFD normalized; JSON escapes are decoded and the resulting scalar sequence is
  preserved;
- unknown fields are rejected for persisted major version 1.

Canonical repository text projection converts CRLF and bare CR to LF, then preserves every other
UTF-8 code point, whitespace byte, punctuation mark, operator, digit, BOM code point, ordering, and
final-newline presence. It performs no token, word, case, formatting, or Unicode normalization.
Binary and raw projections preserve all bytes. Selector-specific stronger projections must have a
new named projection schema and retain `raw_digest`.

Path-set projection includes path and Git mode. A symlink member includes mode and link-target bytes
without dereferencing. An executable-bit change therefore changes a path-set when modes are part of
that selector. Submodule and Git LFS content are unsupported unless the selector explicitly projects
only the repository entry identity.

### 8.2 Required domains

Version 1 implementations MUST use at least these exact domains:

| Value | Domain |
| --- | --- |
| Claim storage key | `assure/claim-key/v1` |
| Observation ID | `assure/observation-id/v1` |
| Artifact ID | `assure/artifact-id/v1` |
| Selector ID | `assure/selector-id/v1` |
| Scope digest | `assure/scope/v1` |
| Claim definition | `assure/claim-definition/v1` |
| Selector resolution | `assure/selector-resolution/v1` |
| Raw selected bytes | `assure/raw-evidence/v1` |
| Raw selected set | `assure/raw-evidence-set/v1` |
| Selector projection | A selector-specific `assure/<kind>-projection/v1` domain |
| Endpoint snapshot | `assure/endpoint-snapshot/v1` |
| Acceptance seal | `assure/acceptance-seal/v1` |
| Claim record seal | `assure/claim-record/v1` |
| Lifecycle transaction | `assure/lifecycle-transaction/v1` |
| Logical ledger root | `assure/logical-ledger-root/v1` |

An implementation MUST NOT hash a structure whose schema or domain is implicit.

### 8.3 Normative seed vectors

These seed vectors include the `sha256:` serialization prefix in expected output:

| ID | Primitive and input | Expected digest |
| --- | --- | --- |
| `GV-001` | `HJ("assure/claim-key/v1", {"claim_id":"docs.expr-precedence"})` | `sha256:f6a22f480cab9ed6e0fc82bcbe67eba85d88f10103f5107008809dec44fb71b0` |
| `GV-002` | `HJ("assure/path-set-projection/v1", {"members":[]})` | `sha256:6765a67e22b2efbaaf89509cd34a70682613f002cd82d0ff4e08332e26b76954` |
| `GV-003` | `HJ("assure/test-json/v1", {"z":"é","a":1})`; JCS bytes are `{"a":1,"z":"é"}` | `sha256:1a2aab8858a444002cd16e1fa53cc33fd12e5e6ac4568f85e06bef971a28425d` |
| `GV-004` | `HB("assure/text-projection/v1", UTF-8("a\nb\n"))` | `sha256:bab154d44fb1340ee8c20af6a1e36b9a903a5e44c584f8ce524237f0289b88c6` |
| `GV-005` | `HB("assure/raw-bytes/v1", empty bytes)` | `sha256:28031daa5fbb3a297dc947195957fe4a05c1bd2e58c56163013ee62be9368fac` |

Before a state schema is declared stable, one published cross-platform golden suite MUST also cover:

- CRLF and bare-CR normalization versus raw-digest inequality;
- composed and decomposed Unicode remaining distinct;
- non-ASCII object-key ordering under RFC 8785;
- final-newline and trailing-whitespace differences;
- path case, executable mode, symlink target bytes, and an empty path set;
- dependency and record traversal-order independence;
- duplicate key, duplicate set member, unsafe integer, float, invalid UTF-8, and invalid path
  rejection;
- multi-endpoint acceptance ordering and one changed endpoint;
- tombstone and split/merge transaction seals;
- same-contract engine upgrade and projection-schema migration.

Every supported implementation language MUST consume the same vectors. A release that disagrees
with a stable vector is incompatible and MUST use a new schema/domain rather than rewrite state.

## 9. Logical state and physical repository storage

### 9.1 Separation of contracts

The logical ledger is a map from every ever-governed `ClaimId` to one current `ClaimRecord`.
Physical layout is versioned separately. Logical IDs, seals, transition validation, and JSON reports
MUST NOT depend on file grouping, directory sharding, traversal order, or an implementation cache.

The logical state contract in this section is normative for a future governed experiment. No
physical layout is selected. The per-claim shape below is one X-06 comparison candidate alongside
global, bucketed, subtree, and external layouts; it is not a released compatibility contract or an
authorization to write repository state. X-06 itself remains closed until X-08 produces a positive
durable-obligation decision. A disposable harness may model this layout only in isolated fixtures
and MUST NOT claim storage version 1 stability. X-06 may select, revise, or reject it without
changing logical IDs, seals, or transitions.

The per-claim candidate would use one record per claim:

```text
.assure/state/meta.json
.assure/state/claims/<first-two-hex>/<remaining-62-hex>.json
```

If X-06 tests this candidate, the 64 hex characters are the unprefixed `ClaimKey`; the first two
form the shard directory. Each claim file contains one strict JCS object followed by one LF, and
the embedded `ClaimId` and `ClaimKey` reproduce the path. Those are candidate-specific test laws,
not current repository-write requirements. Tombstone placement is likewise conditional on this
candidate.

No current storage version exists. A writable monolithic `assure.lock` remains rejected for the
authorized scanner, while X-06 may measure it only as a comparison baseline. Any future selected
layout must define multiple-source detection and explicit migration in its own storage RFC.

`meta.json` contains only rarely changing storage schema, record schema, digest contract, and
minimum reader major. It MUST NOT contain a generated timestamp, host path, current commit, mutable
claim count, or persisted global ledger root. Avoiding a global line for ordinary acceptance is a
contract goal: unrelated claims should merge independently, while two updates to one claim should
conflict exactly where claim-level CAS also conflicts.

The report MAY compute
`LogicalLedgerRoot = HJ("assure/logical-ledger-root/v1", sorted [{claim_id, record_seal}])`; it MUST
NOT write that root back during `check` or ordinary claim updates.

A canonical JSONL export MAY be produced for transport, sorted by `ClaimId`, but it is not the
repository state format and MUST NOT be accepted as a second writable source of truth.

### 9.2 State boundary

State stores only:

- current lifecycle status and transition envelope;
- current `DefinitionDigest` and `ScopeDigest`;
- current acceptance event, if any;
- predecessor record seal;
- predecessor/successor lineage required by lifecycle;
- permanent tombstone data;
- record seal.

State MUST NOT be the sole source of selector intent, relation semantics, scope intent, named-check
definition, policy, ownership, waiver, or inferred relationships. It does not store a global
reverse index or cache. Definitions remain in documents/root named checks; policy and ownership
remain in their protected authored surfaces; waivers are separate governed authorizations.

`verify-state` proves canonical shape, derived digests, internal transaction closure, and a supplied
base-to-candidate transition. It does not prove human attention, actor identity, or truth.

### 9.3 Claim records

An active or retirement-requested record contains:

- schema `assure.claim-record/v1`;
- `record_kind = claim`;
- `ClaimId` and `ClaimKey`;
- lifecycle `active` or `retirement-requested`;
- current `DefinitionDigest` and `ScopeDigest`;
- optional current `AcceptanceEvent` and its seal;
- `lineage.predecessors`, empty except for a new split/merge successor;
- transition kind, reason, and optional lifecycle transaction ID;
- `previous_record_seal`, absent only at creation;
- `record_seal = HJ("assure/claim-record/v1", all preceding validity fields)`.

A tombstone contains the same identity and predecessor fields plus:

- `record_kind = tombstone` and lifecycle `retired`;
- terminal kind `retired`, `split`, or `merged`;
- final prior active record seal;
- sorted successor `ClaimId` list;
- reason and lifecycle transaction ID where applicable;
- no active acceptance.

If X-06 selects a repository-file layout, current-state files would retain only the current record
and Git history would be the ordinary prior-event store; a shallow checkout could then verify
current state and a supplied base transition but not reconstruct arbitrary history. Database,
external, or other selected layouts must publish their own retention/audit law. None is normative
before the storage RFC.

## 10. Lifecycle and concurrency

### 10.1 Legal transitions

All governed lifecycle changes are explicit. Garbage collection and inferred lineage are forbidden.

| From | Operation | Preconditions | To | Acceptance consequence |
| --- | --- | --- | --- | --- |
| Absent | `create` | Authored unique definition; ID never appears in any tombstone | `active` | `absent` for acceptance-capable relations; `not-applicable` otherwise |
| `active` or `retirement-requested` | `accept` | Relation completion uses acceptance; exact expected record seal; current definition and all endpoints resolvable | Same lifecycle state | New complete acceptance |
| `active` | `migrate` | Same `ClaimId`; relation kind unchanged; exact expected seal; explicit new definition and reason | `active` | Old acceptance retained as the prior baseline but is review-required |
| `active` | `request-retire` | Declaration remains; exact expected seal; reason | `retirement-requested` | Preserved but claim remains fully evaluated |
| `retirement-requested` | `cancel-retire` | Declaration remains; exact expected seal; reason | `active` | Preserved if still current |
| `retirement-requested` | `retire` | Request existed in the base state; declaration removed; exact expected seal; reason/approval evidence as policy requires | Tombstone `retired` | Removed permanently |
| `active` | `split` | One predecessor, at least two fresh successor IDs and definitions, exact expected seal, complete transaction | Predecessor tombstone `split`; successors `active` | No inherited acceptance; each successor is unattested |
| `active` set | `merge` | At least two predecessors, exactly one fresh successor ID and definition, every expected seal, complete transaction | Predecessor tombstones `merged`; successor `active` | No inherited acceptance; successor is unattested |
| `active` | `migrate-engine` | Section 11 equivalence conditions; exact expected seal | `active` | Existing explicit acceptance carried by a mechanical migration event |
| Tombstone | Any mutation | Never legal | Tombstone | ID reuse/mutation fails |

Additional lifecycle rules:

- Deleting an active declaration or document without a valid transition emits
  `governed-claim-removed` and cannot make the candidate healthier.
- A retirement request is not retirement. The declaration and normal evaluation remain active
  until a later candidate whose base already contained the request creates the tombstone.
- Split successor count is at least two. Merge predecessor count is at least two and the successor
  MUST use a fresh ID; selecting a predecessor ID as the merged concept is forbidden.
- Split and merge successors name all predecessors, and every predecessor tombstone names the same
  successor set. The transaction is valid only as a closed set.
- A move, locator change, selector retarget, dependency add/remove, validator change, or scope
  change uses `migrate` under the same `ClaimId` and invalidates acceptance. Relation-kind change
  uses retirement plus creation.
- Exact bytes, Git rename detection, similarity, or symbol tracking may propose a migration. None
  may write one automatically.
- Acceptance-capable version 1 constructors are `describes`, `historical-at`, and
  `constrains(completion = acceptance)`. `reference`, `generated-from`, `equivalent`, and
  `constrains(completion = validator)` reject `accept` with `ACCEPTANCE_NOT_APPLICABLE`, exit 2,
  and no write. Recognized-but-deferred relations remain unsupported before this precondition.
- Tombstones are permanent, included in logical roots and exports, and never compacted without a
  new major storage/audit contract.

### 10.2 Compare-and-swap

Every record mutation MUST name the full expected `previous_record_seal`. Creation expects absence
and proves the `ClaimId` is absent from both active records and tombstones. A mismatch is
`CAS_CONFLICT`; the command writes nothing and exits 2.

An acceptance event also names `predecessor_acceptance_seal`. Replacing only the record predecessor
after a rebase is not sufficient: the acceptance must be recomputed against the rebased candidate
and explicitly reissued. Base-to-candidate CI verifies both chains.

Two pull requests accepting the same predecessor are expected to conflict. After one merges, the
other candidate's predecessor no longer matches and its exact final-tree check fails. Changes to
unrelated ClaimIds do not share logical CAS. Whether they share a physical record, file, database
transaction, or merge-conflict surface is deliberately left to X-06 and its future storage RFC.

Multi-record operations have a deterministic `LifecycleTransactionId` over operation kind, every
input `ClaimId` and expected seal, every output `ClaimId` and definition digest, and reason. Every
member record names the same transaction and complete member set. Missing, extra, or inconsistent
members make the logical state invalid and exit 2.

Physical crash consistency is conditional on the layout X-06 eventually selects. If it selects the
per-claim filesystem candidate, that storage RFC must define a state-directory lock, same-directory
temporary writes, fsync order, atomic replacement, multi-record recovery, and fail-closed partial
transactions. A database, monolith, external service, or other layout must publish its own atomicity
and recovery law. None of those writer operations or a `recover-state` command is authorized now.

## 11. Selector-engine compatibility and migration

Every endpoint distinguishes:

1. authored `selector_schema` and parameters;
2. `projection_schema`, which defines canonical evidence meaning;
3. engine contract version, which promises implementation compatibility;
4. exact engine implementation digest, which is forensic provenance.

The transition rules are:

| Change | Comparison result | Required behavior |
| --- | --- | --- |
| Implementation digest changes; selector and projection contracts unchanged | Current projection digest equals accepted digest | Comparable and acceptance may remain current; report implementation change as provenance only |
| Implementation digest changes; contracts unchanged | Projection differs | Ordinary endpoint change under the promised contract; a tool bug is a release-integrity issue, not an automatic rebaseline |
| Selector intent/schema/parameters change | New `SelectorId` | Explicit claim `migrate`, then explicit acceptance |
| Projection schema or engine semantic contract changes | Old/new incomparable by default | `engine-migration-required`; relation is not complete |
| Projection schema changes; accepted raw digest equals current raw digest for every endpoint and old acceptance was current | Exact evidence unchanged | Explicit `migrate-engine` may carry acceptance mechanically |
| Projection schema changes; a reviewed total compatibility function proves equivalence and old evaluator reproduces the accepted old digest | Proven equivalent | Explicit `migrate-engine` may carry acceptance mechanically |
| Old accepted endpoint was already changed, raw digest differs, old evaluator is absent, or equivalence proof fails | Not safe | Human re-acceptance under new definition is required; migration MUST NOT turn it current |

`migrate-engine` creates a new acceptance seal with provenance
`mechanical-engine-migration`, retains a reference to the original explicit acceptance, names old
and new contracts, and advances record CAS. It cannot create an acceptance for an unattested claim.
It is an explicit state change and MUST NOT run inside read-only `check` or any automatic process.

A release that changes persisted projection semantics MUST retain the old evaluator or a reviewed
migration function for the published compatibility window. If neither is available, affected
claims are `migration-required`/`unsupported`, never clean. The report cause is engine migration,
not documentation change.

## 12. Orthogonal facts and derivation

### 12.1 Fact axes

The evaluator computes facts before policy. It MUST NOT implement a single relationship-health
enum.

| Axis | Values |
| --- | --- |
| Declaration | `valid`, `missing`, `duplicate`, `invalid`, `unsupported-schema` |
| Lifecycle | `untracked`, `active`, `retirement-requested`, `retired`, `invalid-transition` |
| Resolution per endpoint | `resolved`, `missing`, `ambiguous`, `unsupported`, `error` |
| Scope | `resolved`, `unresolved`, `unsupported`, `mismatch`, `error` |
| Snapshot comparison per endpoint | `no-acceptance`, `equal`, `changed`, `selector-changed`, `incomparable-engine`, `unknown` |
| Acceptance | `not-applicable`, `absent`, `current`, `review-required`, `invalid` |
| Acceptance provenance | `none`, `explicit-review`, `mechanical-engine-migration` |
| Trust | `none`, `self-asserted`, `provider-verified`, `service-signed`, `unverified` |
| Review context | `none`, `repository-reviewed`; never an acceptance trust class |
| Validation per validator | `not-configured`, `not-run`, `passed`, `failed`, `unsupported`, `skipped`, `expired`, `error` |
| Waiver | `absent`, `current`, `expired`, `invalid` |
| Attribution | `introduced`, `worsened`, `pre-existing`, `improved`, `resolved`, `unknown`, `not-applicable` |
| Coverage | Counts and owned-inventory membership; never a health label |
| Disposition | `record`, `warn`, `fail`; derived last and never persisted as evidence |

`broken`, `changed`, `waived`, `verification-passed`, and `review-required` may coexist. Output MUST
preserve the complete facts even if the human view selects one headline.

### 12.2 Acceptance derivation

For a relation whose constructor requires acceptance, derive `acceptance = current` if and only if:

1. lifecycle is `active` or `retirement-requested`;
2. an internally valid acceptance exists;
3. its `ClaimId`, `DefinitionDigest`, and `ScopeDigest` equal the current authored values;
4. endpoint IDs and selector IDs exactly equal the current complete endpoint set;
5. every required endpoint resolves;
6. every current projection, resolution, and scope digest equals its accepted snapshot under a
   comparable engine contract;
7. no engine migration is required;
8. the acceptance predecessor and record transition are valid.

If no acceptance exists, derive `absent`. If a valid prior acceptance exists but any item 3–7 is
false, derive `review-required`. Structural corruption or an invalid transition derives `invalid`.
Trust sufficiency and review context are separate policy facts and MUST NOT rewrite `current` to
imply a stronger identity proof. A structurally current self-asserted event remains
`acceptance = current` while also carrying `trust-insufficient-for-blocking`.

A changed `raw_digest` with an equal semantic `projection_digest` is retained as a `raw-changed`
forensic fact but does not by itself invalidate acceptance. That is the point of a versioned
projection that deliberately ignores formatting or another non-semantic representation detail.

A subject edit that changes its projection produces `review-required`; it never produces `current`
or a new ClaimId. A raw-only edit whose versioned projection remains equal produces `raw-changed`
without invalidating acceptance. A dependency revert that restores every accepted projection may make
the acceptance current again, because equality rather than chronology is the validity primitive.

### 12.3 Relation completion

Completion is derived after all facts:

- `reference` completes only when all required target endpoints resolve.
- `describes` evidence completes only when acceptance is current and policy-required trust is
  sufficient. Blocking completion always requires at least `provider-verified`; report-only
  evaluation may retain a current self-asserted acceptance with insufficient blocking trust.
- `generated-from` completes only when its required hermetic validator is current and passed.
- `constrains` follows its declared completion constructor; a validator and an acceptance are never
  silently substituted for one another.
- `equivalent` completes only when its declared deterministic validator is current and passed.
- `historical-at` completes only when immutable scope resolves and acceptance is current.

Whenever a blocking completion path depends on narrative acceptance, including `describes`,
`constrains(completion = acceptance)`, and `historical-at`, it also requires at least
`provider-verified` trust. The structurally current acceptance remains visible when trust is
insufficient.

Any required `missing`, `ambiguous`, `unsupported`, `error`, `expired`, `skipped`, `not-run`,
`migration-required`, or invalid fact prevents completion. `verification-passed` on one validator
does not hide another fact. A known unsupported capability produces a complete run with an
unsupported finding only when it is an explicitly disclosed boundary outside promised coverage.
When the command, built-in scope, repository policy, or external floor requests that capability as
covered, the run is incomplete and exits 2. An evaluator crash or unreadable required input is also
incomplete and exit 2.

### 12.4 Findings, policy, ownership, and waivers

Facts are classified into stable finding kinds. Repository policy maps them through the ordered
lattice `record < warn < fail`. Policy cannot change facts, coverage denominators, resolution, or
acceptance provenance.

Scanner v0 already publishes `policy-weakened`, `coverage-reduced`, and `control-plane-changed` as
unsuppressible control-plane kinds; their exact constructors are in machine-contracts. The
following remaining names belong only to the future governed-claim ADT. Once a governed engine and
its own wire schema are authorized, its applicable meta-findings are always emitted and cannot be
suppressed by repository policy or a claim waiver:

- `config-weakened`;
- `ownership-reduced`;
- `debt-added`;
- `debt-weakened`;
- `waiver-added`;
- `waiver-weakened`;
- `governed-claim-removed`;
- `scope-weakened`;
- `validator-changed`;
- `acceptance-transition-invalid`;
- `lifecycle-transition-invalid`;
- `claim-id-reused`;
- `engine-migration-required`;
- `state-corrupt`.

Every listed security/control-plane meta-finding has built-in disposition `fail` in a protected
governed blocking profile. Scanner v0 emits only the closed control subset published by its report
schema and machine contracts; it MUST NOT synthesize these future names. Repository policy, debt,
local flags, and waivers cannot
lower it. A report-only deployment may surface the same failed fact without registering the job as
a required merge gate; it MUST NOT relabel the fact as `warn`, clean, or successful. Structural
state corruption and analysis-integrity failures remain exit 2 rather than ordinary policy
failures. An intentional weakening requires an external authorization bound to exact base and
candidate policy digests. Repository policy cannot authorize its own downgrade.

Base and candidate configuration are evaluated over the union of base and candidate tracked paths,
claims, scopes, validators, and all closed finding kinds. A lower disposition, new exclusion,
removed protected path, weaker owner/trust requirement, broader waiver, live-to-historical scope
change, claim deletion, or validator weakening emits the corresponding meta-finding. A syntactically
new lowering rule or exclusion is conservatively a weakening even if it currently matches no path,
unless the policy engine can prove it unreachable under the versioned glob algebra; this prevents a
future-path bypass. Candidate policy still governs ordinary candidate findings, subject to the
trusted floor; it does not erase the transition finding.

A waiver is a separate authorization with stable target, finding kind, reason, owner, creation
evidence, and absolute UTC expiry. It never changes the underlying fact or acceptance. Expired or
invalid waivers remain visible. Meta-findings and state corruption are not waivable. Adjacent
one-line skip, permanent `not-applicable`, and automatic baseline refresh are not supported.

A blocking governed claim MUST resolve both a document owner and an evidence owner from a protected
ownership source. The CLI may show candidates, but offline state does not prove an eligible person
approved. If repository policy requests blocking without those owners, `owner-unresolved` itself is
an effective failure and the narrative claim cannot be called governed-current. A repository may
keep the claim advisory instead. Blocking additionally requires a provider-verified receipt for the
exact acceptance and its acceptance candidate, plus a separate trustworthy binding to the current
evaluation candidate; self-asserted and repository-reviewed records are report-only.

Attribution is computed by comparing base and candidate fact/finding sets. It is used for
diagnostics and structural adoption debt. It MUST NOT lower a protected governed invariant on the
exact final candidate. Pre-existing structural debt requires exact current key/fact equality with an
explicit external debt record; initialization MUST NOT mass-accept prose to create green state.

The core ADT reserves `improved` and `worsened` for a future finding kind whose schema publishes an
exact order and comparison function. Neither value is legal for a kind without that order;
scanner v0 publishes none. `resolved` means the key is absent from the candidate, while changed
non-equal unordered facts are `unknown`, not guessed improvement or worsening. The separately named
`debt-worsened` meta-finding means exact accepted fact-digest inequality, not an ordered magnitude.

## 13. Evaluation inputs and output honesty

A CI run receives explicit base and candidate Git object IDs from the provider and verifies that
both objects exist. It MUST evaluate the exact candidate tree and MUST NOT infer the safety base
from checkout depth, a mutable branch, or wall clock. A merge-group run follows the same rule even
when attribution to one human pull request is unavailable.

Read-only evaluation:

- MUST write no repository state, cache shared with a trusted context, issue, comment, branch, or
  remote service;
- MUST use no network, secrets, repository code execution, MDX evaluation, imports, plugins,
  generators, or probes;
- MUST distinguish parser error, timeout, resource limit, unsupported, missing, ambiguous, sparse
  content, submodule, LFS pointer, symlink, and invalid UTF-8;
- MUST fail incomplete on unreadable state, malformed declarations, unmerged index stages, parser
  crash, exhausted mandatory limits, or truncated analysis;
- MAY truncate human display after completing analysis, but must retain total counts and machine
  facts.

Current scanner-v0 output is **only** the strict report in machine-contracts; it has no logical
ledger root, governed claim/validator lifecycle, or inferred-candidate fields. The broader shape
below is a future governed-stage requirement and cannot be implemented until it has its own strict
schema and passes Gates B/C. That future deterministic report contains tool/report schema,
evaluation mode, base/candidate IDs, logical ledger root, every claim and observation fact, policy
trace, ownership/trust facts, coverage counts, and exit class. Arrays sort by stable identifier.
Ambient acquisition timestamps, host path, random run ID, ANSI text, and provider decorations
belong in a separate nondeterministic envelope. A schema-defined `evaluation_instant` used to decide
expiry is instead an explicit validity input and belongs in the deterministic payload; it is not
read from ambient wall time.

Every future governed summary includes at least:

- candidate documents discovered, scanned, excluded, unsupported, and unlinked;
- explicit structural references and inferred candidates;
- active, retirement-requested, retired, governed, unattested, and review-required claims;
- validator passed/failed/unsupported/error counts;
- waivers current/expired/invalid;
- record/warn/fail and incomplete counts.

Zero governed claims is reported as zero governed coverage. Agent-readable output exposes evidence
kind, scope, provenance, trust, and limitations. It MUST NOT call accepted prose true or instruct an
agent to prefer it as truth.

## 14. Governed-stage CLI candidate (not authorized)

This section is a future governed-stage research candidate and creates no current public command or
compatibility promise. It may be activated only by a new CLI RFC after Gates B/C and X-08. Scanner
v0 has exactly the single `assure check ...` surface in scanner-v0-spec; `assure scope`, `assure
scan`, every state command below, and all aliases currently return the scanner's invalid/unsupported
invocation exit 2. Within a later governed RFC, the vocabulary would standardize on `accept`; `ok`,
`link`, automatic `refresh`, and ledger-only claim creation would not be aliases.

| Command | Writes | Preconditions and transition |
| --- | --- | --- |
| `assure check --base <oid> --candidate <oid>` | No | Full exact-candidate evaluation and base-to-candidate transition verification |
| `assure verify-state [--base <oid>]` | No | Canonical/digest/internal verification; with base, verifies every transition/CAS edge |
| `assure init-state` | State records only | Empty-state bootstrap only: creates static meta and one active record for every explicit definition; acceptance is absent or not-applicable by relation; refuses an established/nonempty state |
| `assure create <claim> --reason <text>` | One claim record | Established-state addition for one authored unique definition and never-used ID; creates no acceptance |
| `assure accept <claim> --expect-record <seal> --reason <text>` | One claim record | Only for the closed acceptance-capable relation set; evaluates the staged index, snapshots all endpoints, creates one complete self-asserted event, and performs per-claim CAS |
| `assure request-retire <claim> --expect-record <seal> --reason <text>` | One record | Active to retirement-requested; declaration remains |
| `assure cancel-retire <claim> --expect-record <seal> --reason <text>` | One record | Retirement-requested to active |
| `assure retire <claim> --expect-record <seal> --reason <text>` | One tombstone | Base already contains retirement request; declaration is absent |
| `assure split <claim> --into <id...> --expect-record <seal> --reason <text>` | Closed multi-record transaction | Tombstones predecessor and creates at least two unattested successors |
| `assure merge <claims...> --into <fresh-id> --expect-record <seal...> --reason <text>` | Closed multi-record transaction | Tombstones at least two predecessors and creates one unattested successor |
| `assure migrate <claim> --expect-record <seal> --reason <text>` | One record | Binds the current changed definition and retains the old acceptance only as a review-required baseline; relation kind must be unchanged |
| `assure migrate-engine <claim> --expect-record <seal>` | One record | Only the equivalence-preserving transition in section 11 |
| `assure recover-state` | Affected local files | Completes or rolls back a locally interrupted transaction; never invents acceptance or lineage |

State-writing commands MUST evaluate the staged Git index. They refuse unmerged entries, missing
required blobs, sparse placeholders, or unstaged differences in a state file they would replace.
They write canonical state to the worktree for the user to stage and review. A worktree-only mode
may preview but MUST NOT write acceptance state.

`accept` requires a reason for every decision, including an unchanged acceptance. Multi-claim and
bulk acceptance return `UNSUPPORTED_COMMAND_SHAPE`, exit 2, and write nothing. Split and merge are
the only version 1 multi-record commands; they use the closed lifecycle transaction defined in
section 10.

Exit codes are fixed across commands:

| Exit | Meaning |
| --- | --- |
| `0` | Evaluation or requested mutation completed successfully and no effective blocking finding exists |
| `1` | Evaluation completed trustworthily and at least one effective blocking finding exists |
| `2` | Invocation, configuration, schema, Git state, CAS, parser/resource, state transition, or internal error prevented a trustworthy completion; a mutation wrote no valid partial result |

Machine JSON goes to stdout; logs and progress go to stderr. SARIF is deferred and is not a
version-1 command format. `CAS_CONFLICT` and other specific
errors are stable machine error codes inside exit class 2, not additional process exit codes.

## 15. Exact behavior for deferred capabilities

Deferred does not mean ignored or best-effort clean.

| Capability encountered | Required fact/finding | Gate behavior |
| --- | --- | --- |
| Cross-repository artifact | `resolution = unsupported`, `unsupported-scope` | No fetch; protected claim cannot complete |
| Immutable/release/environment/external scope | `scope = unsupported` | No fallback to candidate tree; protected claim cannot complete |
| Symbol, AST, captured-value, file-set, or text-region selector without installed support | Endpoint `unsupported` | No whole-file fallback; protected claim cannot complete |
| Arbitrary command, generator, probe, browser, or transcript validator | Validation `unsupported` | Never executed in the core job; relation cannot complete |
| Network URL | Structural observation `external-out-of-scope` | No request; advisory inventory only |
| LLM or similarity judgment | Advisory candidate with method/version/confidence | Never affects exit status or writes state |
| Automatic rename/move recovery | `migration-candidate` | Suggestion only; `migrate` required |
| Multi-subject claim or partial acceptance | `unsupported-schema` | No state transition; exit 2 for a governed declaration |
| Multi-claim or bulk acceptance | `UNSUPPORTED_COMMAND_SHAPE` | One claim per explicit review command; write nothing |
| Git path outside the `RepoPath` domain | `UNREPRESENTABLE_PATH` | Incomplete exit 2 with bounded raw-byte evidence; never a clean/unsupported fallback |
| Symlink dereference, absent submodule, LFS materialization, or sparse missing bytes | Typed unsupported/unavailable fact | No host-filesystem or network fallback |
| Expensive/scheduled evidence without a current result | `not-run`, `expired`, or `unsupported` | Cannot satisfy deterministic completion |
| Provider/service trust without a verified receipt | `trust = unverified` | Cannot satisfy that trust requirement |
| Translated-tree lag or cross-locale synchronization | Deferred; no version-1 request/rule ID | No locale-pair inference or lag claim; documents may still receive independent structural checks |
| Bitmap-diagram semantic interpretation or OCR | Deferred; no version-1 request/rule ID | Image path existence may resolve, but pixels establish no governed semantic fact |
| Refresh request | `UNSUPPORTED_COMMAND` | Exit 2 and write nothing |

Unknown major schemas, enums that affect validity, relation kinds, or selector kinds are not known
unsupported capabilities; they prevent trustworthy interpretation and therefore exit 2.

## 16. Policy and product-language constraints

Implementations and product copy MUST use these terms consistently:

| Forbidden or misleading | Required replacement |
| --- | --- |
| “fresh by construction” | `newly observed` for observations; `unattested` for claims |
| “editing clears staleness” | “the accepted subject projection changed; explicit acceptance is required” |
| “docs are in sync/fresh” | “no blocking findings in the evaluated scope” plus coverage |
| “verified true” | Exact validator result or “accepted against these snapshots” |
| “audit trail of who attested” in local mode | “self-asserted record stored in Git; reviewer identity and review are unproven” |
| “machine-owned state proves review” | “canonical state proves internal structure; review is an external control” |
| “all code references” | Exact supported reference classes and denominator |

The system establishes change, resolution, deterministic predicates, or explicit acceptance. It
does not establish arbitrary prose truth, completeness, reviewer attention, or graph completeness.

## 17. Normative invariants and required tests

Every invariant ID is a compatibility requirement. Tests may be unit, property, golden, or
end-to-end as indicated, but a stable state schema may not ship while any applicable invariant is
untested.

| ID | Invariant and required adversarial outcome | Minimum test |
| --- | --- | --- |
| `INV-ID-001` | Editing, moving, or retargeting a governed subject preserves `ClaimId`; content-derived `ObservationId` may change | Property |
| `INV-ID-002` | No content/location/ordinal-derived value can enter a `ClaimId` field | Schema/property |
| `INV-ID-003` | Duplicate active IDs and any tombstone reuse fail | End-to-end |
| `INV-OBS-001` | A new inferred block is an observation only and is never acceptance-current | End-to-end |
| `INV-ATT-001` | Init, scan, co-change, and any automatic process create no acceptance | Property |
| `INV-ATT-002` | Target changes followed by an unrelated typo in the claim remains review-required | Adversarial fixture |
| `INV-ATT-003` | Subject edit, selector retarget, scope change, dependency-set change, or validator change invalidates acceptance | Parameterized property |
| `INV-ATT-004` | Acceptance snapshots subject and every dependency atomically; no partial endpoint update is representable | Schema/property |
| `INV-ATT-005` | A structurally valid local acceptance is current only at `self-asserted` trust | End-to-end |
| `INV-ATT-006` | Verification-passed and review-required coexist without masking | Unit/property |
| `INV-ATT-007` | Reverting every endpoint to accepted projections may restore current; chronology alone cannot prevent it | Property |
| `INV-STATE-001` | Hand-editing a digest, seal, definition digest, path key, or predecessor fails verification | Mutation test |
| `INV-STATE-002` | Read-only check produces byte-identical repository state before and after | End-to-end |
| `INV-STATE-003` | Unrelated ClaimIds have independent logical CAS; the selected physical layout must meet X-06's pre-registered independent-update conflict budget | X-06 integration |
| `INV-STATE-004` | Same-claim concurrent accepts share a predecessor and the second fails CAS after the first lands | Merge simulation |
| `INV-STATE-005` | Logical ledger root and export are independent of traversal order and sharding | Property/golden |
| `INV-LIFE-001` | Deleting a failing active declaration/document without lifecycle emits governed-claim-removed | Adversarial fixture |
| `INV-LIFE-002` | Retirement requires a base-visible retirement request and leaves a permanent tombstone | Two-candidate integration |
| `INV-LIFE-003` | Split has at least two fresh successors, complete bidirectional lineage, no inherited acceptance, and a tombstone | Transaction test |
| `INV-LIFE-004` | Merge has at least two predecessors, one fresh successor, complete lineage, no inherited acceptance, and tombstones | Transaction test |
| `INV-LIFE-005` | A partial/crashed split or merge is invalid and cannot pass check | Fault injection |
| `INV-LIFE-006` | Exact-content move yields a suggestion only; no governed record retargets automatically | Adversarial fixture |
| `INV-LIFE-007` | Relation-kind change under one ClaimId is rejected; retire/create is required | Schema/transition test |
| `INV-ENG-001` | Same projection contract plus equal output remains comparable across implementation digest change | Unit |
| `INV-ENG-002` | Projection-schema change produces migration-required and cannot look like a doc change | Unit/golden |
| `INV-ENG-003` | Engine migration never makes an already impacted or raw-changed claim current | Property |
| `INV-ENG-004` | Safe mechanical migration requires equal raw evidence or a registered equivalence proof and retains original acceptance lineage | End-to-end |
| `INV-SCOPE-001` | Versioned/historical docs without supported scope are scope-unresolved and not compared with current main | Fixture |
| `INV-SCOPE-002` | Immutable/external scope never falls back to candidate-tree | Property |
| `INV-POL-001` | Candidate policy downgrade, exclusion, live-to-historical reclassification, or trust weakening emits an unsuppressible meta-finding | Parameterized adversarial test |
| `INV-POL-002` | Organization floor composition is monotone and repository policy cannot waive meta-findings | Property |
| `INV-POL-003` | A waiver leaves underlying resolution/change/acceptance facts visible; expiry never refreshes a baseline | Unit |
| `INV-CI-001` | Final merge candidate invalidates an earlier acceptance when a queued predecessor changes selected evidence | Merge-queue simulation |
| `INV-CI-002` | Attribution labels blame but never lowers a protected final-tree invariant | Property |
| `INV-CI-003` | Missing parser support, resource exhaustion, crash, or truncated analysis never produces exit 0 | Fault injection |
| `INV-COV-001` | Zero governed claims reports zero governed coverage, not globally clean docs | Snapshot |
| `INV-COV-002` | Excluded, unsupported, unlinked, waived, and truncated counts are present in every summary | Schema/snapshot |
| `INV-HASH-001` | Every implementation matches `GV-001` through `GV-005` and the full cross-platform suite | Golden |
| `INV-HASH-002` | Punctuation, numbers, Unicode form, ordering, trailing whitespace, and final newline remain significant in conservative subject projection | Parameterized golden |
| `INV-HASH-003` | Dependency and record sets reject duplicates and hash independently of traversal order | Property |
| `INV-SEC-001` | Path traversal, symlink dereference, unsafe path bytes, sparse content, submodule absence, and LFS placeholders fail or report typed unsupported facts, never escape scope | Adversarial fixture |
| `INV-CLI-001` | Every mutation obeys all-or-nothing CAS; conflict exits 2 and leaves no valid partial update | Fault/concurrency test |
| `INV-CLI-002` | `refresh` and `ok` are not state-writing aliases | CLI snapshot |
| `INV-CLI-003` | No state field uses the enclosing commit as a validity input | Schema test |

## 18. Closure of the pre-implementation blockers

| Review issue | Normative resolution |
| --- | --- |
| P0-01 automatic baselines versus attestation | Observations and acceptances are different layers; only explicit `accept` creates acceptance |
| P0-02 unstable governed identity | Explicit immutable `ClaimId`; content-derived IDs are observations only |
| P0-03 hidden authored intent in state | Definition/state boundary in section 3 and section 9.2 |
| P0-04 invalid repeated directive syntax | Repeated unqualified syntax is invalid; only conformance-tested versioned adapters may emit definitions |
| P0-05 relation labels without semantics | Closed relation ADT with authority, invalidation, arity, cycle, and completion rules |
| P0-06 hash/lock ambiguity | SHA-256, strict JCS/HB/HJ, seed vectors, per-claim canonical state |
| P0-07 unauthenticated lock | Explicit trust classes; local acceptance is only self-asserted |
| P0-08 refresh writes lifecycle state | No refresh writer or alias; lifecycle changes are explicit reviewed transitions |
| P0-09 unsafe delete/split/merge | Two-stage retirement, closed split/merge transactions, permanent tombstones |
| P0-10 undefined scopes | Closed `ScopeSpec`; only candidate-tree supported initially; no fallback |
| P0-11 attribution as merge safety | Exact final-tree invariant; attribution diagnostic; per-claim CAS |
| P0-12 policy can erase itself | Base/candidate semantic policy diff and unsuppressible meta-findings |
| P0-13 dishonest green/first run | Init is unattested; coverage-bearing success sentence only |
| P0-14 selector/engine migration ambiguity | Separate selector intent, projection contract, implementation, and explicit migration rules |
| P1-01 ownership/reviewer proof | Protected document/evidence owners plus provider-verified minimum for every blocking narrative acceptance; local self-assertion is report-only |
| P1-02 adoption debt | Explicit structural debt ratchet; never mass-accept prose |
| P1-03 privileged automation | Core check is read-only, networkless, secretless; writers are local staged-index commands |
| P1-04 validator provenance | Complete validator/environment contract; unsupported until hermetic for derivation claims |
| P1-05 errors versus unsupported/skipped | Orthogonal facts and fixed exit 0/1/2 semantics |

This contract deliberately makes governed assurance more explicit than zero-configuration
discovery. That cost is the consequence of preserving identity, review obligations, and lifecycle
without allowing edits, refreshes, deletion, policy changes, or concurrent merges to manufacture a
green result.
