# Every provider binary can check its configuration offline

Before this, an operator learned that a credential path was wrong by starting the service and
waiting for a webhook. On GitHub it was worse: private-key and API-client validation happened at
runtime, so a bad key surfaced during the first real delivery rather than at deploy time.

Every provider binary now takes `--check` with an absolute configuration path. It runs the same
strict loader startup runs, over the same local credentials, trust anchors, controls, execution
constraint, bootstrap, and path layout, and it validates the execution constraint against the
actual host it is running on. GitHub also constructs its App client during that load, so a
malformed private key fails here instead of later. Then it exits, before service runtime,
before mutable state is opened, before the bootstrap is executed, and before any provider I/O.

It is the real loader rather than a second implementation, which is the only version of this
feature worth having. A preflight that agrees with a configuration the service would reject is
a preflight that costs trust rather than earning it.

What it deliberately does not claim is written down: not provider reachability, not credential
permissions, not merge-rule correctness, not service readiness, and not retained live-provider
evidence. It is account-free on purpose, so it can run in a deployment pipeline that holds no
provider credentials at all.

Added in [#121](https://github.com/hardmax71/amiss/pull/121). The entry points are each service's `main.rs`, such as
[`controller/github-service/src/main.rs`](https://github.com/hardmax71/amiss/blob/main/controller/github-service/src/main.rs), and each
service's `tests/config.rs` covers the family.
