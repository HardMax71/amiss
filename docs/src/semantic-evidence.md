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

Only the sealed controls request has this intake. The public command supplies an empty set. A
successful report projects the accepted envelopes' payload and producer/input identities under
`controls.semantic_evidence`; the sealed bootstrap checks that projection against the request. An
inventory-backed external destination is already resolved evidence, so the external-probe plan does
not schedule it for a second network judgment.

The schema and checked example are
[`scanner-semantic-evidence.schema.json`](https://github.com/HardMax71/amiss/blob/main/spec/scanner-semantic-evidence.schema.json)
and
[`scanner-semantic-evidence.json`](https://github.com/HardMax71/amiss/blob/main/spec/examples/scanner-semantic-evidence.json).
The site-build consumers remain future observation grammars over this same boundary; the engine
still executes no producer and treats no repository-controlled evidence as authority.
