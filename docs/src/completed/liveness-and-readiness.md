# Liveness and readiness answer different questions

An orchestrator that cannot tell "this process is alive" from "this process should receive
traffic" either restarts a healthy service or routes deliveries into one that cannot accept
them.

The private listener separates `/healthz` liveness from `/readyz` admission readiness. Health
answers whether the process is running. Readiness answers whether admission can currently
accept a delivery, which is the question a load balancer is actually asking. Both are on the
private listener rather than the delivery listener, so probing does not go through the path
that accepts provider traffic.

The probe endpoints are [`controller/service/src/probe.rs`](https://github.com/HardMax71/amiss/blob/main/controller/service/src/probe.rs). Added in [#122](https://github.com/HardMax71/amiss/pull/122).
