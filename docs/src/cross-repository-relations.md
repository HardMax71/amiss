# Cross-repository relations

Documentation and the implementation it describes may live in different repositories. A change to
either one can introduce drift while the other repository remains byte-for-byte unchanged. The
controller now has the operator-owned registry needed to identify those relations without letting
repository content choose another repository, credential, selector, limit, or publication target.

This is the registry boundary, not a complete cross-repository checking lane. No service loads this
configuration yet, and the controller does not yet acquire the four Git snapshots, evaluate the
relation, retain evidence, or publish a status.

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

The configured target branch is not treated as an immutable revision. The next acquisition stage
must refresh provider state, verify that target, and freeze exact base and candidate object IDs for
both subjects before any comparison. Commit timestamps, nearby branch heads, URL versions, and
repository prose are not pairing evidence.

## Trust boundary

The registry lives only in the unpublished controller layer. The offline engine still has one
declared repository root, no network or async dependency, no credential input, and no ability to
follow a link into another repository. Projection sources in this registry are operator input;
similarly shaped repository policy does not add a relation or gain access to its credential
reference.

The remaining stages are deliberately separate:

1. acquire exact base and candidate revisions for both subjects under their independent and
   aggregate budgets;
2. compare the two complete projections and retain exact human-readable subject identities;
3. model coordinated release or paired-change intent without timestamp inference; and
4. deduplicate triggers from either provider route and publish only to the configured subject roles.

Until those stages exist, the registry proves only that trusted configuration and authenticated
delivery selection have a closed representation. It does not claim that any cross-repository
content has been acquired, compared, or approved.

The implementation is in the
[provider-neutral relation registry](https://github.com/HardMax71/amiss/blob/main/controller/src/relations.rs),
and the [registry laws](https://github.com/HardMax71/amiss/blob/main/controller/tests/suite/relations.rs)
exercise bidirectional selection, stable ordering, invalid identities, projection compatibility,
joint budgets, and exact destinations.
