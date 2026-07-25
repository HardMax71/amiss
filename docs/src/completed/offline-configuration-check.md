# Every provider binary can check its configuration offline

Finding out that a credential path is wrong when the first webhook arrives is an outage.
Finding out at deploy time is a typo.

Every provider binary has a network-free `--check` path that loads the same strict
configuration, local credentials, trust anchors, controls, constraint, bootstrap, and path
layout it would use at startup, without binding a listener, opening mutable service state, or
contacting a provider. It is the real loader rather than a second implementation that could
agree with a config the service would reject.

The check paths live beside each service entry point, such as
[`controller/github-service/src/main.rs`](https://github.com/HardMax71/amiss/blob/main/controller/github-service/src/main.rs), and are pinned by each service's
`tests/config.rs`. Added in [#121](https://github.com/HardMax71/amiss/pull/121).
