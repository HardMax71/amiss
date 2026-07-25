# The GitLab lane runs as a policy job on the merge train

GitLab has no App identity to own a status, and any project member can edit a job the project
defines. A gate the checked project can edit is not a gate, so the usual shape, a CI job that
posts its own result, does not survive the threat model.

The lane uses a pipeline execution policy owned outside the checked project, which injects the
job into every enforced merge train. The checked project cannot remove it, rewrite it, or skip
it. The service then authenticates the job's short-lived OIDC token and binds its policy project
and commit, job and pipeline, runner, merge request, repository, and exact train-result commit
before trusting any provider state at all. Each of those is a way the job could be someone
else's, and the binding is what makes the token mean this run rather than any run.

Success is the exact HTTP `204` and nothing else. The job waits, and only the exact saved pass
returns it.

The lane requires GitLab 19.3 or newer with Ultimate, because enforced merge trains are what
make the policy job unavoidable, and they are generally available from 19.3. No live run is
recorded yet: as of July 2026 the newest release is 19.2.0, so no supported instance exists to
run. [Retained provider runs](../provider-evidence.md) states what a 19.2 instance did settle,
which is that the floor is real and where it bites.

Completed in [#107](https://github.com/hardmax71/amiss/pull/107). The service is
[`controller/gitlab-service/`](https://github.com/hardmax71/amiss/tree/main/controller/gitlab-service), the OIDC checks are
[`controller/gitlab/src/oidc.rs`](https://github.com/hardmax71/amiss/blob/main/controller/gitlab/src/oidc.rs), and the setup is
[GitLab](../provider-gitlab.md).
