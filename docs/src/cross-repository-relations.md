# Cross-repository relations

Documentation and the implementation it describes may live in different repositories. A change to
either one can introduce drift while the other repository remains byte-for-byte unchanged. The
controller has the operator-owned registry and exact Git acquisition boundary needed to identify
and materialize those relations without letting repository content choose another repository,
credential, selector, limit, or publication target.

This is not yet a complete cross-repository checking lane. The shared service can load one bounded
operator file into an immutable registry, and the controller can bind the complete set of opaque
credential identities to caller-owned authorities. The service can also bind one opaque
operator-declared coordination identity to the exact relation owned by an authenticated delivery.
No provider service supplies that registry or constructs those authorities yet. Closed plan,
evidence, and assessment contracts represent the exact comparison selected after acquisition. The
provider-neutral Git layer can project
repository-backed sources from all four acquired snapshots, but no trusted record producer is
admitted yet. When a caller supplies a complete chain, the controller binds it to the accepted
trigger report and frozen operator transition, replays the assessment, retains the exact bytes
immutably, and can reconcile a claimed GitHub or Gitea-family destination. An authenticated live
GitLab policy job can instead consume its exact synchronous destination.

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
status names. Two relations also cannot own the same provider-instance, repository, and status-name
key, even through different integrations or credentials. Subject, destination, and relation order
is canonicalized before the private trigger index is exposed.

There is no mutation API. A successful construction owns immutable relation plans behind shared
references; changing operator configuration requires building and installing another complete
registry. A rejected construction exposes no partial index and cannot replace a live entry.

Credential routing is another atomic construction over that frozen registry. Every distinct opaque
credential identity must have exactly one caller-owned authority under the provider instance and
integration named by its subjects. One authority may cover several registered repositories under
that scope. Missing authorities, extra or repeated rows, and reuse of one identity under another
provider or integration are rejected. Lookup repeats the subject binding before returning the
authority, and neither the registry nor the router exposes a mutation API. The authority value can
directly contain its concrete provider client and Git acquisition credential; the controller does
not erase it behind another provider trait or interpret its secret bytes.

## Authenticated triggering

Both subjects are trigger owners by construction. This removes a configuration branch that could
silently check changes from only one side.

Lookup accepts an `AuthenticatedDelivery`, not a repository path, URL, webhook body, or policy
file. Its key is the authenticated provider instance, integration, repository identity, and object
format. Provider facts that disagree inside the delivery are an error. A coherent delivery outside
the registry is ordinary authenticated no-work. A matching delivery returns every affected
relation in stable relation-identity order together with the role that triggered it.

Coordination admission consumes the authenticated delivery and accepts only an operator-declared
relation among that exact trigger set. Its result keeps the delivery, frozen relation, trigger role,
and bounded opaque coordination identity together for execution. An unknown relation or an
internally inconsistent delivery is an error. The service does not expose a coordination-policy
enum or derive identity from a commit, timestamp, branch name, URL, or provider event spelling.

The configured target branch is not treated as an immutable revision. A provider resolver must
refresh it and supply exact base/candidate commit and tree IDs for each role. The controller freezes
all four revisions against the registered roles and object formats before acquisition. The same
trusted call supplies one bounded coordination identity naming the exact pair, release, or workflow
occurrence. The relation configuration and its context digest define what that identity means;
Amiss treats the spelling as opaque. Commit timestamps, nearby branch heads, URL versions, and
repository prose are not pairing evidence.

Status preparation requires a fresh head fact for both subjects, not only the repository whose
delivery triggered the audit. Each fact must reproduce the complete registered subject, including
its provider scope, target, credential identity, selector, and limits. A changed subject binding or
object format is invalid; a changed candidate commit is superseded. Only then does the controller
freeze the configured destination roles into a stable batch carrying the relation, coordination,
trigger role, pending fence, exact provider scope and credential identity, candidate commit, and
required status name. Selector, target-branch, and resource-limit fields have already served their
finality proof and are not copied into the provider outbox.
An unconfigured role never appears in that batch.

## Pending and supersession law

The provider-neutral scheduler is a pure transition over an optional pending value and one newly
frozen relation transition. The first exact value receives fence 1. Repeating the same operator
plan, coordination identity, and four subject snapshots is a duplicate and preserves the original
pending value, even when the other authenticated role triggered the repeat. A coordination identity
cannot be rebound to different snapshots, and a relation identity cannot be rebound to different
operator configuration.

A different coordination identity under the same relation advances the fence and becomes the new
pending value. A worker holding the earlier fence is therefore superseded, while an audit it already
retained remains immutable under its own artifact identity. Fence overflow fails closed. This model
contains no clock and gives no lexical meaning to coordination identities.

The file-backed admission store applies that law under one cross-process lock. An atomically
replaced committed head bounds a hash-chained append-only journal, so restart either observes the
whole new binding or discards its uncommitted suffix. Every admitted coordination remains bound to
its first exact work and fence: a delayed retry returns that historical fence but cannot become
current again. New work appends one bounded record instead of rewriting all history, the immutable
capacity applies only to new bindings, and a missing, shortened, reordered, rebound, or malformed
committed record fails closed. The journal retains full configuration and work digests rather than
credential references or complete operator configuration. Head-final status preparation is pure
and does not authorize an external write: a later durable publisher must still stage the exact
batch while proving its fence is current. Invoking either boundary from a live relation lane
remains a separate stage.

The pure staging transition models that next boundary without choosing its disk format. It accepts
only the current pending fence and fresh two-subject head facts, replays the complete relation audit
against the pending transition, and binds its retained artifact reference and verdict to the exact
destination batch. A first stage returns one direct record. An unfinished exact retry returns that
same record, a completed exact retry returns no work, and substituting any target or audit field is
a binding conflict. Completion changes only the terminal bit and is idempotent for the exact staged
value. The file-backed relation journal applies that transition under the same cross-process lock
as scheduling, so current-fence verification and the committed stage cannot race. The stage action
first verifies the live retained artifact, then stores the relation identity, coordination, trigger
role, fence, artifact identity, and one domain-separated binding over the complete typed target and
audit record. It does not duplicate provider configuration or credential identities in the journal.
Each configured external destination is retained in the staged action as a domain-separated digest
of its stable provider-instance, repository, and status-name identity. After a provider accepts or
reconciles the exact staged value, a separate hash-chained action acknowledges that destination.
A foreign or repeated destination fails closed, and batch completion is refused until every staged
destination has a durable acknowledgement. Completion is itself a separate hash-chained action;
an exact retry is idempotent, while a missing or rebound record fails closed. Completed in-memory
state keeps only the status binding digest.

Restart recovery now performs that reopening without persisting a second provider configuration.
It reads the bounded retained relation audit by artifact identity, reconstructs the frozen transition
from its exact snapshots and the immutable registry, replays the audit, and returns the staged batch
only when the complete status binding is identical. A missing or rebound registry, expired artifact,
or changed target remains a refusal. Reopening still grants no external delivery authority.

Delivery claims use 256 deterministic operating-system lock shards derived from those stable
destination digests. For every destination, selection retains only its lowest unresolved fence;
candidates sharing a shard are tried in stable fence, relation, and coordination order. The shard
is acquired without holding the journal lock, then the journal is synchronized and selection is
repeated before the registry and artifact are reopened. This lock order lets acknowledgements take
the journal lock without deadlock. A newer coordination cannot reach the same destination first,
while unrelated shards may proceed in parallel. A hash collision can reduce concurrency but cannot
change selection or authority.

The returned claim directly carries the exact reopened status record and one target while a private
file handle keeps its shard locked. Dropping it, including process failure, changes no durable state;
the next attempt selects the same oldest unacknowledged destination. Acknowledgement consumes the
claim and appends only after provider acceptance or provider-specific reconciliation. The final
acknowledgement also appends completion under the journal lock. If failure lands between those two
commits, the next claim pass completes the fully acknowledged batch before selecting more provider
work.

The outbox still makes no provider call. Provider adapters must reconcile an ambiguous response for
the exact claimed value before acknowledging it, and a live relation lane must continue polling
until the outbox has no claim.

The GitHub installation client can independently refresh the final head of an operator-configured
GitHub subject. It accepts the typed subject directly, requires the configured provider and
installation, a canonical repository on that provider instance, and SHA-1, then resolves the
declared branch and validates both the returned commit and its tree. The result is a typed
`RelationSubjectHead`; it does not choose a credential, schedule work, or authorize publication.
Credential routing and the two-subject finality decision remain controller-lane responsibilities.

The same client accepts one exact status record and one destination from a durable delivery claim.
It rejects a completed or malformed batch, a target not present exactly once, another provider or
installation, a noncanonical repository, a non-SHA-1 candidate, or an inconsistent relation audit
before provider I/O. The check is attached to the subject candidate commit under the
operator-configured relation status name. `aligned` and `resolved-drift` conclude `success`;
`introduced-drift`, `pre-existing-drift`, and `unproven` conclude `failure`. Its credential-free
summary binds the relation, coordination, fence, roles, target, verdict, and report, plan, evidence,
and assessment digests. A domain-separated digest of that complete projection is the external ID.

Before creating anything, the client lists the App-owned checks for that exact commit and name. It
reuses one exact external-ID and output match, creates only when none exists, and rejects duplicate
or conflicting matches. A lost create response therefore leaves the claim unacknowledged; a retry
normally reconciles the accepted run before the caller durably acknowledges the destination. The
GitHub API and local journal still have no shared transaction, so a stale provider read can expose a
duplicate later and make subsequent reconciliation fail closed.

The Gitea-family client independently refreshes an operator-configured subject through the
authenticated commit endpoint. It requires the configured provider and dedicated reviewer, a flat
canonical repository on that provider instance, and SHA-1 before resolving the declared branch.
Both the returned commit and tree must be exact SHA-1 names. The result is the same typed
`RelationSubjectHead` used by provider-neutral finality; full object acquisition remains a separate
bounded Git operation.

The client projects the same checked, credential-free value onto the exact candidate commit through
the native commit-status API. It applies the same scope requirements before listing statuses. The
list is bounded to 1,000 rows and ordered by the provider's commit-status index. For the configured
context, the client reuses an exact latest row, advances a different valid Amiss marker written by
the same reviewer, and rejects a foreign, malformed, or same-marker conflict. A create succeeds only
when the provider echoes every field and the exact reviewer.

The short versioned description carries a domain-separated digest of the same complete projection;
the status state carries success or failure. Gitea and Forgejo do not bind a required status context
to its writer, so checking the response's reviewer protects Amiss reconciliation but cannot make the
provider merge rule identity-secure. Use this surface for an unchanged relation subject only with
that limitation understood; publish the actual protected gate on an identity-bound destination when
one is required.

GitLab has no asynchronous relation publisher. Its adapter accepts an already authenticated policy
job and a relation subject only when both name the configured project, integration, SHA-1 format,
and protected target branch. A fresh provider refresh must still prove that exact job, pipeline,
merge-train car, candidate, runner, policy origin, project controls, and branch protection active.
The resulting head fact is the ephemeral merge-train candidate, not the mutable protected-branch
head.

The same live-job boundary can consume one staged relation destination. Its scope and candidate must
match the authenticated delivery, and its status name must be the exact configured policy job name.
The shared relation projection supplies only a pass or block decision; another final refresh must
still find the job and train active before the caller lets the endpoint return success. No GitLab API
write occurs. A stopped job cannot be resumed by a background publisher, and a lost success response
still fails the provider job closed even if Amiss retained the completed local result.

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

1. construct concrete provider authorities and install the frozen registry and router in a service;
2. resolve and acquire snapshot-bound records from the admitted coordination;
3. assemble and resume the complete relation lifecycle in live provider lanes.

Until those stages exist, the projector can prove an exact bounded repository comparison when its
caller supplies the frozen transition, checked plan, and two acquired roots. The GitHub adapter can
deliver an already retained and claimed result, but no provider service yet assembles the complete
relation lifecycle.

The implementation is in the
[provider-neutral relation registry](https://github.com/HardMax71/amiss/blob/main/controller/src/relations.rs),
the [bounded registry loader](https://github.com/HardMax71/amiss/blob/main/controller/service/src/config/relation.rs),
the [exact relation transport](https://github.com/HardMax71/amiss/blob/main/controller/git/src/relation.rs),
the [repository projector](https://github.com/HardMax71/amiss/blob/main/crates/amiss-scan/src/projection/repository.rs),
the [durable scheduler](https://github.com/HardMax71/amiss/blob/main/controller/src/relations/store.rs),
the [relation laws](https://github.com/HardMax71/amiss/blob/main/controller/tests/suite/relations.rs),
and the [durable scheduling laws](https://github.com/HardMax71/amiss/blob/main/controller/tests/relation_schedule_store.rs)
exercise bidirectional selection, stable ordering, four-revision binding, independent roots,
projection compatibility, joint budgets, exact destinations, restart recovery, delayed retries,
capacity, corruption, and concurrent admission.
