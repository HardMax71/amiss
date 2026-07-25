# The GitLab lane runs as a policy job on the merge train

GitLab has no App identity to own a status, and a project member could edit any job the
project defines. A gate that the checked project can edit is not a gate.

The source-built service completes the policy-job lane for one project and one protected
target branch on GitLab 19.3 or newer with Ultimate. A pipeline execution policy owned outside
the checked project injects the job into every enforced merge train, so the checked project
cannot remove or rewrite it. The service authenticates the job's short-lived OIDC token and
binds its policy project and commit, job and pipeline, runner, merge request, repository, and
exact train-result commit before trusting any provider state.

The service is [`controller/gitlab-service/`](https://github.com/HardMax71/amiss/tree/main/controller/gitlab-service) and the setup is
[GitLab](../provider-gitlab.md). Completed in [#107](https://github.com/HardMax71/amiss/pull/107). No live run is recorded yet: the
floor is unreleased, as [Retained provider runs](../provider-evidence.md) explains.
