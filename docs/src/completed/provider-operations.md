# Provider operations

Closed July 2026. A lane that cannot be deployed, watched, or restarted without losing work is
not finished, whatever its verdicts say. Five claims, all added in
[#121](https://github.com/hardmax71/amiss/pull/121) and
[#122](https://github.com/hardmax71/amiss/pull/122).

## Every provider binary can check its configuration offline

Before this, an operator learned that a credential path was wrong by starting the service and
waiting for a webhook. On GitHub it was worse: private-key and API-client validation happened at
runtime, so a bad key surfaced during the first real delivery.

Every provider binary takes `--check` with an absolute configuration path. It runs the same
strict loader startup runs, over the same local credentials, trust anchors, controls, execution
constraint, bootstrap, and path layout, and validates the execution constraint against the host
it is running on. GitHub also constructs its App client during that load. Then it exits, before
service runtime, before mutable state is opened, before the bootstrap is executed, and before
any provider I/O.

It is the real loader rather than a second implementation, which is the only version of this
feature worth having. A preflight that agrees with a configuration the service would reject
costs trust rather than earning it.

What it does not claim is written down: not provider reachability, not credential permissions,
not merge-rule correctness, not service readiness, and not retained live-provider evidence. It
is account-free on purpose, so it can run in a pipeline holding no provider credentials at all.

The entry points are each service's `main.rs`, such as
[`controller/github-service/src/main.rs`](https://github.com/hardmax71/amiss/blob/main/controller/github-service/src/main.rs),
and each service's `tests/config.rs` covers the family.

## Liveness and readiness answer different questions

An operator could not tell a live process from a serving one. `/healthz` answered before local
state was open, so an orchestrator would route deliveries into a process that could not accept
them, and neither restart nor credential rotation had an observable boundary.

The private listener separates the two. `/healthz` answers whether the process is running.
`/readyz` answers whether admission can currently accept a delivery, which is what a load
balancer is actually asking. While unready, provider work is refused with `503` rather than
accepted and dropped, so the provider retries into a service that will still be there.

Lifecycle transitions are written to stderr as one redacted JSON object each, and the schema is
deliberately narrow: `schema`, `level`, `event`, and `component`, nothing else. A log line that
can carry a repository name or a delivery identity can echo request data into an operator's
aggregator, so this one cannot.

One operator consequence is stated rather than left implied: none of the three private endpoints
is authenticated. The listener belongs on loopback or an operator network, with only the
provider `POST` path published through a proxy. An unauthenticated readiness endpoint on a
public interface is a free liveness oracle for anyone who wants one. The endpoints are
[`controller/service/src/probe.rs`](https://github.com/hardmax71/amiss/blob/main/controller/service/src/probe.rs).

## Ten counters, no labels, no cardinality surprise

Metrics with provider-supplied labels let a provider decide how much memory the registry uses,
which turns monitoring into a denial-of-service surface. A metric set that grows as the code
grows becomes a compatibility surface nobody agreed to maintain.

`/metrics` exposes exactly ten fixed, label-free, process-local counters under the
`amiss_controller` prefix:

```rust
pub provider_requests: Counter,
pub provider_acceptances: Counter,
pub provider_refusals: Counter,
pub provider_unavailable: Counter,
pub delivery_attempts: Counter,
pub delivery_completions: Counter,
pub delivery_retries: Counter,
pub delivery_discards: Counter,
pub maintenance_runs: Counter,
pub maintenance_removals: Counter,
```

The set cannot grow from a repository, an identity, or a result, so nothing a provider sends can
add a series. Lifecycle transitions are emitted as events rather than folded into counters, so a
restart does not move a number someone alerts on.

Process-local is a deliberate limit, not an oversight. These counters describe one process since
it started. The durable record is the delivery ledger, which is designed for that job. They live
in
[`controller/service/src/operations.rs`](https://github.com/hardmax71/amiss/blob/main/controller/service/src/operations.rs).

## Shutdown finishes the work it already accepted

A service that exits on a signal drops the delivery it just acknowledged, and an acknowledged
webhook is one the provider will not send again. Restarts are ordinary. Losing a verdict per
deploy is not.

On a termination signal, in-flight HTTP work finishes. A webhook worker finishes its current
delivery and leaves the durable backlog for the next process, which is the right split: the
backlog is already in the inbox, while the delivery in hand has state only this process holds.
The GitLab lane also finishes admitted evaluations and any running ledger maintenance, since
maintenance interrupted halfway is what leaves a root needing recovery.

A second termination signal aborts the process rather than waiting on a stuck drain. That is the
escape hatch an operator needs at three in the morning, and it is documented so nobody has to
discover it by holding a key down. Drain is
[`controller/service/src/shutdown.rs`](https://github.com/hardmax71/amiss/blob/main/controller/service/src/shutdown.rs).

## Hostile provider input is tested without provider accounts

Robustness checks that need a live provider account run rarely, run late, and stop running when
a token expires. The input worth testing does not need an account: it is bytes arriving at a
listener.

Two fuzz targets construct valid signed GitHub and Gitea-family webhooks and valid GitLab OIDC
material, then vary exactly one fact: an identity, a binding, a replay marker, or a freshness
claim. Starting from a valid request and breaking one thing is what makes the result meaningful.
Random bytes mostly test the parser's first branch, while a correctly signed request with the
wrong audience tests the check that matters. Committed seeds keep the corpus, a deterministic
smoke lane runs in CI, and a nightly coverage-guided run goes deeper.

The same change removed a smaller problem. Four RSA private keypairs were committed in the tree
as test fixtures. They were only test keys, but a valid private key in a repository is a finding
in every scanner that looks, and explaining that forever is worse than fixing it. A fixtures
crate now generates one pair per test process, which also proves freshness in a test rather than
in a comment.

The targets are
[`controller/fuzz/fuzz_targets/`](https://github.com/hardmax71/amiss/tree/main/controller/fuzz/fuzz_targets),
with the keypair generator in
[`controller/fixtures/`](https://github.com/hardmax71/amiss/tree/main/controller/fixtures).
