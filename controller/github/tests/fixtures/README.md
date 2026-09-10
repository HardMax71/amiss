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

The workflow-run fixture is the complete response for that same run. The
workflow-runs page uses the adapter's query for its workflow ID and head SHA,
event=pull_request, status=success, exclude_pull_requests=true, per_page=2 and
page=1. The GET retains the linked pull request; the filtered page has an empty
pull_requests array. Both were captured read-only on 2026-09-10 with API version
2022-11-28. Nullable states and referenced workflows added in tests are synthetic.

The GitHub app fixture is the complete app member of
[check run 102692011089](https://api.github.com/repos/HardMax71/amiss/check-runs/102692011089),
captured read-only on 2026-09-10 with API version 2022-11-28. Its 23 permission
entries have string values; permission names are open in the upstream contract.
Enterprise owners and optional/null metadata in tests are schema-backed synthetic
cases, not captured enterprise data. No check run or app was created or changed.

The check-run fixture is the complete response for the same check. Its check-runs
page uses the exact adapter query: check_name=json-contract-drift, filter=all,
per_page=100, page=1 and app_id=15368 on the captured head. The page's sole record
equals the individual response. Deployment metadata and stale conclusions in
tests are synthetic; GitHub documents stale as a server-assigned conclusion even
though its response-schema enumeration omits it. No check was created or updated.

The pull-repository fixture is the complete head repository member of
[pull request 935](https://api.github.com/repos/HardMax71/amiss/pulls/935), captured
read-only on 2026-09-10 with API version 2022-11-28; the base repository member was
identical. The repository-access fixture is the complete permissions member of
the ordinary Amiss repository GET on the same date. Optional policies, code-search
metadata and token text in tests are synthetic, not captured credentials.

The full repository fixtures are read-only GETs of
[Amiss](https://api.github.com/repos/HardMax71/amiss) and an existing
[API-description fork](https://api.github.com/repos/rizalgowandy/rest-api-description),
captured on 2026-09-10 with API version 2022-11-28. The fork preserves both parent
and source records. The Amiss response contains the documented validity-check
security setting missing from the upstream shared response schema. Captured clone
tokens are absent or empty; populated template, organization and token cases in
tests are synthetic. No repository or security setting was changed.

The pull-refs fixture collects the complete head and base members, in that order,
from read-only GETs of [Amiss pull request 936](https://api.github.com/repos/HardMax71/amiss/pulls/936)
and [Rust pull request 147350](https://api.github.com/repos/rust-lang/rust/pulls/147350),
captured on 2026-09-10 with API version 2022-11-28. It is a collection of response
members, not a server listing response. The closed Rust PR has a null head
repository despite the schema's nonnullable reference. No nonempty clone token
is present, and no pull request or repository was changed.

The pull-request fixtures retain those same complete open and closed GET responses,
including the closed PR's real label. Milestone, auto-merge, requested-team and
stack instances in contract tests are synthetic schema-backed cases, not captured
provider behavior. Their absence from these responses does not make them optional
where the API declares otherwise. No token was created or nonempty clone token retained.

The installation-token fixture is synthetic, following the versioned API schema:
all eight root fields and all 55 named app permissions are populated. Its token
text is not a credential. Tests separately embed the captured pull repository;
neither case is claimed as a captured installation-token response. The historical
official token example has repository/schema inconsistencies, so it was not
silently edited into a purported live capture. No installation token was minted.

The branch-rule fixtures are unchanged first pages for
[Amiss main](https://api.github.com/repos/HardMax71/amiss/rules/branches/main?per_page=100&page=1)
and [github/docs main](https://api.github.com/repos/github/docs/rules/branches/main?per_page=100&page=1),
captured read-only on 2026-09-10 with API version 2022-11-28. The latter includes
the boolean review and merge-queue settings missing from the pinned OpenAPI
description. Both are false in the capture; true, omitted and malformed values
in tests are synthetic variations. No rules or repository settings were changed.
Only a final newline is added to the captured JSON for repository hygiene.

branch-rules-synthetic.json is the existing 23-variant contract example, not a
provider capture. Optional parameters and integration IDs use omission, as
declared by the endpoint schema; supplied nulls are negative test cases.

workflow-webhook-pr.json projects the first workflow_run.pull_requests entry
from [Octokit's published completion example](https://github.com/octokit/webhooks/blob/7dd7fa56498a827a08b71919fae89428f5e8e283/payload-examples/api.github.com/workflow_run/completed.with-pull-requests.payload.json).
Its values and members are unchanged; the projection is pretty-printed with a
final newline. This is a public example, not a new live webhook capture or a
positive full-envelope authentication fixture. The existing workflow-run.json
separately supplies the real REST counterpart.

webhook-workflow.json projects the unchanged workflow member from the same
published completion example. webhook-installation.json projects installation
from [Octokit's ready-for-review example](https://github.com/octokit/webhooks/blob/7dd7fa56498a827a08b71919fae89428f5e8e283/payload-examples/api.github.com/pull_request/ready_for_review.with-installation.payload.json).
Both retain every member and value, pretty-printed with one trailing newline.
They are published examples, not newly captured live deliveries.

webhook-pull-changes.json is synthetic, following the closed changes object in
[Octokit's pinned edited-PR schema](https://github.com/octokit/webhooks/blob/7dd7fa56498a827a08b71919fae89428f5e8e283/payload-schemas/api.github.com/pull_request/edited.schema.json).
It includes previous base ref and SHA, body and title. That revision has no
published edited-PR payload example; this is not presented as a captured delivery.

webhook-workflow-repository.json projects the unchanged workflow_run.repository
from the published completion example linked above; its head_repository member
is identical. Every member and value is retained with pretty-printing and a
trailing newline. Nullable and minimally populated owners in tests follow the
official OpenAPI contract, which is broader than Octokit's owner reference.

webhook-template-repository.json is synthetic: it populates all 87 template fields
and all 18 optional owner fields in repository-webhooks from the pinned official
OpenAPI contract. Compatible values come from the published completion example;
other fields use explicit sample values. It is not an observed template response.
The complete event-root repository example is shared through amiss-fixtures.

Root organization fixtures cover the name declared by Octokit's completion
schema and the nullable simple-user object declared by GitHub OpenAPI. Both
forms are explicit typed alternatives; the account form keeps the existing
closed owner contract. These additional forms are constructed, not live captures.

webhook-workflow-commit.json projects the unchanged workflow_run.head_commit
from the published completion example linked above. Null emails and optional
date/username fields in tests are synthetic cases from the pinned webhook
contract. The REST workflow fixtures keep their distinct nullable-author contract.

webhook-workflow-run.json projects the complete unchanged workflow_run from that
published completion example. It retains all 34 supplied fields; optional referenced
workflows, nullable actors and PR entries, and additional terminal states in tests
are synthetic cases from GitHub's pinned OpenAPI contract, not new live captures.

webhook-pull-repository.json retains the complete head.repo member from
[Octokit's synchronize example](https://github.com/octokit/webhooks/blob/7dd7fa56498a827a08b71919fae89428f5e8e283/payload-examples/api.github.com/pull_request/synchronize.payload.json).
Its base.repo and both references in the published opened example are identical.
All 78 supplied fields remain unchanged, with pretty-printing and a final newline.
This is a published example, not a newly captured live delivery.

The pinned OpenAPI synchronize and Octokit common-repository schemas disagree on
required discussion/template/signoff/custom-property fields. The model preserves
their 74 shared required fields and all 25 named optional additions: absent disputed
fields receive no invented defaults, and supplied nulls remain invalid. Timestamp,
license, owner and policy variations in tests are synthetic schema-backed cases.
The existing REST repository contract is deliberately not widened to match them.

webhook-pull-refs.json retains the complete head and base references, in that
order, from the same published synchronize example. Both references are identical
to the corresponding opened-example members. All five fields and their complete
nested records are unchanged, with pretty-printing and a trailing newline.
Nullable accounts, deleted head repositories and numeric timestamps in tests
are explicit synthetic cases from the two pinned contracts, not live captures.
