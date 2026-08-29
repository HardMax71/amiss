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

The outer payload digest is an integrity check, not a signature or authority claim. Repository
content must not choose the plan, producer context, relation rule, or credentials. A future
controller lane will retain the operator-selected plan beside the report and acquire
provider-authenticated deployment evidence. No scanner command or engine report consumes the plan
yet, and the plan alone never says a deployment happened.

The checked public contract is
[`publication-plan.schema.json`](https://github.com/HardMax71/amiss/blob/main/spec/publication-plan.schema.json),
with a matching
[`publication-plan.json`](https://github.com/HardMax71/amiss/blob/main/spec/examples/publication-plan.json)
example. The strict reader and writer live in `amiss-wire`; later evidence and assessment layers
reuse these typed values instead of reparsing provider-specific payloads inside the engine.
