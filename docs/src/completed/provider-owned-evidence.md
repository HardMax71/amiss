# Provider evidence lives in the provider, not in the report

A report that says it was verified is a report asserting its own trustworthiness. Anything
that can produce the report can produce the claim.

So the evidence is an object the provider owns and the repository cannot forge: the App-owned
Check Run, the protected GitLab policy job, or the dedicated Gitea-family review, each with
the matching merge rule. The engine report stays exactly what it was, self-asserted, with no
provider signature and no `provider_verified` field. Nothing was added to it, which is the
decision worth recording. Each provider page states its commit or tree freshness limit, retry
behaviour, rotation rules, and full trust boundary.

The lanes and their boundaries are [Provider-verified controls](../provider-controls.md), and
the runs themselves are in [Retained provider runs](../provider-evidence.md). Completed in
[#107](https://github.com/HardMax71/amiss/pull/107).
