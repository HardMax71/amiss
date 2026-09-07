# Trusted semantic evidence

Some documentation identities exist only after another tool has done work the repository tree
cannot represent. A Sphinx inventory maps foreign object names to published URIs. A completed site
build owns generated routes, anchors, redirects, versions, locales, and navigation. Amiss does not
execute either producer inside the engine, but those different producers need the same trust and
replay boundary.

The semantic-evidence envelope is that boundary. Its payload binds:

- the scanner candidate-identity digest, which already covers repository identity, refs, both
  snapshot materializations, and forge semantics;
- an optional source-report payload digest when evidence was derived after a scan;
- the producer kind, stable implementation identity, version, independently selected semantic
  context, and the kind-defined digest of all inventories or completed-build input;
- whether the producer completed that exact input;
- at most 100,000 observation objects, sorted by canonical JSON and unique.

The envelope carries the domain-separated payload digest and is limited to 16 MiB. Its strict
reader refuses malformed JSON, unknown fields or producer and observation kinds, invalid identities,
duplicate or unsorted observations, oversized input, and a mismatched digest. Construction sorts
observations once so a filesystem, inventory, or build traversal order cannot change the evidence
identity.

Producer kinds are the closed set `sphinx-inventory-set`, `site-build`, and `record-set`.
Templates, controller acquisition expectations, and report provenance use the same set.
Implementation identities remain producer-owned; a new producer family requires a compiled
consumer and a corresponding contract update.

Observation vocabularies do not share a synthetic universal graph. An Intersphinx producer needs
domain, role, object name, inventory identity, and URI. A site-output producer needs routes,
anchors, redirects, navigation edges, and source attribution. Envelope and template readers decode
observations directly into the existing closed site, Sphinx label, or record-set models. Unknown
kinds or fields and positional observation arrays are rejected at that boundary. Binding a template
borrows its typed observations through sorting with the standard library's `Cow`; decoded rows
are owned. The controller keeps the bound envelope by taking ownership of those rows, without
reparsing its audit bytes. The scanner consumes the same model without rebuilding its fields or
encoding and parsing each row again. Sealed scanner intake consumes the typed controls request,
retaining its envelope storage instead of encoding and reparsing it. A counting writer preserves
the exact encoded-byte ceiling for in-process inputs too; it does not retain a JSON buffer.
Compiled consumers additionally check producer versions, family membership, and semantic
laws such as valid routes or sorted record keys. Parsing an envelope never turns it into a pass,
a block, or a suppression.

Envelope construction returns the typed model; canonical encoding belongs to intake and output
boundaries. Template intake checks the bound envelope's byte ceiling through a sink, without
retaining a discarded output buffer. Controller audit capture and site-output producers write
the canonical bytes they actually retain. The canonicalization library may still allocate
internally; this separation does not promise allocation-free serialization.

This contract authenticates nothing by itself. Provider-enforced use must acquire it outside the
repository. Each sealed value carries an independently planned expected context digest; the engine
requires the producer's context digest to match before interpreting any observation. A repository
file, cache entry, or self-asserted local producer cannot promote its own observations to authority.
Partial evidence may prove a fact positively only where a later kind contract permits it; absence
can carry meaning only for a declared complete set over the exact input digest.

The first compiled consumer accepts one complete `sphinx-inventory-set` producer at version `1`,
with no source-report binding. A `sphinx-label` observation carries an inventory identity, a
Docutils-normalized label, and one syntactically valid absolute HTTP(S) destination. The engine uses
that table only after every envelope in the controls request matches the exact candidate identity.
One unique prefixless `:ref:` label resolves through the inventory; repeated labels across
inventories remain ambiguous, colon-prefixed names remain unsupported, and local declarations keep
precedence. Missing evidence, an incomplete producer, another producer version, a stale candidate
binding, or an invalid observation can never clear a missing label.

The second compiled consumer accepts at most one complete `site-build` producer at version
`0.5.1`. A `site-route` observation carries one exact absolute-path URI, one repository source
document, and a byte-sorted unique set of decoded anchor identities. Routes exclude authority,
query, and fragment components; sources obey the repository-path grammar; anchors and their
aggregate count are bounded. On the candidate side only, an exact route resolves to its scanned
structured source. A nonempty fragment first matches a published `id` or legacy `<a name>` anchor
verbatim, then by percent-decoded identity; ASCII-case-insensitive `top` identifies the page top.
This preserves a literal percent sign in a valid anchor without making malformed escapes resolve by
guesswork. A `site-generated-route` carries the same route and anchors plus a required nullable
source. A repository path is attribution for generated output, not its target body, and must remain
an exact ordinary candidate blob. `null` says the completed page has no repository attribution; it
does not invent a virtual source. Either form resolves as `external/site-build`. A missing source
field or malformed attribution rejects the complete evidence. Query text remains identity data. A
route absent from the evidence, an absent anchor, an unsuitable attributed source, and image use
remain unsupported rather than being guessed into either a pass or a failure.
A `site-redirect` observation maps one exact redirect route and its repository routing source to
its exact terminal route, not an intermediate hop. The destination may carry a fragment but no
query. It resolves only when that terminal route has one uniquely claimed source-backed or
generated page and the effective fragment is in its anchor set. Following the
[HTTP Location rule](https://www.rfc-editor.org/rfc/rfc9110.html#section-10.2.2), an absent
destination fragment inherits the authored fragment, a nonempty one replaces it, and an empty `#`
suppresses inheritance. Self-redirects and fragments containing raw control characters make the
evidence invalid.
Conflicting route owners and redirects ending at a missing, ambiguous, nonterminal, or anchor-less
target do not resolve and each produce one `site-build-defect` whose fact retains the exact route,
claim identity, reason, and every available routing source. A conflict containing only unattributed
generated pages has an empty source set and no location path rather than a fabricated one. A
`site-navigation` observation adds one source root, its navigation manifest, rendered entrypoint
routes, and the byte-sorted unique source set reachable through the completed link graph; that set
may be empty.
Every entrypoint must be a unique page route, every reachable source must own a repository-backed
route, and all named sources must remain beneath the declared root. Only then does
`unlinked-document` mean a scanned structured source inside that root which is neither the manifest
nor reachable. Without this observation the engine makes no navigation claim. The base side never
consumes candidate build output.

The public `check` command may read one candidate-independent
[semantic template](https://github.com/HardMax71/amiss/blob/main/spec/scanner-semantic-template.schema.json).
It has the producer, completeness, and observation fields above but no subject field. After the
scanner resolves the exact commit tree or pins the staged-index projection, it binds the template
to that candidate and passes the resulting envelope through the same compiled consumers. The file
is repository-user-selected and its context is not independently planned, so the report remains
`self-asserted`; this local convenience path cannot become provider authority.

The offline `amiss record-set` authoring form accepts one closed
[normalized record-set input](https://github.com/HardMax71/amiss/blob/main/spec/scanner-record-set-input.schema.json)
and emits that template shape with the fixed `record-set@1` producer contract. Its rows pass the
same record validation the scanner uses: keys are sorted and unique, and keys and display values are
nonempty, control-free, and bounded. The specialist still owns extraction, its stable identity,
both supplied digests, and whether the set is complete. Amiss neither executes a language tool nor
recomputes or authenticates those claims; the command only validates and canonicalizes their
transport. Its output therefore remains self-asserted when supplied to `check`.

The separate unpublished `amiss-rust-public-api` producer is one such specialist, but it writes the
checked template directly so the same bytes can be a planned workflow artifact. It accepts exactly
one bounded producer context and one bounded Rustdoc JSON file:

```console
amiss-rust-public-api --context rust-public-api-context.json \
  --rustdoc target/doc/example.json > amiss/semantic-template.json
```

The producer currently consumes format 61. The example was measured with
`nightly-2026-08-28`; the workspace's pinned Rustdoc emits format 60 and is deliberately refused.
Generate the input with the exact separately pinned toolchain named by the producer context, for
example:

```console
cargo +nightly-2026-08-28 rustdoc -p example --lib -- \
  -Z unstable-options --output-format json
```

Do not edit the format number in an artifact.

The context is strict JSON with this closed shape:

```json
{
  "cfg": [],
  "compiler": "rustc 1.100.0-nightly (e457a7b0d 2026-08-27)",
  "dependencies_digest": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "features": ["default"],
  "name": "rust/example/local-function-declarations",
  "package": "example",
  "rustdoc_format": 61,
  "schema": "amiss/rust-public-api-context",
  "target": "example",
  "target_triple": "x86_64-unknown-linux-gnu"
}
```

The feature and cfg sets are byte-sorted and unique. The set name ends in
`/local-function-declarations`, so its completeness cannot be mistaken for the entire Rust item
surface or dependency-owned re-exports. Compiler, package, target, features, cfg, and the
operator-computed dependency/configuration digest all enter the context digest. `target` is the
Rust crate target name recorded on the root module, after Cargo's crate-name normalization or any
explicit target rename; it and the target triple are checked exactly against the parsed input.
Package and crate target names are intentionally separate because Cargo permits them to differ.
Rustdoc carries no Cargo package identity, so `package` is context-bound while the independently
declared target is the artifact-side check. The active producer accepts only the one Rustdoc format
represented by its pinned maintained adapter. It does not start Cargo, rustdoc, or another process.
Completeness applies only to that exact feature, cfg, target, and dependency context. Two
configurations are independent record sets and need distinct names when supplied to one scan;
neither the producer nor the scanner silently unions or intersects them. A matrix-wide API requires
a separately named producer contract that declares union or intersection and resolves keys whose
values differ between configurations.

The complete set contains public free functions defined by the root crate, public functions from
inherent implementations of its public structs, enums, and unions, and functions declared by its
public traits. Keys use the disjoint `fn/`, `inherent-fn/`, and `trait-fn/` namespaces followed by
the adapter-owned public import path; an associated-function path appends its method name to its
owner path. A local re-export therefore owns its alias rather than the definition's private path.
Trait-implementation bodies and functions defined by a dependency are outside this explicitly
scoped set; those need separate impl-relation or dependency inputs before they can be called
complete. Specialized inherent implementations that expose the same public owner and function name
are refused as ambiguous; neither Rustdoc numeric IDs nor rendered syntax are promoted into a
false stable identity.

Each value is a one-line canonical comparison string made from the adapter signature and that
exact path. It is not Rust source or downstream call syntax. Rustdoc removes raw-identifier markers
from canonical paths, and the adapter may retain crate-relative type paths and crate-authored
parameter names. ASCII whitespace is collapsed so ordinary multiline `where` predicates remain
representable. Malformed input, an unsupported format, a crate-target or target-triple mismatch,
an ambiguous path, a duplicate row, more than 100,000 rows, a context above 64 KiB, or Rustdoc JSON
above 32 MiB refuses the output instead of weakening completeness. Raw Rustdoc numeric IDs appear
in neither keys nor values.

The scanner needs no Rust-specific control to attach one of those values to visible documentation.
This projection assertion selects the producer row for `example::check`:

```json
{
  "document": "docs/api.md",
  "name": "check-signature",
  "projection": "code-text-v1",
  "sink": "previous-code",
  "source": {
    "kind": "record-value",
    "set": "rust/example/local-function-declarations",
    "key": "fn/example::check"
  }
}
```

The named document owns the ordinary projection sink immediately after the visible value:

````markdown
```text
pub fn example::check() -> bool
```
[amiss:check-signature]: <amiss:projection>
````

The `set` is the exact context name and the `key` is a producer-owned stable record identity. A
changed declaration becomes `projection-drift`; a missing row is proven absent only when the
producer says this scoped set is complete. To show the whole scoped API instead, use a
`record-set` source with the same `set` and `sorted-rows-v1`. That projection compares every
display value in byte-sorted order and refuses a partial set before judging visible rows. These are
the same generic projection controls used by any record producer; the scanner does not parse Rust
syntax or introduce a symbol-specific finding.

The sealed controls request remains the provider-authenticated intake. It decodes each supplied
envelope into the same closed model: malformed shapes are invalid controls requests, while digest,
context, ordering, and semantic checks remain the consumers' responsibility. The request schema
includes the full envelope definitions with local references, so it needs no external schema registry.
A controller plan may hold
candidate-independent templates such as an Intersphinx inventory set. A trusted acquisition may
instead return exact candidate-independent template bytes beside the repository and action roots.
The frozen plan names each acquisition identity, producer kind, producer identity, version, and
expected context exactly once. While building the sealed job, the controller strictly parses both
sets, requires every acquired template to match its planned identity and producer context, binds
the exact candidate and no source report itself, applies their one combined count limit, orders
them by payload digest, and rejects collisions. The engine repeats the context comparison from the
sealed pair before consuming the evidence. A missing, extra, malformed, stale, wrong-context,
duplicate, or oversized acquired set is runtime tampering, not absent evidence.

The same plan contract can freeze each provider workflow artifact as an acquisition source. Its
provider and repository, workflow identity, event, artifact name, sole payload member, producer
contract, and separate archive and payload byte ceilings are all plan-digest inputs. The only
candidate rule is exact equality with the authenticated provider run's candidate commit; it is not
repository-selectable policy. This is a controller trust primitive, not a claim that every provider
can fetch such artifacts: a lane must separately expose operator configuration and implement the
provider-specific authenticated acquisition before the expectation is usable.

Job construction also produces one canonical semantic-input audit value, capped by
`SEMANTIC_INPUT_ARTIFACT_BYTES` before base64 allocation. In payload-digest order it holds each
exact template byte stream, the exact canonical bound envelope, its acquisition identity when
acquired, and SHA-256 and semantic payload digests. Reconstructing candidate binding therefore does
not depend on mutable producer output. The bootstrap job exposes this value separately from the
frozen engine report.

Provider lanes retain that value beside an accepted report and publish its digest and
[authenticated locator](provider-artifacts.md), so audit does not depend on a producer workflow's
shorter artifact lifetime.

A successful report projects the accepted envelopes' payload and producer/input identities under
`controls.semantic_evidence`; the sealed bootstrap checks that projection against the request. An
inventory-backed external destination is already resolved evidence, so the external-probe plan does
not schedule it for a second network judgment.

Provider services can produce that set from controller-configured local `objects.inv` files. The
producer bounds both compressed and decoded bytes before the pinned `sphinx_inv` parser sees the
body, selects only `std:label` records, resolves their locations beneath an operator-owned HTTP(S)
base under the engine's URI grammar, and binds the exact inventory bytes, identity, and base into
its input digest. The resulting template is held once in the controller plan and receives the exact
candidate identity only while the sealed job is built. Fetching and caching remain deployment
concerns outside both the engine and the repository being checked; [provider
configuration](provider-controls.md) accepts only the bounded local result.

The controller's first site-build producer consumes an operator-owned identity containing the
exact repository `book.toml` path, publication prefix, optional locale, and optional version, plus
the exact post-preprocessor
[mdBook renderer context](https://rust-lang.github.io/mdBook/for_developers/backends.html) from
mdBook `0.5.4` and a caller-opened completed HTML output directory. It uses the renderer's `path`
and `source_path`, rather than reconstructing routes from `SUMMARY.md`, so the built page and its
original repository source, when one exists, remain distinct after preprocessing. The first
rendered chapter's independent `index.html` copy is read separately. Every other route follows the
HTML renderer's `.html` path rule beneath one trusted publication prefix, with URI path segments
encoded from the actual output names.

The producer reads only the rendered pages named by the context, with one 16 MiB context
ceiling and one 16 MiB aggregate HTML ceiling. A no-follow directory capability bounds every page
read. A WHATWG tokenizer extracts decoded `id` values and hyperlink destinations from the completed
HTML. The producer honors the document's first `base` URL when usable, retains only links to another
proved page under the same publication origin, and walks that bounded graph from the independent
`index.html` entrypoint. It emits the configured source root, its `SUMMARY.md` manifest, the
entrypoint, and the sorted set of reachable repository sources beside the sorted page anchors, then
binds the resolved renderer configuration, every exact page digest, and the navigation result into
the input digest. A plan independently freezes the site identity; evidence from another
configuration path, publication prefix, locale, or version is refused. The wrong mdBook version,
no HTML renderer, an escaping or non-text output or source path, a source without an output path,
duplicate route ownership, an unreadable page, an unrepresentable anchor or link, or an oversized
graph refuses the complete set. A rendered chapter with an output path and no `source_path` instead
becomes an explicitly unattributed generated route. Theme, preprocessor, and configuration effects
are therefore observed in their finished bytes without running any of them inside Amiss.

A trusted candidate acquisition may call this producer after an operator-owned build and return
the exact candidate-independent template bytes under its planned acquisition identity. The
controller, not the producer, binds the candidate. The GitHub provider lane can read an explicitly
configured workflow artifact through the App installation API; the Gitea-family and GitLab lanes
still expose no such source. The controller neither starts mdBook nor treats repository output or
cache state as authority. The acquisition boundary keeps candidate output out of a startup plan
and keeps the scanner from searching the repository for evidence.

Site evidence may be complete while its route graph is internally defective. Those defects become
ordinary `site-build-defect` findings: a broken redirect records its source, route, destination,
claim digest, and reason, while duplicate ownership records the route plus the sorted distinct
claim digests and repository sources. Generated routes have no repository source, so a duplicate
route's source list may be empty even when its claim list is not.

The bound-envelope schema and checked example are
[`scanner-semantic-evidence.schema.json`](https://github.com/HardMax71/amiss/blob/main/spec/scanner-semantic-evidence.schema.json)
and
[`scanner-semantic-evidence.json`](https://github.com/HardMax71/amiss/blob/main/spec/examples/scanner-semantic-evidence.json).
The candidate-free input has its own
[`scanner-semantic-template.schema.json`](https://github.com/HardMax71/amiss/blob/main/spec/scanner-semantic-template.schema.json)
and
[`scanner-semantic-template.json`](https://github.com/HardMax71/amiss/blob/main/spec/examples/scanner-semantic-template.json).
The record-set authoring input is checked by
[`scanner-record-set-input.schema.json`](https://github.com/HardMax71/amiss/blob/main/spec/scanner-record-set-input.schema.json)
and its
[`scanner-record-set-input.json`](https://github.com/HardMax71/amiss/blob/main/spec/examples/scanner-record-set-input.json)
example is required to reproduce the semantic-template example through the real writer.
The engine still executes no producer and treats no repository-controlled evidence as authority.
