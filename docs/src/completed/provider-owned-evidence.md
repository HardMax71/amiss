# Provider evidence lives in the provider, not in the report

A report that says it was verified is a report asserting its own trustworthiness. Anything that
can produce the report can produce the claim, so the field would be worth exactly nothing and
would read as though it were worth something. That is worse than omitting it.

So the evidence is an object the provider owns and the checked repository cannot forge: the
App-owned Check Run, the protected GitLab policy job, or the dedicated Gitea-family review, each
paired with the merge rule that makes it necessary. The engine report stays exactly what it was,
self-asserted, with no provider signature and no `provider_verified` field. Nothing was added to
it, and that decision is the one worth recording: the natural move when shipping provider
verification is to stamp the artifact, and the stamp would have been a lie.

Each provider page states its own commit or tree freshness limit, retry behavior, rotation
rules, and full trust boundary, including which accounts and keys can satisfy the gate without
Amiss. A trust boundary that is not written down is a trust boundary nobody checked.

Completed in [#107](https://github.com/hardmax71/amiss/pull/107). The lanes are
[Provider-verified controls](../provider-controls.md) and the runs themselves are
[Retained provider runs](../provider-evidence.md).
