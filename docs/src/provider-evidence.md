# Retained provider runs

The provider lanes are tested against local HTTP fixtures, and those fixtures are
regression tests rather than evidence. A fixture answers the way its author expected the
provider to answer. This page retains runs against provider software instead, where the
answers come from the provider.

One row is one published verdict: a real instance, a real dedicated reviewer or App, a
protected target branch, and a candidate that reached the sealed bootstrap. The provider
evidence column names what the provider itself recorded, not what the controller believed.

## July 2026

Controller `2dbb0b6`, action tree pinned at commit
`ca5b2b24f3c349553964387ceba62db5b3e87f5e` on every instance. The Gitea and Forgejo
instances are self-hosted, which is how nearly every deployment of either runs.

| Provider | Version | Control | Provider evidence | Gate commit |
| --- | --- | --- | --- | --- |
| GitHub | github.com | ruleset active | Check Run `success`, `conclusion: pass` | `6dbc7eb8c17b` |
| GitHub | github.com | ruleset enforcement disabled | Check Run `failure`, `unavailable / authorization-revoked` | `2d584f289f50` |
| Gitea | 1.27.0 | protection rule intact | review `APPROVED`, `conclusion: pass` | `4cf4fd91e3e2` |
| Gitea | 1.27.0 | direct push re-enabled | review `REQUEST_CHANGES`, `unavailable / authorization-revoked` | `d9496a77e2f5` |
| Forgejo | 16.0.1 | protection rule intact | review `APPROVED`, `conclusion: pass` | `ca697bd509d9` |
| Forgejo | 16.0.1 | `apply_to_admins: false` | review `REQUEST_CHANGES`, `unavailable / authorization-revoked` | `4fa69ed7d4c5` |

Each pair of rows holds the candidate content fixed and changes only the control, so the
verdict can flip for one reason. Every revocation was restored afterwards and the lane
returned to passing the same content.

Drift verdicts from the same instances, where the candidate broke a documented reference
and the control stayed intact, are recorded in
[pull request 131](https://github.com/HardMax71/amiss/pull/131). Both families blocked the
drift, refused the merge, and approved the correction.

In July GitLab had no row: the lane's floor is 19.3 with Ultimate, 19.2.0 was the newest
release, and a live 19.2.0-ee confirmed only that the floor refuses the instance.

## August 2026

Controller `d6d42de`, action tree pinned at commit
`b4b576da872c2e5b7243264919a324e39b7276ad`. The instance is gitlab.com itself, running
`19.3.0-pre` (revision `6ec80fea941`) ahead of the self-managed 19.3 release, under an
Ultimate trial namespace. The candidate content is held fixed across the pair and only
the enforcement control moves.

| Provider | Version | Control | Provider evidence | Gate commit |
| --- | --- | --- | --- | --- |
| GitLab | gitlab.com 19.3.0-pre | train enforced for all users | policy job `success`, train merged | `73da3cdf0319` |
| GitLab | gitlab.com 19.3.0-pre | `merge_train_enforcement: allow_bypass` | policy job `failed` on `412`, train dropped the car | `270b65505c42` |

The revocation was restored afterwards and the restored train merged the same content.
Running the lane live found two defects every fixture had agreed with, keeping the July
pattern: the documented policy job wrapped its script across YAML lines that policy
injection preserves literally, so the book now keeps the command on one physical line,
and gitlab.com answers the jobs API with a null `source` for the policy job, so the
adapter now accepts an absent REST source while the signed OIDC `job_source` claim and
the pinned `job_config` binding continue to state the provenance.

## What a row must be

A row enters this page only from a verdict the provider published and still holds: the
provider version as the provider reports it, the controller commit that produced the run,
the gate commit the verdict names, and the provider's own record of the verdict. A
positive row and a revoked-control row must differ in the control alone. A run against a
local HTTP fixture never becomes a row, however faithful the fixture looks.
