The Git commit responses were captured read-only on 2026-09-09 with
`X-GitHub-Api-Version: 2022-11-28`. They are unmodified JSON responses, including the
signature and signed payload where present. Tests run offline against these fixed bytes.

The [unsigned commit](https://api.github.com/repos/HardMax71/amiss/git/commits/9cefdc3c43f7f5c2b2da9f4bb84e1170842b668e)
has one parent and null signature metadata. The
[signed merge](https://api.github.com/repos/github/rest-api-description/git/commits/3cef12e8a02d612ad032473d4fb87266f2befeae)
has two parents and populated verification metadata. The model follows the
[GitHub Git commit contract](https://docs.github.com/en/rest/git/commits#get-a-commit-object).

The reference captures use the same API version and capture date. The
[exact reference](https://api.github.com/repos/HardMax71/amiss/git/ref/heads/github/typed-commit-flow)
and [prefix listing](https://api.github.com/repos/HardMax71/amiss/git/matching-refs/heads/github)
are stored separately to preserve their object-versus-array contracts. The branch name
contains a slash, and the nested object points to the fixed commit captured in the response.

The [artifact page](https://api.github.com/repos/HardMax71/amiss/actions/runs/34409057444/artifacts?per_page=2)
was captured read-only on 2026-09-10 with the same API version. It retains the complete
metadata and linked run for a coverage artifact; no archive was downloaded. Tests also
cover the missing and nullable fields declared by the
[artifact API](https://docs.github.com/en/rest/actions/artifacts#list-workflow-run-artifacts).

The owner fixtures were extracted with `gh api --jq .owner` from the public
[Amiss repository](https://api.github.com/repos/HardMax71/amiss) and
[GitHub API-description repository](https://api.github.com/repos/github/rest-api-description)
on 2026-09-10 with the same API version. They cover user and organization owners,
including URI templates and optional visibility metadata. They are nested-object
captures, not complete repository-response fixtures.

The workflow-repository fixture is the complete repository member of public
[run 34409057444](https://api.github.com/repos/HardMax71/amiss/actions/runs/34409057444),
captured read-only on 2026-09-10 with API version 2022-11-28. The custom-properties
fixture is the member of [github/docs](https://api.github.com/repos/github/docs)
captured the same way; its 11 entries were cross-checked against the read-only
repository properties endpoint. Optional metadata in contract tests is synthetic,
not claimed as captured from the workflow run.
