# Amiss controller

This unpublished nested Rust workspace holds provider transport, credentials, storage, and
runtime code. The offline scanner remains in the root workspace with a separate lockfile and no
network stack.

The workspace separates shared mechanics from provider code:

- `amiss-controller` defines provider-neutral ingress, orchestration, durable delivery records,
  and supervised bootstrap contracts.
- `amiss-controller-git` acquires exact SHA-1 commits through the bounded Git protocol-v2 path.
- `amiss-controller-service` provides a bounded HTTP receiver, a durable raw-delivery inbox, and
  worker for webhook lanes, plus a bounded synchronous endpoint for a CI job that must wait for
  the result.
- `amiss-controller-github` and `amiss-controller-github-service` implement the GitHub App and
  required Check Run lane.
- `amiss-controller-gitlab` and `amiss-controller-gitlab-service` implement the GitLab merge-train
  policy-job lane.
- `amiss-controller-gitea` and `amiss-controller-gitea-service` implement the dedicated-reviewer
  lane used by Gitea and Forgejo.

Durable state uses checksummed ordinary files with bounded capacity and atomic replacement. There
is no SQL or database. Webhook raw input is removed after controller completion; `FileLedger`
remains the replay and publication-retry authority. Exact-body completion markers are permanent
when the provider signature has no trusted time.

[Provider-verified controls](../docs/src/provider-controls.md) compares the supported lanes and
links their exact setup, configuration, state, and trust rules.
[Controller delivery](../docs/src/controller.md) documents the provider-neutral record,
heartbeats, races, and retry rules.

Run the nested workspace checks without adding anything to the root workspace:

```sh
cargo nextest run --manifest-path controller/Cargo.toml --workspace --locked
cargo clippy --manifest-path controller/Cargo.toml --workspace --all-targets --locked -- -D warnings
```

Source builds require the pinned Rust toolchain and a working C/C++ compiler for the AWS-LC
cryptography backend.
