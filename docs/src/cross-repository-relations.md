# Cross-repository relations

Documentation and the implementation it describes may live in different repositories. A change to
either one can introduce drift while the other repository remains byte-for-byte unchanged. The
controller has the operator-owned registry and exact Git acquisition boundary needed to identify
and materialize those relations without letting repository content choose another repository,
credential, selector, limit, or publication target.

This is not yet a complete cross-repository checking lane. No service loads this configuration or
resolves foreign provider heads and credentials yet. Closed plan, evidence, and assessment
contracts represent the exact comparison selected after acquisition. The provider-neutral Git
layer can project repository-backed sources from all four acquired snapshots, but no trusted
record producer is admitted yet, and no controller lane schedules the audit or publishes a status.
When a caller supplies a complete chain, the controller binds it to the accepted trigger report
and frozen operator transition, replays the assessment, and retains the exact bytes immutably.

## One closed relation

One relation contains exactly two subjects and one of the existing projection kinds:
`code-text-v1`, `sorted-rows-v1`, or `decimal-count-v1`. Reusing the scanner's projection vocabulary
keeps copied text, exact inventories, and counts under one comparison model instead of adding a
second selector language.

Each subject carries:

- a relation-local role;
- the exact provider family and instance, integration, and canonical repository identity;
- the operator-selected target branch and Git object format;
- an opaque credential reference, never credential bytes;
- one projection source checked through the scanner-policy source grammar; and
- independent acquisition-object, acquisition-byte, projection-record, and projection-byte limits.

The relation adds an identity, aggregate limits for the same four resources, and one or two status
destinations. A destination names one of the two subject roles and a status name under the existing
required-status grammar. The same credential reference may be configured for both subjects when a
provider genuinely issues one appropriately scoped identity; the registry still retains the choice
separately on each subject.

Construction is atomic. It rejects more than 1,024 relations, repeated relation identities, equal
subject roles, equal repository identities, malformed or projection-incompatible sources, zero or
overflowing limits, aggregate limits that cannot admit either subject or exceed both subject
ceilings together, missing or repeated destinations, foreign destination roles, and malformed
status names. Subject, destination, and relation order is canonicalized before the private trigger
index is exposed.

There is no mutation API. A successful construction owns immutable relation plans behind shared
references; changing operator configuration requires building and installing another complete
registry. A rejected construction exposes no partial index and cannot replace a live entry.

## Authenticated triggering

Both subjects are trigger owners by construction. This removes a configuration branch that could
silently check changes from only one side.

Lookup accepts an `AuthenticatedDelivery`, not a repository path, URL, webhook body, or policy
file. Its key is the authenticated provider instance, integration, repository identity, and object
format. Provider facts that disagree inside the delivery are an error. A coherent delivery outside
the registry is ordinary authenticated no-work. A matching delivery returns every affected
relation in stable relation-identity order together with the role that triggered it.

The configured target branch is not treated as an immutable revision. A provider resolver must
refresh it and supply exact base/candidate commit and tree IDs for each role. The controller freezes
all four revisions against the registered roles and object formats before acquisition. The same
trusted call supplies one bounded coordination identity naming the exact pair, release, or workflow
occurrence. The relation configuration and its context digest define what that identity means;
Amiss treats the spelling as opaque. Commit timestamps, nearby branch heads, URL versions, and
repository prose are not pairing evidence.

## Exact Git acquisition

The existing strict HTTPS protocol-v2 shallow fetch now accepts a positive object and pack-byte
ceiling and returns its measured pack usage. Caller ceilings can only narrow Amiss's global
2,000,000-object and 2 GiB pack ceilings. The streaming pack validator enforces the selected limits
before indexing, so an oversized response is never accepted and counted afterward.

Relation acquisition sorts inputs by role, binds each canonical HTTPS repository URL and opaque
credential identity back to the operator plan, and fetches both exact commits for each subject into
its own root. The first subject's measured usage is subtracted from the aggregate budget; the
second receives the smaller of its own ceiling and what remains. Both roots are then reopened by
the bounded repository reader, and every commit must name the independently resolved tree.

Any missing object, transport failure, cancellation, exhausted budget, wrong tree, or aliased root
makes the complete relation `unproven`; no partial relation result is returned. Roots must remain
physically distinct even when two independent repositories happen to produce identical Git object
IDs. SHA-256 subjects remain representable in the registry but are unproven through the current
SHA-1-only provider transport.

## Report-bound audit plan

After the four snapshots are frozen, a separate 64 KiB wire plan records the exact comparison that
later evidence must reproduce. It contains:

- the accepted scanner report payload digest and the role whose authenticated change selected it;
- the relation identity and a context digest for the complete operator-owned configuration;
- the exact operator-supplied coordination identity;
- one shared projection kind; and
- exactly two role-sorted subjects, each with its canonical repository, selected target branch,
  compatible source selector, object format, and exact base/candidate commit and tree IDs.

The coordination identity records intent and the four object pairs identify its exact comparison;
neither is derived from the other. It does not impose an order, deadline, or lifecycle by itself.
The target branch remains explanatory selection context. Base and candidate may be identical for
an unchanged subject, and the two subjects may use different object formats. Repository identities
and roles must differ, and the trigger role must name one subject. Credentials, raw provider tokens,
and transport budgets are deliberately not copied into the portable document.

The checked writer and strict reader share the scanner's existing projection-source grammar. A
`code-text-v1` plan therefore accepts blob lines, a named region, or one record value;
`sorted-rows-v1` and `decimal-count-v1` accept tree paths or one record set. The plan does not add a
second selector language or let either repository change the operator-owned selector.

The context digest is an integrity binding for operator configuration interpreted outside the wire;
it is not authority by itself. Likewise, the report digest and trigger role are closed facts, but
the wire reader does not have the report and cannot prove they agree with it. Controller admission
performs that binding before retaining the sidecar. The plan intentionally contains no
projected value, completeness claim, alignment verdict, blame assignment, or status policy.
Malformed branches, selectors, identities, object IDs, subject ordering, unknown fields, and a
changed payload refuse the whole document.

## Projection evidence

A second closed 64 KiB document binds projection evidence to the exact plan payload digest. It has
the same two byte-sorted role rows, each with independent `base` and `candidate` slots. A slot is
either null or one complete projected value:

- `value_digest` is the plain SHA-256 of the exact canonical projected bytes; and
- `value_bytes` is the nonnegative safe-integer length of those same bytes.

Null means that producer did not establish one complete value for that exact slot. It does not mean
an empty value, a missing source, or a mismatch, and it cannot participate in an equality claim.
There is no `complete` flag that can disagree with nullable digest fields and no partial projected
value whose absence claims would be ambiguous. All four slots may independently remain null; a
missing evidence document remains distinct from a present receipt that records four unproven
attempts.

The projection kind in the plan defines the canonical bytes. For `code-text-v1`, blob-line and
named-region selections normalize CR and CRLF to LF and remove one terminal LF; record values use
their exact UTF-8 value bytes. `sorted-rows-v1` byte-sorts the complete selected rows and joins them
with one LF and no trailing LF. `decimal-count-v1` uses the canonical ASCII decimal item count
without leading zeroes. The compact receipt does not copy potentially multi-megabyte values merely
to compare them twice.

After acquisition, the Git projector rebuilds the plan, binds every repository, selector, commit,
and tree back to the frozen operator relation, and reopens the two physically independent roots.
It visits roles in canonical order and each role's base before its candidate. Each snapshot receives
the smaller of the subject budget and aggregate budget that remains. Blob-line and named-region
sources count one selected record and charge the larger of the source blob or canonical projected
value. Tree-path sources count every selected path and charge the larger of their combined path
bytes or canonical output. A crossed budget or untrusted Git object refuses the complete operation;
a missing, unsupported, or incomplete repository source records a null slot. Record-value and
record-set slots also remain null until a separately authenticated, snapshot-bound producer exists.

The evidence reader establishes shape and payload integrity only. Repeating a plan digest is not
producer authority, and matching role spellings are not checked until the plan and evidence are
assessed together. Unknown fields, malformed digests or roles, reordered or repeated rows, unsafe
byte counts, and a changed payload refuse the whole receipt. The evidence contract carries no
verdict and never identifies which subject should change.

## Equality transition

A third closed 64 KiB document records the deterministic offline assessment. It binds the accepted
report, exact plan, optional evidence, and evaluator version and digest. Before comparing values,
the evaluator rebuilds both supplied envelopes, requires the evidence to name the exact plan, and
requires its two role rows to match the plan. Any absent evidence, foreign plan digest, mismatched
role, or null projection slot yields `unproven` with one corresponding reason.

For four complete slots, projected values are equal only when both their digest and byte length are
equal. The two booleans map to exactly one transition:

| Base equal | Candidate equal | Verdict |
| --- | --- | --- |
| yes | yes | `aligned` |
| yes | no | `introduced-drift` |
| no | no | `pre-existing-drift` |
| no | yes | `resolved-drift` |

These names describe equality over time, not correctness. Both roles participate symmetrically;
the assessment neither chooses an authority nor says which repository should change. A proved
transition carries a null reason, while `unproven` carries exactly one of `evidence-absent`,
`evidence-unbound`, `role-mismatch`, or `projection-unproven`. Inconsistent verdict/reason/evidence
combinations and a changed payload refuse the assessment instead of being normalized.

The assessment is replayable over the exact bound documents, but the portable evidence document
does not authenticate its producer by itself. The acquisition projector establishes repository
slots under the operator-owned relation and resource limits. Before storage, the controller
reopens the accepted scanner report, binds its repository, target, and exact snapshots to the
trigger role, rechecks the operator plan, and independently replays the evidence and assessment.
The artifact store retains the report, plan, optional evidence, and assessment under one immutable,
restart-safe identity; retries may reproduce the same bytes but cannot substitute any component.

## Trust boundary

The registry lives only in the unpublished controller layer. The offline engine still has one
declared repository root, no network or async dependency, no credential input, and no ability to
follow a link into another repository. Projection sources in this registry are operator input;
similarly shaped repository policy does not add a relation or gain access to its credential
reference.

The remaining stages are deliberately separate:

1. admit snapshot-bound record values and sets from an authenticated producer;
2. durably schedule coordination identities and fence superseded work without timestamp inference;
   and
3. deduplicate triggers from either provider route and publish only to the configured subject roles.

Until those stages exist, the projector can prove an exact bounded repository comparison when its
caller supplies the frozen transition, checked plan, and two acquired roots. No provider lane yet
claims that it performed or approved that comparison.

The implementation is in the
[provider-neutral relation registry](https://github.com/HardMax71/amiss/blob/main/controller/src/relations.rs),
the [exact relation transport](https://github.com/HardMax71/amiss/blob/main/controller/git/src/relation.rs),
the [repository projector](https://github.com/HardMax71/amiss/blob/main/crates/amiss-scan/src/projection/repository.rs),
and the [relation laws](https://github.com/HardMax71/amiss/blob/main/controller/tests/suite/relations.rs)
exercise bidirectional selection, stable ordering, four-revision binding, independent roots,
projection compatibility, joint budgets, and exact destinations.
