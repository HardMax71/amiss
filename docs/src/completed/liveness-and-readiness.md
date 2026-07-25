# Liveness and readiness answer different questions

An operator could not tell a live process from a serving one. `/healthz` answered before local
state was open, so an orchestrator would route deliveries into a process that could not accept
them, and neither restart nor credential rotation had an observable boundary.

The private listener now separates the two questions. `/healthz` answers whether the process is
running. `/readyz` answers whether admission can currently accept a delivery, which is what a
load balancer is actually asking. While unready, provider work is refused with `503` rather
than accepted and dropped, so the provider retries into a service that will still be there.

Lifecycle transitions are written to stderr as one redacted JSON object each, and the schema is
deliberately narrow: `schema`, `level`, `event`, and `component`, nothing else. A log line that
can carry a repository name or a delivery identity is a log line that can echo request data into
an operator's aggregator, so this one cannot.

One operator consequence is stated rather than left implied: none of the three private
endpoints is authenticated. The listener belongs on loopback or an operator network, with only
the provider `POST` path published through a proxy. An unauthenticated readiness endpoint on a
public interface is a free liveness oracle for anyone who wants one.

Added in [#122](https://github.com/hardmax71/amiss/pull/122). The endpoints are
[`controller/service/src/probe.rs`](https://github.com/hardmax71/amiss/blob/main/controller/service/src/probe.rs) and the event surface is
[`controller/service/src/operations.rs`](https://github.com/hardmax71/amiss/blob/main/controller/service/src/operations.rs).
