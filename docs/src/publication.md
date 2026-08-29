# Publication audits

A repository scan proves facts about one repository candidate. It does not prove that a completed
site was deployed, that a public channel serves that site, or that the site describes the intended
product release. Those are separate publication facts acquired after the scan and often after a
deployment.

The publication plan is the operator-owned half of that later audit. It binds one intended relation
between:

- the payload digest of the scanner report the operator accepted;
- the docs repository, commit, tree, and full scanner candidate identity;
- the deployment provider instance, environment, channel, and canonical HTTPS URL;
- the immutable completed-site artifact and the site producer input digest derived from it;
- the exact product resource URI and digest;
- the independently selected deployment evidence producer and its context; and
- the operator's named docs-to-product relation rule and its context digest.

Channel names such as `stable`, `latest`, or `next` are opaque policy labels. Amiss does not sort
versions, compare timestamps, select a tag, or infer that two similarly named resources belong
together. The product and site URI spellings are identifiers; their SHA-256 digests establish the
exact bytes. A mutable URL or tag without its resource digest is not a publication identity.

The plan is a closed, 64 KiB, digest-bound JSON document. Git object IDs must match their declared
object format. The canonical public URL is HTTPS without a query or fragment. Resource URIs use one
exact lowercase absolute scheme and carry no fragment. Provider, channel, environment, producer,
and relation identities use the bounded artifact-identity grammar. Unknown fields, invalid values,
mixed Git formats, oversized input, or a changed payload refuse the whole document.

The matching publication evidence is a provider-normalized receipt for one successful terminal
deployment. It binds the exact plan payload digest and independently repeats the observed docs
candidate, target, completed-site artifact, and product resource. It also records:

- the selected evidence producer identity, version, and context digest;
- the provider deployment record as an immutable URI-and-digest resource;
- the exact workflow or deployment definition as another immutable resource; and
- the one-based provider run attempt that distinguishes reruns.

Only `succeeded` is a receipt outcome. A failed, cancelled, pending, partial, or unauthenticated
provider response cannot be encoded as successful publication evidence. Repeating the publication
facts is intentional: the later offline assessment compares independently acquired facts with the
plan rather than trusting a producer that merely echoes a plan digest. The provider adapter must
authenticate its API, attestation, or receipt before normalization; engine crates perform no
network or signature work.

The offline assessment has three outcomes. `matched` means the bound receipt came from the planned
producer context and its docs, target, site, and product facts all equal the plan. `refuted` means
that trusted receipt disagrees with at least one of those four fact groups, each named in a sorted
reason set. `unproven` means there was no receipt, it answered another plan, or it came from another
producer context. Foreign or absent evidence can never refute a plan.

The assessment binds the accepted report, plan, optional evidence, and exact evaluator binary by
digest. A missing receipt is represented by a null evidence digest and the single
`evidence-absent` reason. Malformed, failed, pending, partial, or mutable-only provider material
never becomes a typed successful receipt; callers retain that acquisition failure and assess the
plan as unproven instead of manufacturing a negative fact.

The outer payload digests are integrity checks, not signatures or authority claims. Repository
content must not choose the plan, producer context, relation rule, or credentials. A future
controller lane will retain the operator-selected plan beside its provider-authenticated evidence
and offline assessment. No scanner command or engine report consumes these publication documents
yet, and a plan alone never says a deployment happened.

The checked public contracts are
[`publication-plan.schema.json`](https://github.com/HardMax71/amiss/blob/main/spec/publication-plan.schema.json),
with a matching
[`publication-plan.json`](https://github.com/HardMax71/amiss/blob/main/spec/examples/publication-plan.json)
example, and
[`publication-evidence.schema.json`](https://github.com/HardMax71/amiss/blob/main/spec/publication-evidence.schema.json),
with its
[`publication-evidence.json`](https://github.com/HardMax71/amiss/blob/main/spec/examples/publication-evidence.json)
example, and
[`publication-assessment.schema.json`](https://github.com/HardMax71/amiss/blob/main/spec/publication-assessment.schema.json),
with its replayed
[`publication-assessment.json`](https://github.com/HardMax71/amiss/blob/main/spec/examples/publication-assessment.json)
example. The strict readers, writers, and pure assessment live in `amiss-wire`; provider-specific
payloads never enter the engine contract.
