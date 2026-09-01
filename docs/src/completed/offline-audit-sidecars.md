# Offline audit sidecars

Closed August 2026. A repository report cannot prove what was later deployed, which product a site
describes, or whether two locale inventories cover the same pages. Putting those facts into the
scanner report would give one candidate a claim over events outside its trust boundary. This phase
instead added closed report-bound sidecars with the same conservative `matched`, `refuted`, and
`unproven` vocabulary. [Publication audits](../publication.md) and
[Locale coverage audits](../locale-coverage.md) own the live contracts.

## Publication is an exact relation, not a URL guess

The publication plan in [#601](https://github.com/HardMax71/amiss/pull/601) binds the accepted report,
docs candidate, completed-site artifact, deployment target, exact product resource, producer, and
operator relation rule. Provider-normalized evidence from
[#602](https://github.com/HardMax71/amiss/pull/602) repeats those identities beside one immutable
successful deployment record and workflow definition. URLs, tags, timestamps, and similarly named
channels never substitute for resource digests.

[#603](https://github.com/HardMax71/amiss/pull/603) assesses the pair offline through one `Result`
flow. Failure, missing evidence, mutable identity, or any mismatched binding cannot become a match.
The controller independently validates that chain against its accepted scanner report in
[#604](https://github.com/HardMax71/amiss/pull/604), then
[#605](https://github.com/HardMax71/amiss/pull/605) retains and reopens the exact report, plan,
optional evidence, assessment, and digest set. Retry replays bytes; it does not rejudge a changed
deployment.

## Locale coverage separates presence from provenance

[#606](https://github.com/HardMax71/amiss/pull/606) first extracted the shared bounded, digest-bound
sidecar envelope before adding another audit family. The locale plan from
[#607](https://github.com/HardMax71/amiss/pull/607) then fixes opaque stable page keys, source and
target locales, coverage policy, producer context, and an optional exact product resource.
Independent complete or partial inventories arrive in
[#608](https://github.com/HardMax71/amiss/pull/608), and the bounded assessment in
[#609](https://github.com/HardMax71/amiss/pull/609) reports missing and orphan keys only where
completeness permits it. A present key proves coverage, not translation quality.

Fallback cannot masquerade as target-owned content after
[#610](https://github.com/HardMax71/amiss/pull/610): every fallback has an operator-authorized opaque
class and exact source digest. [#611](https://github.com/HardMax71/amiss/pull/611) compares explicit
producer-owned source lineage and refuses to infer it from time, text similarity, or Git adjacency.
[#612](https://github.com/HardMax71/amiss/pull/612) reuses publication's exact product resource on
both inventories, so a locale version label cannot impersonate a release identity.

## The offline boundary is part of the result

No command or controller lane currently acquires locale evidence. Publication has controller
validation and durable retention, but no provider lane authenticates deployment completion or
publishes a post-deployment audit. GitHub, GitLab, and Gitea-family deployment integration remains
operator gated because their environment, artifact, credential, completion, and public-destination
contracts are not interchangeable. The milestone is the reusable offline proof and retention core,
not a claim that a live deployment can already invoke it.
