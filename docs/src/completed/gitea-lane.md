# The Gitea family lane publishes through a dedicated reviewer

Gitea and Forgejo have no App identity and no first-class status owner, so the only gate
available is an approval from an account nobody else controls.

The source-built service completes one lane for Gitea 1.27 or newer and Forgejo 16 or newer.
It authenticates the native exact-body HMAC, refreshes the pull request, commits, trees,
effective branch rule, and reviewer identity, then publishes an approval or a request for
changes through one dedicated reviewer account. That account, its recovery path, and any
session able to act as it are trust anchors, which the provider page states plainly.

The service is [`controller/gitea-service/`](https://github.com/HardMax71/amiss/tree/main/controller/gitea-service) and the setup is
[Gitea and Forgejo](../provider-gitea.md). Completed in [#107](https://github.com/HardMax71/amiss/pull/107). Live runs against both
families are in [Retained provider runs](../provider-evidence.md), and reaching them took
four corrections: [#131](https://github.com/HardMax71/amiss/pull/131) and [#132](https://github.com/HardMax71/amiss/pull/132).
