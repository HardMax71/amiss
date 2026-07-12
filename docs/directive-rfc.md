# RFC A-001: governed claim directives

Date: 2026-07-11.

Status: accepted syntax contract for a future governed-claim pilot. This RFC does not enable an
adapter or authorize persisted claim state. Scanner v0 treats every reserved directive as
`unsupported-capability: governed-claim`; it does not create, lint as valid, or require governed
declarations. An adapter becomes enabled only after the implementation-readiness and conformance
gates pass.

Normative words `MUST`, `MUST NOT`, `SHOULD`, and `MAY` have their usual RFC 2119 meanings.

## Decision

A governed Markdown or MDX claim is declared by one unique, unused link-reference definition
immediately before the source block it governs:

```markdown
[assure:docs.expr-precedence]: <assure:v1/describes?selector=file-content&target=modules%2Fparser%2Fsrc%2Fmain%2Fantlr4%2FSpec.g4>

`implies` binds loosest of the binary operators, so parenthesize invariant bodies.
```

A governed structural reference uses the `reference` relation and declares whether the authored
target is a regular repository file or a repository tree:

```markdown
[assure:docs.module-tree]: <assure:v1/reference?selector=path-exists&artifact=repository-tree&target=modules%2Fparser>

The parser implementation is under the parser module.
```

The claim ID is `docs.expr-precedence` or `docs.module-tree`. The subject is the next eligible
top-level source block. The URI declares one of the two complete simple shapes defined below.
Accepted fingerprints and lifecycle state never appear in the directive.

The `check=<id>` query shape is reserved so a future separately versioned root named-check schema
can reuse the label and URI envelope. It is recognized but unsupported in RFC A-001. A directive
using it produces `unsupported-named-check-schema`, emits no `ClaimDefinition`, and cannot complete
or create state. In particular, a `generated-from?check=published-openapi` line is not a valid v1
claim today.

The earlier repeated `[assure]: ...` and `[assure]: skip` forms are withdrawn. CommonMark labels
have document-wide lookup semantics and the first of several matching definitions wins; repeated
labels therefore cannot be independent governed declarations. See the
[CommonMark 0.31.2 link-reference definition rules](https://spec.commonmark.org/0.31.2/#link-reference-definitions).

## Goals

The syntax MUST:

- carry a stable explicit `ClaimId` across subject edits and moves;
- remain invisible when rendered by conforming Markdown/MDX pipelines;
- preserve a source span without evaluating the document;
- keep authored intent next to the governed subject;
- distinguish relation semantics from selector syntax;
- support exactly one simple dependency without a root manifest entry;
- reserve, but not implement, routing to a named root check;
- reject ambiguity and unknown semantics rather than guessing;
- leave ownership, waivers, fingerprints, acceptance, and lifecycle outside authored prose.

The syntax is not intended to encode arbitrary graphs, shell commands, policy, reviewer identity,
or state.

## Supported formats

RFC A-001 defines candidate adapters only for:

- CommonMark-compatible `.md` files;
- GitHub-flavored Markdown constructs accepted by the selected Markdown adapter;
- `.mdx` source parsed without evaluation by the selected MDX adapter.

No candidate adapter is enabled merely because this RFC exists. Plain-text discovery remains an
ungoverned scanner concern, and plain text cannot declare a governed claim in v1. reStructuredText,
AsciiDoc, Org, HTML documents, notebook formats, and directives nested inside code comments return
`unsupported-declaration-format`. They MUST NOT fall back to superficially similar scope rules.

Each supported adapter MUST pass the shared semantic fixture suite plus format-specific adversarial
fixtures before it can create governed claims. “The parser accepted the file” is not sufficient;
claim ID, relation, selector/check, subject bytes, and source span MUST match the contract.

## Claim ID

The reserved label is `assure:` followed by a `ClaimId`:

```text
claim-id = lower-alpha *( lower-alpha / digit )
           *( ( "." / "-" ) 1*( lower-alpha / digit ) )
```

Additional rules:

- length is 3 through 128 ASCII bytes;
- the first character is `a` through `z`;
- the final character MUST be alphanumeric;
- adjacent or trailing separators are forbidden;
- IDs are compared as exact lowercase ASCII and are unique across the repository scope;
- an ID present in any active or retired state record cannot be reused for a different logical
  claim;
- CommonMark label normalization MUST NOT be used as a second identity algorithm. Input outside the
  canonical lowercase grammar is invalid.

The stable ID is authored because no deterministic algorithm can infer logical continuity through
arbitrary edits, moves, split, and merge. A content digest identifies the current subject version,
not the continuing claim.

## URI grammar

The link destination MUST use the `assure` scheme and an angle-bracket destination:

```text
assure-uri = "assure:v1/" relation "?" query
relation   = "reference" / "describes" / "generated-from" /
             "constrains" / "equivalent" / "historical-at"
```

The parser recognizes these query envelopes:

```text
reference-query = "selector=path-exists&artifact=" artifact-kind
                  "&target=" pct-encoded-target
describes-query = "selector=file-content&target=" pct-encoded-target
check-query = "check=" check-id
artifact-kind = "repository-file" / "repository-tree"
```

Only two query/relation combinations emit a version 1 `ClaimDefinition`:

| Relation | Required query | Meaning |
| --- | --- | --- |
| `reference` | `reference-query` | The subject names one repository entry whose authored kind and path must resolve |
| `describes` | `describes-query` | The subject is narrative prose governed against the complete text projection of one regular UTF-8 repository file |

`path-exists` is legal only with `reference`. `artifact` is mandatory because “the path exists” is
not enough to distinguish a file from a tree. `file-content` is legal only with `describes` and
implies `artifact = repository-file`; adding an `artifact` key is noncanonical and invalid. There
is no simple `describes?selector=path-exists` form because structural resolution is a `reference`,
not narrative acceptance.

Every `check-query` is recognized but returns `unsupported-named-check-schema` until a separately
versioned root named-check schema is accepted and installed. The parser MUST preserve the relation
and `check-id` for diagnostics, but MUST NOT expand a check, infer endpoints, emit a partial
definition, execute anything, or create state. Missing root configuration does not turn this into
a simpler claim.

All other relation/query combinations are `unsupported-relation-shape` when their syntax is
recognizable. Duplicate keys, unknown keys, a mixed simple/check query, or keys outside the exact
canonical order are `invalid-directive-uri`. An unsupported selector is never reinterpreted as
`file-content` or `path-exists`.

Canonical percent encoding operates on the UTF-8 bytes of the target. ASCII letters, digits, `-`,
`.`, `_`, and `~` remain literal. Every other byte, including `/`, `%`, `+`, `?`, `#`, and every
non-ASCII byte, is encoded as `%` plus two uppercase hexadecimal digits. `+` is never decoded as a
space. Decoding occurs exactly once and must produce one valid `RepoPath`.

Decoded repository targets:

- use `/` separators and no leading `/`;
- contain no empty, `.`, or `..` segment;
- preserve case;
- MUST NOT contain backslash, NUL, a control character, query, or fragment;
- are limited to 1,024 UTF-8 bytes;
- identify Git tree entries, not arbitrary filesystem paths;
- do not follow a symlink target outside the evaluated tree.

A `check-id` uses the same lexical grammar as `ClaimId` with a 64-byte maximum. RFC A-001 does not
define a root configuration, check expansion, validator binding, or named-check digest. Those must
arrive together in a separate versioned schema; state can never be their sole definition.

### Exact semantic expansion

Every supported simple directive expands to the canonical `ClaimDefinition` in
[normative-core-spec.md](./normative-core-spec.md#32-claim-definition). The adapter derives the
following values; authors cannot override them with URI parameters.

The scope for the claim, subject artifact, and dependency artifact is exactly:

```json
{"kind":"candidate-tree","repository":"self"}
```

RFC A-001 has no historical, release, environment, external, or endpoint-specific scope. Adding
scope syntax is invalid. A future scope-bearing directive requires a new URI version.

The top-level expansion is exactly:

```json
{
  "schema": "assure.claim/v1",
  "claim_id": "<label ClaimId>",
  "relation": {"kind": "<reference or describes>"},
  "scope": {"kind": "candidate-tree", "repository": "self"},
  "subject": "<subject endpoint below>",
  "dependencies": ["<target endpoint below>"],
  "validators": []
}
```

The placeholders above denote the canonical objects defined below; they are not serialized as
strings. `relation.kind` is copied from the only supported relation/query combination, and
`claim_id` is the exact canonical label suffix.

The subject endpoint is exactly:

```json
{
  "endpoint_id": "subject",
  "selector": {
    "schema": "assure.selector/v1",
    "kind": "document-region",
    "selector_schema": 1,
    "artifact": {
      "schema": "assure.artifact/v1",
      "kind": "document",
      "repository": "self",
      "locator": {"path": "<document RepoPath>"},
      "scope": {"kind": "candidate-tree", "repository": "self"}
    },
    "parameters": {"binding": "adjacent-subject-v1"},
    "cardinality": "exactly-one",
    "projection": {"kind": "repository-text", "projection_schema": 1},
    "path_semantics": "locator"
  }
}
```

`<document RepoPath>` is the exact bytewise, case-sensitive repository path of the declaring
document. The selected raw evidence is the exact source-byte span of the next eligible block. Its
`repository-text/v1` projection converts CRLF and bare CR inside that span to LF and preserves
every other UTF-8 code point, whitespace byte, punctuation mark, ordering choice, and final-newline
presence. The accepted snapshot retains the complete raw digest separately. Heading and source
position are diagnostic metadata, not selector parameters.

The only dependency has the derived `EndpointId` `target`. Authors cannot rename it. Retargeting
preserves that endpoint ID and changes its `SelectorId`.

A `reference-query` expands `target` as follows:

```json
{
  "endpoint_id": "target",
  "selector": {
    "schema": "assure.selector/v1",
    "kind": "path-exists",
    "selector_schema": 1,
    "artifact": {
      "schema": "assure.artifact/v1",
      "kind": "<artifact query value>",
      "repository": "self",
      "locator": {"path": "<decoded target RepoPath>"},
      "scope": {"kind": "candidate-tree", "repository": "self"}
    },
    "parameters": {},
    "cardinality": "exactly-one",
    "projection": {"kind": "entry-identity", "projection_schema": 1},
    "path_semantics": "identity"
  }
}
```

`repository-file` requires a regular Git blob with mode `100644` or `100755`.
`repository-tree` requires a Git tree. A symlink, gitlink, absent object, or kind mismatch does not
satisfy the endpoint. The projection contains the canonical path, authored entry kind, and Git
mode; it contains no target file bytes. A target content change therefore does not invalidate a
`reference` whose entry identity and mode are unchanged.

A `describes-query` expands `target` as follows:

```json
{
  "endpoint_id": "target",
  "selector": {
    "schema": "assure.selector/v1",
    "kind": "file-content",
    "selector_schema": 1,
    "artifact": {
      "schema": "assure.artifact/v1",
      "kind": "repository-file",
      "repository": "self",
      "locator": {"path": "<decoded target RepoPath>"},
      "scope": {"kind": "candidate-tree", "repository": "self"}
    },
    "parameters": {},
    "cardinality": "exactly-one",
    "projection": {"kind": "repository-text", "projection_schema": 1},
    "path_semantics": "identity"
  }
}
```

This form requires one regular tracked UTF-8 file. Its projection uses the same conservative
newline-only normalization as the subject, while `raw_digest` preserves the complete Git blob
bytes. Binary files, symlinks, trees, gitlinks, LFS content that is not materialized in the Git
blob, and ambiguous or missing targets cannot produce an accepted endpoint.

Both expansions set `validators` to `[]`. `reference` completes when the subject and target
resolve. `describes` completes only through one explicit atomic acceptance covering `subject` and
`target`. The `ClaimDefinition` contains exactly one subject and one dependency; no inferred
dependency can be added during acceptance.

## Relation semantics

The URI relation names a closed semantic type. It is not a display tag.

| Relation | Core requirement | RFC A-001 behavior |
| --- | --- | --- |
| `reference` | One document subject and one or more existence dependencies; every endpoint resolves | Supported simple shape with exactly one derived `target` dependency after an adapter is enabled |
| `describes` | One document subject and one or more evidence dependencies; explicit acceptance snapshots all endpoints | Supported simple shape with exactly one derived `target` dependency after an adapter is enabled |
| `generated-from` | One output subject, complete declared inputs, generator/environment identity, and one hermetic-regeneration validator | Recognized but `unsupported-relation-shape`; an adjacent document-region plus one selector cannot encode the required output and validator contract |
| `constrains` | One normative subject, implementation dependencies, and explicit `completion = validator` or `completion = acceptance` | Recognized but `unsupported-relation-shape`; RFC A-001 has no completion constructor or validator binding |
| `equivalent` | One reporting subject, peers, and deterministic two-input validator | Recognized but `unsupported-relation-shape`; RFC A-001 has neither peer endpoints nor a validator |
| `historical-at` | One document subject and dependencies in one immutable scope; pinned scope resolves and explicit acceptance is current | Recognized but `unsupported-relation-shape`; RFC A-001 fixes every endpoint to `candidate-tree` and cannot express immutable scope |

The last four relations remain in the grammar only so future syntax is diagnosed as known but
unsupported rather than guessed. A `check-query` does not make them usable because the named-check
schema is also unsupported. No generated, constraining, equivalent, or historical claim can emit a
partial definition or complete under RFC A-001.

Authority and invalidation are derived from the relation ADT and cannot be overridden by query
parameters. `reference` asserts resolution only and has no acceptance event. `describes` requires a
current explicit acceptance over both endpoints; subject co-change or validator-like evidence
elsewhere cannot discharge it.

## Subject binding

The definition attaches to the next eligible top-level source block after the definition. Blank
lines are ignored. Eligible subjects are:

- paragraph;
- list;
- GFM table;
- fenced or indented code block;
- top-level raw HTML block;
- top-level MDX JSX flow element treated as opaque source bytes.

The following end the search and make the directive `orphan-directive`:

- the next heading;
- a thematic break;
- end of file;
- frontmatter or ESM after the directive;
- a container boundary the adapter cannot model consistently.

Additional binding rules:

- directives MUST be top-level, not nested in a list, block quote, footnote, JSX child, or HTML
  element;
- exactly one governed directive may attach to a subject block in v1;
- a directive definition is not a subject block;
- a second directive before an eligible subject is `multiple-directives-for-subject` for both;
- the subject raw evidence is the exact source byte span; `repository-text/v1` performs only the
  newline conversion defined in the semantic expansion above;
- headings and section slugs are display/navigation metadata, not identity;
- the source path and robust quote/context hints are mutable locators, not `ClaimId`;
- an MDX JSX subject is opaque: references inside it are not inferred unless a future adapter
  explicitly supports that component grammar.

The tool MUST use parser source positions or a grammar-aware lexer. It MUST NOT find a subject by a
regular expression that can confuse nested JSX, code strings, comments, or frontmatter.

## Reserved-label behavior

Any link-reference label beginning `assure:` is reserved.

- The definition MUST be unused. A link/image reference that consumes the label is
  `directive-rendered-as-link` and invalid.
- Duplicate reserved definitions are invalid even if a Markdown parser would choose the first.
- An unknown `assure:` URI version is `unsupported-directive-version`.
- A reserved label pointing to a non-`assure` URI is `invalid-directive-uri`.
- A non-reserved unused reference definition remains ordinary document content.
- An enabled governed document adapter MUST preserve every definition node in source order and
  MUST NOT collect reserved definitions into a map before duplicate diagnostics are emitted.

Scanner v0 recognizes the reserved prefix only to emit
`unsupported-capability: governed-claim`. It does not apply this RFC's semantic expansion,
duplicate-ID rules, subject binding, lifecycle, or completion logic. That behavior is fixed by
[scanner-v0-spec.md](./scanner-v0-spec.md) and
[ci-security-spec.md](./ci-security-spec.md#capability-boundary).

Repositories using a Markdown linter MUST configure its unused-reference rule narrowly for the
`assure:` prefix. A project-wide disable is not part of this RFC.

## Declaration and state boundary

For a supported simple directive, the directive and this RFC's closed expansion contain all
governed intent:

- stable claim ID;
- relation type;
- subject binding;
- dependency selector intent;
- fixed `candidate-tree` scope;
- the empty validator set.

A named-check directive cannot satisfy this boundary until its separately versioned root schema
defines and hashes the complete endpoint, relation, scope, and validator intent. RFC A-001 therefore
reports it unsupported before creating a definition.

Persisted state contains resolutions, snapshots, acceptance, trust, and lifecycle. It MUST NOT
silently prune an inferred dependency set, change relation semantics, or hold a governed claim
whose declaration is absent. A zero-touch ledger-only governed claim is not supported.

Ownership comes from protected policy and provider ownership data. Waivers live in their own
governed authorization surface. `skip`, `ignore`, reviewer, reason, timestamp, and fingerprint are
not legal directive parameters.

## Lifecycle interaction

The same `ClaimId` follows the logical claim through subject edits and document moves. These events
do not silently update accepted state:

| Candidate change | Required finding or transition |
| --- | --- |
| New supported declaration with an unused ID | `create`; `reference` may complete by resolution, while `describes` starts unattested |
| Subject projection changes without changing selector intent | `review-required` for `describes`; explicit `accept` records both current endpoints |
| Only raw subject bytes change while the versioned projection remains equal | `raw-changed` forensic fact; current acceptance is not invalidated |
| Document path changes, same ID remains | `locator-changed`; explicit `migrate` MUST bind the new definition before `accept` is legal |
| Target path or selector intent changes | `definition-changed`; explicit `migrate` first, then a new `describes` acceptance; `reference` is re-evaluated after migration |
| Relation kind changes | Not a migration; request retirement of the old ID and create a fresh ID for the new relation |
| Directive disappears while state is active | Unsuppressible `governed-claim-removed` |
| Claim is intentionally deleted | Candidate one uses `request-retire` while the declaration remains; only a later candidate whose base contains that request may remove the declaration and write the permanent tombstone |
| Retirement request is canceled | `cancel-retire` while the declaration remains; ordinary evaluation never stopped |
| One claim becomes several | Closed `split`: at least two fresh successor IDs, complete bidirectional lineage, predecessor tombstone, and no inherited acceptance |
| Several claims become one | Closed `merge`: at least two predecessor tombstones, one fresh successor ID, complete lineage, and no inherited acceptance |
| Retired ID reappears | `claim-id-reused`, always invalid |

Scan or extraction may observe that a directive-shaped line exists. It writes nothing and cannot
create, accept, migrate, split, merge, request retirement, retire, or reuse a claim. There is no
automatic refresh operation in this RFC.

Every mutation names the exact predecessor record seal and follows the compare-and-swap and
all-or-nothing transition rules in
[normative-core-spec.md](./normative-core-spec.md#10-lifecycle-and-concurrency). A stale predecessor,
partial split/merge, or attempt to accept a moved-but-unmigrated definition writes nothing.

## Canonical formatting

Canonical source syntax is mandatory in version 1. An enabled adapter reports
`noncanonical-directive` and emits no `ClaimDefinition` for a recognizable line that differs from
this form:

- no indentation;
- exact lowercase `[assure:<claim-id>]`;
- one ASCII space after `:`;
- angle-bracket URI;
- no title, surrounding whitespace, or trailing whitespace;
- lowercase scheme, version, relation, keys, selector, and check ID;
- relation-specific query keys in the exact order defined above;
- uppercase percent escapes;
- exactly one LF or CRLF source-line terminator immediately after `>`.

LF and CRLF are the two canonical transport encodings of the same directive line. The adapter
strips that terminator before URI parsing. Bare CR, a missing line terminator, a second blank or
space on the directive line, and mixed `CRCRLF` are invalid. The following subject retains its own
source line endings; its projection performs the core newline normalization independently.

A formatter MAY repair a recognizable noncanonical directive as a normal source edit. It MUST
rewrite only the directive line, preserve whether that line used LF or CRLF, and MUST NOT rewrite
subject prose or state. The checker never accepts the pre-format spelling, and invalid or ambiguous
input is never reformatted by guessing.

## Security and limits

- A directive line is limited to 4,096 UTF-8 bytes.
- The parser performs no network access and treats the `assure` URI as data, never as a fetchable
  URL.
- It executes no named check while parsing.
- Path normalization happens before tree lookup and cannot escape the Git tree.
- Malformed percent encoding, invalid UTF-8, controls, traversal, duplicate keys, or over-limit
  input is an invalid declaration.
- Parser failure, resource exhaustion, or an unsupported protected declaration makes the complete
  evaluation fail closed; it cannot be silently skipped.
- Diagnostic output escapes control characters and bounds displayed source.

## Versioning

The URI `v1` versions authoring syntax and meaning. Selector and projection schemas have independent
versions in resolved snapshots. A binary supporting only v1:

- rejects malformed v1 input;
- preserves but reports `unsupported-directive-version` for a syntactically recognizable future
  version;
- never interprets future input with v1 defaults;
- cannot change v1 subject binding or query meaning in place.

Additive query keys are not backward compatible because unknown keys can change validity. New
semantics require `v2` or a named check schema upgrade with explicit migration.

## Required conformance fixtures

Before governed claims ship, the fixture suite MUST cover:

1. valid file and tree `reference` directives and one valid file-content `describes` directive in
   Markdown and MDX, with identical expansion after substituting each fixture's declaring
   `RepoPath`;
2. `check=<id>` for every recognized relation returning `unsupported-named-check-schema` without
   expansion, execution, or state;
3. repeated old `[assure]` definitions;
4. duplicate unique IDs in one file and across files;
5. label case variants and Unicode lookalikes;
6. definition consumed by a shortcut, collapsed, full, or image reference;
7. frontmatter, ESM, JSX, nested JSX expressions, code strings, and comments containing fake
   directive text;
8. definition under a heading, before every eligible subject kind, and before each terminating
   construct;
9. nested list, block quote, footnote, HTML, and JSX-child definitions;
10. invalid, duplicate, reordered, unknown, and over-limit query parameters, including omitted or
    mismatched `artifact` on `reference` and forbidden `artifact` on `describes`;
11. percent-encoded slash, hash, question mark, percent, controls, invalid UTF-8, traversal, and
    backslash;
12. LF, CRLF, bare CR, missing directive terminator, Unicode subject bytes, trailing whitespace,
    noncanonical case/encoding/key order, and missing subject final newline;
13. exact subject and `target` endpoint selector expansion, artifact kind, scope, schemas,
    parameters, cardinality, projections, path semantics, raw bytes, and newline-normalized text;
14. parser error and resource-limit behavior;
15. rendered-output snapshots proving valid unused directives emit no visible content in every
    claimed renderer;
16. document move requiring `migrate` before `accept`, heading rename, subject edit, target
    retarget, relation change requiring retire/create, two-stage retirement, cancel-retire, split,
    merge, and ID reuse transitions;
17. simple `generated-from`, `constrains`, `equivalent`, and `historical-at` shapes returning
    `unsupported-relation-shape`, including proof that historical scope never falls back to the
    candidate tree;
18. scanner v0 returning `unsupported-capability: governed-claim` without applying subject binding
    or creating state.

The repository-local results and any remaining external renderer matrix are recorded in
[preimpl-experiments.md](./preimpl-experiments.md). Unsupported adapters stay outside the governed
claim compatibility statement.

## Rejected alternatives

| Alternative | Reason rejected |
| --- | --- |
| Repeated `[assure]: target` | Document-wide duplicate-label semantics; no stable ID or relation type |
| `[assure]: skip` | Overloads declaration with policy bypass and duplicate-label behavior |
| HTML comments | Not one portable Markdown/MDX source contract; comments can be stripped or rejected |
| MDX `{/* ... */}` comments | MDX-specific and unusable in plain Markdown |
| Heading slug as ID | Changes on rename and collides under duplicate headings |
| Content hash as governed ID | Changes on meaningful edit and cannot preserve lifecycle |
| Line range as subject | Silently retargets after insertions |
| Ledger-only claim | Hidden authored intent and no source-local review visibility |
| Per-document YAML sidecar | Detached second authoring surface with move/orphan failure modes |
| Arbitrary inline JSON/YAML | Excess syntax, weak renderer portability, and larger parser attack surface |

This RFC intentionally pays one visible source-line and one stable ID for governed narrative
claims. That is the minimum honest cost of durable identity; zero-authoring remains available only
for non-governed observations.
