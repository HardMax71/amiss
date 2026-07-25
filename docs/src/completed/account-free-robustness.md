# Hostile provider input is tested without provider accounts

Robustness checks that need a live provider account run rarely, run late, and stop running
when a token expires. The hostile input worth testing does not need an account: it is bytes
arriving at a listener.

Account-free checks construct valid signed GitHub and Gitea-family webhooks and valid GitLab
OIDC material locally, then mutate them. They cover signature and claim tampering, wrong
routes, replayed identities, and malformed frames, in a coverage-guided loop that keeps its
corpus. Anyone with a checkout can run them, which is what keeps them running.

The targets are [`controller/fuzz/fuzz_targets/provider_webhooks.rs`](https://github.com/HardMax71/amiss/blob/main/controller/fuzz/fuzz_targets/provider_webhooks.rs) and
[`controller/fuzz/fuzz_targets/gitlab_oidc.rs`](https://github.com/HardMax71/amiss/blob/main/controller/fuzz/fuzz_targets/gitlab_oidc.rs). Added in [#122](https://github.com/HardMax71/amiss/pull/122).
