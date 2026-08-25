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
- the producer kind, stable implementation identity, version, and the kind-defined digest of all
  configuration, inventories, or completed-build input;
- whether the producer completed that exact input;
- at most 100,000 observation objects, sorted by canonical JSON and unique.

The envelope carries the domain-separated payload digest and is limited to 16 MiB. Its strict
reader refuses malformed JSON, unknown envelope fields, invalid identities, duplicate or unsorted
observations, oversized input, and a mismatched digest. Construction sorts observations once so a
filesystem, inventory, or build traversal order cannot change the evidence identity.

Observation vocabularies do not share a synthetic universal graph. An Intersphinx producer needs
domain, role, object name, inventory identity, and URI. A site-output producer needs routes,
anchors, redirects, navigation edges, and source attribution. The envelope requires only a bounded
`kind` on every observation; a compiled consumer owns the closed grammar and judgment for kinds it
recognizes. An unknown kind therefore remains inert data. Parsing the envelope never turns it into
a pass, a block, or a suppression.

This contract authenticates nothing by itself. Provider-enforced use must acquire it outside the
repository and bind its expected digest through a trusted sealed input. A repository file, cache
entry, or self-asserted local producer cannot promote its own observations to authority. Partial
evidence may prove a fact positively only where a later kind contract permits it; absence can carry
meaning only for a declared complete set over the exact input digest.

The first compiled consumer accepts one complete `sphinx-inventory-set` producer at version `1`,
with no source-report binding. A `sphinx-label` observation carries an inventory identity, a
Docutils-normalized label, and one syntactically valid absolute HTTP(S) destination. The engine uses
that table only after every envelope in the controls request matches the exact candidate identity.
One unique prefixless `:ref:` label resolves through the inventory; repeated labels across
inventories remain ambiguous, colon-prefixed names remain unsupported, and local declarations keep
precedence. Missing evidence, an incomplete producer, another producer version, a stale candidate
binding, or an invalid observation can never clear a missing label.

The second compiled consumer accepts at most one complete `site-build` producer at version
`0.1.0`. A `site-route` observation carries one exact absolute-path URI, one repository source
document, and a byte-sorted unique set of decoded anchor identities. Routes exclude authority,
query, and fragment components; sources obey the repository-path grammar; anchors and their
aggregate count are bounded. On the candidate side only, an exact route resolves to its scanned
structured source, and a nonempty fragment additionally requires an exact member of the published
anchor set. Query text remains identity data. A route absent from the evidence, an absent anchor,
duplicate ownership, a missing or unscanned source, and image use remain unsupported rather than
being guessed into either a pass or a failure. A `site-redirect` observation maps one exact redirect
route to its exact terminal route, not an intermediate hop. The destination may carry a fragment
but no query. It resolves only when that terminal route has one source-backed `site-route` and the
effective fragment is in its anchor set. Following the
[HTTP Location rule](https://www.rfc-editor.org/rfc/rfc9110.html#section-10.2.2), an absent
destination fragment inherits the authored fragment, a nonempty one replaces it, and an empty `#`
suppresses inheritance. Self-redirects, malformed fragments, and missing or ambiguous terminal
routes remain invalid or unsupported rather than becoming guessed passes. The base side never
consumes candidate build output.

Only the sealed controls request has this intake. The public command supplies an empty set. A
controller plan may hold candidate-independent templates such as an Intersphinx inventory set. A
trusted acquisition may instead return already-formed pre-scan envelopes beside the exact
repository and action roots. While building the sealed job, the controller strictly parses both
sets, requires every envelope to name the exact candidate and no source report, applies their one
combined count limit, orders them by payload digest, and rejects collisions. A malformed, stale,
post-scan, duplicate, or oversized acquired set is runtime tampering, not absent evidence.

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

The controller's first site-build producer consumes the exact post-preprocessor
[mdBook renderer context](https://rust-lang.github.io/mdBook/for_developers/backends.html) from
mdBook `0.5.4` and a caller-opened completed HTML output directory. It uses the renderer's `path`
and `source_path`, rather than reconstructing routes from `SUMMARY.md`, so the built page and its
original repository source remain distinct after preprocessing. The first rendered chapter's
independent `index.html` copy is read separately. Every other route follows the HTML renderer's
`.html` path rule beneath one trusted publication prefix, with URI path segments encoded from the
actual output names.

The producer reads only those source-backed pages named by the context, with one 16 MiB context
ceiling and one 16 MiB aggregate HTML ceiling. A no-follow directory capability bounds every page
read. A WHATWG tokenizer extracts decoded `id` values from the completed HTML, then the producer
sorts and deduplicates each page's anchors and binds every exact page digest into the input digest.
The wrong mdBook version, no HTML renderer, an escaping or non-text path, generated content without
`source_path`, duplicate route ownership, an unreadable page, or an unrepresentable anchor refuses
the complete set. Theme, preprocessor, and configuration effects are therefore observed in their
finished bytes without running any of them inside Amiss.

A trusted candidate acquisition may call this producer after an operator-owned build and return
the already candidate-bound envelope. The built-in provider acquisitions still return none: the
controller neither starts mdBook nor treats repository output or cache state as authority. The
acquisition boundary keeps candidate output out of a startup plan and keeps the scanner from
searching the repository for evidence.

The schema and checked example are
[`scanner-semantic-evidence.schema.json`](https://github.com/HardMax71/amiss/blob/main/spec/scanner-semantic-evidence.schema.json)
and
[`scanner-semantic-evidence.json`](https://github.com/HardMax71/amiss/blob/main/spec/examples/scanner-semantic-evidence.json).
Generated pages without source attribution, redirect defects, locales, versions, and navigation
remain future observation grammars over this same boundary. The engine still executes no producer
and treats no repository-controlled evidence as authority.
