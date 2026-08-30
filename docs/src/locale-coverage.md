# Locale coverage audits

A repository scan can bind the exact docs candidate that approved a later locale audit, but it
cannot infer which pages a documentation generator considers equivalent across locales. Routes are
publication outputs; they are not stable page identities. The locale coverage plan therefore names
one independently selected inventory producer and treats every page key it will emit as opaque.
It can also select the same immutable product-resource identity used by publication audits, without
turning a locale version label into a release heuristic.

The plan binds:

- the accepted scanner report payload digest;
- the docs repository, commit, tree, and full candidate identity;
- one site, source locale, target locale, channel, and optional version;
- an optional exact product resource URI and digest that both locale inventories must identify;
- the inventory producer identity, version, and plan-owned context digest; and
- the operator's coverage-policy identity, context digest, required-page rule, and authorized
  fallback classes and page scopes, plus whether exact target lineage is required.

Site and channel use the artifact-identity grammar. Locale and version labels use a broader bounded
identity grammar so a producer can retain spellings such as `de-DE`; Amiss compares them exactly.
It does not validate BCP 47, order versions, or infer fallback from a locale hierarchy. Source and
target locale must differ. The opaque version remains scope metadata and never substitutes for an
immutable product resource.

Page selectors have two closed forms. The coverage rule uses one selector: `all-source` means that
every key in the future complete source inventory is required in the target inventory; `named`
carries one nonempty, byte-sorted, duplicate-free set and makes only those source keys required.
Other source keys remain optional, while target keys outside the source inventory can still be
reported as orphaned. A page key is nonempty, control-free, and at most 4,096 UTF-8 bytes. It is a
generator-owned identity such as a canonical docname, never a path or route guessed by Amiss.

The policy also carries a byte-sorted set of fallback rules, unique by opaque class. Each class has
an `all-source` or `named` page selector describing exactly where that producer-defined fallback
mode is allowed. An empty set forbids all fallbacks. The plan chooses authorization; it does not
prove that a target page really came from a source resource.

`require_target_lineage` independently chooses whether every observed target-owned page must carry
exact source lineage. False preserves a coverage-and-fallback-only audit and ignores any supplied
target lineage. True makes missing lineage unproven and a mismatched lineage digest an exact
refutation. Fallback pages are excluded because their F02 origin already binds the exact current
source resource.

The optional plan product reuses publication's `PublicationResource` directly: one absolute URI
identifies the resource and its SHA-256 digest identifies the exact bytes. Null selects no product
alignment policy. Amiss does not derive this value from the channel, scope version, tag, timestamp,
or similarly spelled URL.

The contract intentionally contains no translation verdict or timestamp. A target page with the
same key can prove structural coverage only. Exact lineage can prove which normalized source
resource a target was based on; it cannot prove that the page was translated correctly, remains
semantically equivalent, or required a change. This contract uses a digest because the source
inventory supplies an exactly comparable digest. It does not accept an opaque revision without a
matching source-side revision identity.

The plan is a closed, 64 KiB, digest-bound JSON document. Unknown fields, malformed identities,
mixed Git object formats, repeated or unsorted named keys, oversized text, or a changed payload
refuse the whole document. The outer digest establishes integrity, not authority: repository
content must not choose the producer context or operator policy.

The matching evidence contract repeats the plan digest, docs candidate, locale scope, and producer
context, then carries separate source and target inventories. Each inventory has its own input
digest, nullable independently observed product resource, completeness bit, and byte-sorted map
from page key to exact resource digest. The producer context defines which normalized bytes those
digests identify. A null product says the authenticated producer could not establish that side's
release identity; it does not assert a mismatch.

Every observed target page also carries one closed origin. `target-resource` says the producer
observed a target-owned resource and carries either its exact `based_on_source_digest` or null when
the producer has no exact lineage assertion. `fallback` names an opaque producer-declared class and
the exact source resource digest from which the fallback was obtained. A separate fallback list
would make omission indistinguishable from target ownership. Requiring an origin on every target
instead forces one explicit producer claim; authority still comes from authenticated acquisition
outside the engine.

Completeness belongs to each side independently. A false value preserves the pages the producer
did observe, but absence from that inventory is not evidence that a page is absent from the locale.
Product availability is independent of page completeness: a complete page set may still have an
unproven product identity, and a partial page set may carry an exact product receipt.
The two inventories may carry at most 100,000 page rows combined inside one 16 MiB document. Page
keys are unique within each side; malformed digests, duplicate or unsorted keys, unknown fields,
and a changed payload refuse the whole receipt.

The engine reader establishes shape and integrity, not producer authority. Repeating a plan digest
does not establish that the independently repeated facts match that plan. The pure offline
assessment first requires the exact plan digest and selected producer. Foreign evidence remains
unproven; correctly bound docs or scope disagreement refutes the plan without comparing the foreign
inventories. Evidence acquisition outside the engine must therefore authenticate the selected
producer, while the selected producer context defines the meaning of its origin classification.

For matching facts, every page row in the assessment is proved by presence on one side and complete
absence on the other. A complete target can therefore prove that an observed required source page
is missing even when the source inventory is partial. A complete source can likewise prove that an
observed target page is orphaned when the target inventory is partial. Such a refutation is valid,
but `coverage.complete: false` says the reported rows may be only a lower bound. The assessment
matches only when its policy-scoped missing, orphan, named-source, and fallback checks are
exhaustive and clean. A named policy can be exhaustive without an unrelated full source inventory
when every named requirement and every target key has explicit source presence; `all-source`
always requires a complete source set. Page resource digest differences remain deliberately inert.

Every fallback assessment row retains its page key and class. `allowed` means one plan rule admits
that class and page and the declared source digest equals the observed source resource.
`unauthorized` and `source-mismatch` are exact refutations. If the source page is absent from a
partial source inventory, `source-unproven` keeps the whole result unproven; absence cannot become a
digest mismatch until the source inventory is complete. Allowed fallback is not a translation or
freshness verdict: it proves only the exact policy and provenance relation named by the contracts.

When target lineage is required, the assessment also checks every observed target-owned page whose
source row is available, including pages outside a named required-coverage set. `current` means its
declared based-on digest equals that current source resource; `stale` is the exact unequal case;
`unproven` means the producer supplied no exact based-on digest. A target absent from a complete
source set is already orphaned. A target absent from a partial source set remains covered by the
existing source-incomplete result, so the evaluator does not manufacture a lineage row without a
current source value to compare.

When the plan selects a product, the assessment compares each inventory's product independently
with that exact planned URI and digest. The source and target fields reuse the ordinary
`matched`/`refuted`/`unproven` verdict vocabulary. Matched means exact equality; refuted means a
different immutable resource was observed; unproven means that inventory supplied null. A product
mismatch is an exact refutation even if the other side is unavailable. If the plan product is null,
both supplied product receipts are deliberately ignored and the assessment product result is null.
This relation proves release identity only, not translation quality or deployment success; the
producer or future publication lane must authenticate how each inventory acquired it.

The assessment binds the exact evaluator, accepted report, plan, and optional evidence payload. Its
three byte-sorted key sets name policy keys absent from source, required source keys absent from
target, and target keys absent from source. A fourth byte-sorted set records every assessed
fallback, and a fifth records every assessed target lineage. The document is bounded to 16 MiB and
200,000 rows across those sets. A separate nullable product result records the source and target
resource verdicts. `coverage.complete` remains strictly about the page comparison, so the overall
assessment can be unproven solely because a product receipt is missing while coverage is complete.
Missing, unbound, wrong-producer, or otherwise insufficient evidence is unproven rather than clean.
The command and controller intake are not built yet.

The checked public contracts are
[`locale-coverage-plan.schema.json`](https://github.com/HardMax71/amiss/blob/main/spec/locale-coverage-plan.schema.json),
with the matching
[`locale-coverage-plan.json`](https://github.com/HardMax71/amiss/blob/main/spec/examples/locale-coverage-plan.json)
example, and
[`locale-coverage-evidence.schema.json`](https://github.com/HardMax71/amiss/blob/main/spec/locale-coverage-evidence.schema.json),
with its
[`locale-coverage-evidence.json`](https://github.com/HardMax71/amiss/blob/main/spec/examples/locale-coverage-evidence.json)
example, and
[`locale-coverage-assessment.schema.json`](https://github.com/HardMax71/amiss/blob/main/spec/locale-coverage-assessment.schema.json),
with its replayable
[`locale-coverage-assessment.json`](https://github.com/HardMax71/amiss/blob/main/spec/examples/locale-coverage-assessment.json)
example. Their strict readers and writers live in `amiss-wire`; generator-specific parsing and
authentication stay outside the engine crates.
