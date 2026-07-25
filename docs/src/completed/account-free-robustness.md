# Hostile provider input is tested without provider accounts

Robustness checks that need a live provider account run rarely, run late, and stop running when
a token expires. The input worth testing does not need an account: it is bytes arriving at a
listener, and every one of them can be generated locally.

Two fuzz targets construct valid signed GitHub and Gitea-family webhooks and valid GitLab OIDC
material, then vary exactly one fact: an identity, a binding, a replay marker, or a freshness
claim. Starting from a valid request and breaking one thing is what makes the result meaningful,
because random bytes mostly test the parser's first branch, while a correctly signed request
with the wrong audience tests the check that matters. Committed seeds keep the corpus,
a deterministic smoke lane runs in CI, and a nightly coverage-guided run goes deeper.

The same change removed a smaller problem. Four RSA private keypairs were committed in the tree
as test fixtures. They were only test keys, but a valid private key in a repository is a finding
in every scanner that looks, and explaining that forever is worse than fixing it. A fixtures
crate now generates one pair per test process, which also proves freshness in a test rather
than in a comment.

Added in [#122](https://github.com/hardmax71/amiss/pull/122). The targets are
[`controller/fuzz/fuzz_targets/provider_webhooks.rs`](https://github.com/hardmax71/amiss/blob/main/controller/fuzz/fuzz_targets/provider_webhooks.rs)
and
[`controller/fuzz/fuzz_targets/gitlab_oidc.rs`](https://github.com/hardmax71/amiss/blob/main/controller/fuzz/fuzz_targets/gitlab_oidc.rs),
with the keypair generator in
[`controller/fixtures/`](https://github.com/hardmax71/amiss/tree/main/controller/fixtures).
