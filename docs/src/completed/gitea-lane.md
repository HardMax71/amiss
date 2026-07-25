# The Gitea family lane publishes through a dedicated reviewer

Gitea and Forgejo have no App identity and no first-class status owner. The only gate available
is an approval from an account nobody else controls, which makes the account itself a trust
anchor: whoever can act as that reviewer can satisfy the gate without Amiss, and the provider
page says so in those words.

The service authenticates the native exact-body HMAC, refreshes the pull request, commits,
trees, effective branch rule, and reviewer identity, then publishes an approval or a request for
changes as that one account. It supports Gitea 1.27 or newer and Forgejo 16 or newer.

Getting it to work against real instances took four corrections that no fixture had caught,
because each fixture was written to the API's documentation rather than its behavior. Both
families answer `/git/commits/{sha}` with the commit's own name in the commit's `tree` field:

```text
$ curl .../git/commits/436a6f35fd89b32d8661c6d7e12ba19960dfd841
  sha        : 436a6f35fd89b32d8661c6d7e12ba19960dfd841
  commit.tree: 436a6f35fd89b32d8661c6d7e12ba19960dfd841
$ git cat-file -p 436a6f35
  tree 5c1c95daa0e57e7a46ad6937d4b1515e0b5ff43f
```

No route on either family states the tree of a commit, so trees now come from fetched Git
objects through the same resolver the GitLab lane already used. Forgejo also sends one signature
under two spellings, `X-Forgejo-Signature` and `X-Gitea-Signature`, and the header reader treated
the second spelling as ambiguity and answered `401` to every real Forgejo delivery. A transient
`mergeable: false`, which Gitea reports for a second or two while it recomputes a merge, was
being read as a terminal verdict. And a control revoked between staging and publication left the
lane publishing nothing at all.

Completed in [#107](https://github.com/hardmax71/amiss/pull/107), corrected in [#131](https://github.com/hardmax71/amiss/pull/131) and [#132](https://github.com/hardmax71/amiss/pull/132). Live runs against
Gitea 1.27.0 and Forgejo 16.0.1 are in [Retained provider runs](../provider-evidence.md). The
service is [`controller/gitea-service/`](https://github.com/hardmax71/amiss/tree/main/controller/gitea-service) and the setup is
[Gitea and Forgejo](../provider-gitea.md).
