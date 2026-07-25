# Fuzzing controller authentication

These harnesses exercise the account-free authentication boundary shared by the
provider services. They build valid signatures before changing one event or claim
fact, so most inputs reach provider identity, repository, run, target binding,
replay identity and lifetime, and freshness checks instead of stopping at signature
syntax.

`provider_webhooks` signs generated GitHub and Gitea-family pull-request events with a
fixed test secret. `gitlab_oidc` signs generated policy-job claims with a fresh
process-local RSA pair. Neither target opens a listener, contacts a provider, or reads
operator credentials.

The stable smoke replays the committed seeds and a deterministic set of flips,
truncations, and extensions:

```sh
cd controller/fuzz
cargo test --locked --release
```

Nightly coverage-guided runs use the same library bodies:

```sh
cargo +nightly fuzz run <target> --features harness \
  corpus/<target> seeds/<target>
```

Targets: `provider_webhooks`, `gitlab_oidc`.

Pack validation and the durable inbox and ledger frame decoders stay private to their
own crates. They retain focused deterministic corruption suites; this fuzz crate does
not widen those production APIs merely to reach them.
