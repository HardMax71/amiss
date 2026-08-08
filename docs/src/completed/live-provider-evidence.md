# Live provider evidence

Closed August 2026. A provider lane tested only against local HTTP fixtures answers the way its
author expected the provider to answer, so the fixtures are regression tests and not evidence. This
phase retained runs against the providers themselves, one positive and one revoked-control pair per
lane, with the candidate content held fixed inside each pair so the verdict could flip for exactly
one reason. The rows live in [Retained provider runs](../provider-evidence.md), which stays the live
chapter; this page records how the phase closed.

GitHub, Gitea, and Forgejo closed in July 2026 against github.com, a self-hosted Gitea 1.27.0, and a
self-hosted Forgejo 16.0.1, controller `2dbb0b6`, action tree `ca5b2b24f3c3`. Drift verdicts from
the same instances, where the candidate broke a documented reference and the control stayed intact,
are kept in [pull request 131](https://github.com/HardMax71/amiss/pull/131).

GitLab closed in August 2026 without waiting for the self-managed release: gitlab.com deploys ahead
of it and was running `19.3.0-pre` under an Ultimate trial, and the lane's floor is structural
rather than a version compare, so the served `merge_train_enforcement` field was the whole test.
Controller `d6d42de`, action tree `b4b576da` at v0.17.0, an enforced merge train on a protected
project, and the pipeline execution policy pinned by commit. The enforcement control was revoked to
`allow_bypass` for the second row, the policy job failed on `412` and the train dropped the car, and
the restored train then merged the same content.

The campaigns kept paying in the same currency: six defects across the four lanes that every fixture
had agreed with. July's four are recorded with the July rows. August found two more, the documented
policy job wrapping its script across YAML lines that policy injection preserves literally, fixed by
keeping the command on one physical line in [the lane page](../provider-gitlab.md), and gitlab.com
answering the jobs API with a null `source` for the policy job, fixed by accepting an absent REST
source while the signed OIDC `job_source` claim and the pinned `job_config` binding continue to
state the provenance.

What the phase defends: every supported lane's trust story now rests on verdicts the provider
published and still holds, not on what a fixture author believed. The remaining planned milestone,
the wire leaving experimental, is a discipline stated in the [Roadmap](../roadmap.md) rather than a
build.
