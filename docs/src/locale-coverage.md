# Locale coverage audits

A repository scan can bind the exact docs candidate that approved a later locale audit, but it
cannot infer which pages a documentation generator considers equivalent across locales. Routes are
publication outputs; they are not stable page identities. The locale coverage plan therefore names
one independently selected inventory producer and treats every page key it will emit as opaque.

The plan binds:

- the accepted scanner report payload digest;
- the docs repository, commit, tree, and full candidate identity;
- one site, source locale, target locale, channel, and optional version;
- the inventory producer identity, version, and plan-owned context digest; and
- the operator's coverage-policy identity, context digest, and required-page rule.

Site and channel use the artifact-identity grammar. Locale and version labels use a broader bounded
identity grammar so a producer can retain spellings such as `de-DE`; Amiss compares them exactly.
It does not validate BCP 47, order versions, or infer fallback from a locale hierarchy. Source and
target locale must differ.

The required-page rule has two closed forms. `all-source` means that every key in the future
complete source inventory is required in the target inventory. `named` carries one nonempty,
byte-sorted, duplicate-free set and makes only those source keys required. Other source keys remain
optional, while target keys outside the source inventory can still be reported as orphaned. A page
key is nonempty, control-free, and at most 4,096 UTF-8 bytes. It is a generator-owned identity such
as a canonical docname, never a path or route guessed by Amiss.

This contract intentionally contains no fallback bit, translation verdict, timestamp, or source
lineage. A target page with the same key can prove structural coverage only. It cannot prove that
the page was translated, is current, or is semantically equivalent. Fallback requires separately
authenticated provenance, and staleness requires an exact producer-owned `based_on` source digest
or revision; neither can be manufactured from this plan.

The plan is a closed, 64 KiB, digest-bound JSON document. Unknown fields, malformed identities,
mixed Git object formats, repeated or unsorted named keys, oversized text, or a changed payload
refuse the whole document. The outer digest establishes integrity, not authority: repository
content must not choose the producer context or operator policy.

The matching evidence contract repeats the plan digest, docs candidate, locale scope, and producer
context, then carries separate source and target inventories. Each inventory has its own input
digest, completeness bit, and byte-sorted map from page key to exact resource digest. The producer
context defines which normalized bytes those digests identify. Matching keys prove only structural
presence; different page digests do not prove either drift or translation.

Completeness belongs to each side independently. A false value preserves the pages the producer
did observe, but absence from that inventory is not evidence that a page is absent from the locale.
The two inventories may carry at most 100,000 page rows combined inside one 16 MiB document. Page
keys are unique within each side; malformed digests, duplicate or unsorted keys, unknown fields,
and a changed payload refuse the whole receipt.

The engine reader establishes shape and integrity, not producer authority. Repeating a plan digest
does not establish that the independently repeated facts match that plan. The plan and inventory
evidence contracts exist today; the offline assessment, command, and controller intake do not. No
missing or orphan verdict exists until that assessment applies the plan policy while respecting
both completeness bits.

The checked public contracts are
[`locale-coverage-plan.schema.json`](https://github.com/HardMax71/amiss/blob/main/spec/locale-coverage-plan.schema.json),
with the matching
[`locale-coverage-plan.json`](https://github.com/HardMax71/amiss/blob/main/spec/examples/locale-coverage-plan.json)
example, and
[`locale-coverage-evidence.schema.json`](https://github.com/HardMax71/amiss/blob/main/spec/locale-coverage-evidence.schema.json),
with its
[`locale-coverage-evidence.json`](https://github.com/HardMax71/amiss/blob/main/spec/examples/locale-coverage-evidence.json)
example. Their strict readers and writers live in `amiss-wire`; generator-specific parsing and
authentication stay outside the engine crates.
