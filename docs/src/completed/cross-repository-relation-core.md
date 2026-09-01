# Cross-repository relation core

Closed September 2026. Documentation and code can live in different repositories, but neither
repository is allowed to choose the other repository, its credential, the comparison, or where a
verdict is published. This phase built the provider-neutral relation lifecycle beneath that rule:
four exact snapshots, one symmetric projection transition, durable supersession, and restart-safe
status delivery. [Cross-repository relations](../cross-repository-relations.md) is the live contract;
this page records what the core proves and where live assembly still stops.

## One operator plan owns four exact snapshots

The immutable two-subject registry in [#613](https://github.com/HardMax71/amiss/pull/613) fixes both
provider scopes, repositories, branches, credentials, selectors, budgets, trigger ownership, and
status destinations atomically. [#614](https://github.com/HardMax71/amiss/pull/614) acquires each
base/candidate pair into a physically independent Git root under per-subject and aggregate streaming
limits; an unavailable or unverifiable subject leaves the complete relation unproven.

The portable audit is split into a plan, four-slot projection evidence, and a replayable equality
transition by [#615](https://github.com/HardMax71/amiss/pull/615),
[#616](https://github.com/HardMax71/amiss/pull/616), and
[#617](https://github.com/HardMax71/amiss/pull/617). It can say aligned, introduced drift,
pre-existing drift, resolved drift, or unproven without appointing either repository as truth. The
pure tree projector and controller projection path land through
[#618](https://github.com/HardMax71/amiss/pull/618) and
[#619](https://github.com/HardMax71/amiss/pull/619), with operator-context binding in
[#620](https://github.com/HardMax71/amiss/pull/620). Accepted-report decoding is shared in
[#621](https://github.com/HardMax71/amiss/pull/621), the complete chain is independently replayed in
[#622](https://github.com/HardMax71/amiss/pull/622), and component derivation plus immutable retention
close in [#623](https://github.com/HardMax71/amiss/pull/623) and
[#624](https://github.com/HardMax71/amiss/pull/624).

## Coordination is opaque and supersession is fenced

The operator-supplied coordination identity added in
[#625](https://github.com/HardMax71/amiss/pull/625) may mean a pair, release, or workflow occurrence;
Amiss never infers it from timestamps or nearby heads. The pure admission law in
[#626](https://github.com/HardMax71/amiss/pull/626) preserves an exact retry, rejects identity
rebinding, and advances a fence when new coordination supersedes pending work.
[#627](https://github.com/HardMax71/amiss/pull/627) persists the same law in a bounded hash-chained
journal whose committed head makes interrupted appends recoverable and committed mutation visible.

Fresh two-subject heads freeze only configured destinations in
[#628](https://github.com/HardMax71/amiss/pull/628), and
[#629](https://github.com/HardMax71/amiss/pull/629) reserves every external status key to one
relation. Pure fenced staging arrives in [#630](https://github.com/HardMax71/amiss/pull/630); a tagged
journal action grammar in [#631](https://github.com/HardMax71/amiss/pull/631) lets the durable outbox
land in [#632](https://github.com/HardMax71/amiss/pull/632). Restart reopening, per-destination durable
acknowledgements, and serialized oldest-fence claims follow in
[#633](https://github.com/HardMax71/amiss/pull/633),
[#634](https://github.com/HardMax71/amiss/pull/634), and
[#635](https://github.com/HardMax71/amiss/pull/635). Dropping a claim mutates nothing; unrelated lock
shards can still publish in parallel.

## Provider boundaries preserve provider semantics

GitHub exact head resolution and idempotent App-owned check runs land in
[#636](https://github.com/HardMax71/amiss/pull/636) and
[#637](https://github.com/HardMax71/amiss/pull/637). Gitea-family commit statuses arrive in
[#638](https://github.com/HardMax71/amiss/pull/638) with their lack of writer-bound merge-gate
identity stated rather than hidden. GitLab's [#639](https://github.com/HardMax71/amiss/pull/639)
keeps its result synchronous and bound to the authenticated active policy job instead of pretending
it has the same asynchronous status model. Gitea-family exact head resolution follows in
[#640](https://github.com/HardMax71/amiss/pull/640).

Service assembly is likewise provider neutral: strict registry loading in
[#641](https://github.com/HardMax71/amiss/pull/641), exact credential routing in
[#642](https://github.com/HardMax71/amiss/pull/642), and authenticated coordination admission in
[#643](https://github.com/HardMax71/amiss/pull/643). Exact provider heads are retained and frozen in
[#644](https://github.com/HardMax71/amiss/pull/644) and
[#645](https://github.com/HardMax71/amiss/pull/645); canonical audit construction and the projection,
assessment, retention, and staging pipeline land in
[#646](https://github.com/HardMax71/amiss/pull/646) and
[#647](https://github.com/HardMax71/amiss/pull/647). The provider-neutral loop resumes durable claims
and acknowledges only reconciled destinations after restart in
[#648](https://github.com/HardMax71/amiss/pull/648).

The provider binaries still do not construct and install this registry, credential router, snapshot
acquisition, and lifecycle as one live lane. That topology needs an operator contract rather than a
hidden cross-provider default. Post-publication wiki drift also remains demand gated and would be a
separate Git subject observed after publication, never a pre-merge guarantee. Privacy-safe external
URL histories, organization-scale routing, and portable signed outcomes remain research until real
operators supply their retention, isolation, and trust-root requirements.
