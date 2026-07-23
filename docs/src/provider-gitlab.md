# GitLab provider lane

The unpublished
[`amiss-controller-gitlab-service`](https://github.com/HardMax71/amiss/tree/main/controller/gitlab-service)
crate serves one GitLab project, one pipeline execution policy, and one protected target branch.
It supports GitLab 19.3 or newer with Ultimate. The minimum comes from enforced merge trains,
which are generally available from 19.3.
GitLab 19.2's feature-flagged preview is not supported.

This lane does not use a project webhook or write a commit status. An independently owned pipeline
execution policy injects one job into the merge train. That job presents a short-lived GitLab OIDC
token to the service and waits. Only an exact Amiss pass returns HTTP success, so the job's own
result is the provider evidence required by the train.

## Flow

The request body contains only the merge request's project-local number, which GitLab calls its
IID:

```json
{"merge_request_iid": 42}
```

```dot process
digraph gitlab_provider {
  rankdir = LR;
  node [shape = box, fontname = "Latin Modern, Georgia, serif", fontsize = 11];
  edge [arrowsize = 0.7, fontname = "Latin Modern, Georgia, serif", fontsize = 10];
  policy [label = "independent pipeline\nexecution policy"];
  train  [label = "enforced merge train\n+ injected job"];
  tls    [label = "TLS terminator"];
  oidc   [label = "OIDC claims\n+ bounded request"];
  first  [label = "refresh job, train,\nchange + rule"];
  fetch  [label = "acquire exact\nrepo + action"];
  boot   [label = "sealed\nbootstrap"];
  final  [label = "refresh gate\nagain"];
  save   [label = "save exact\nresult"];
  result [label = "204 only for pass;\npolicy job succeeds"];
  policy -> train -> tls -> oidc -> first -> fetch -> boot -> final -> save -> result;
}
```

The bearer token, not that body, supplies the project, pipeline, job, runner, policy origin, exact
train commit, issue time, and replay ID. The service requires an RS256 token under one configured
key ID, issuer, and audience. It then binds the `job_project_id`, canonical
`job_project_path`, `pipeline_id`, `job_id`, `runner_id`, `runner_environment`, `sha`,
`pipeline_source`, `job_source`, and policy `job_config` claims. The pipeline source must be
`merge_request_event`, and the job source must be `pipeline_execution_policy`.

The first provider refresh reads the exact job, pipeline, merge-train car, merge request, project,
target branch, protected-branch rules, and Git objects. The service requires the job, pipeline,
and train to be running and to name the same train commit. It runs the sealed bootstrap only after
that state and both acquired trees agree. A second refresh performs the same checks before the
saved result is accepted.

## GitLab project

Use a SHA-1 project on a root-mounted HTTPS GitLab instance. Configure the checked project as
follows:

- enable merged-results pipelines and merge trains;
- set merge-train enforcement to **Enforce for all users**, including Owners and administrators;
- require pipelines to succeed and do not count skipped pipelines as successful;
- disable the option that lets a merge request skip the train;
- use the `merge` merge method, set squash to `never`, and do not enable squash on the merge
  request; and
- protect the target branch with direct push and force push disabled.

The service reads every protected-branch rule whose exact or wildcard name matches the target.
At least one must match, and every match must report `allow_force_push: false` and one nonempty
push-access entry with access level `0` and no user, group, deploy key, or member-role exception.
A broader matching rule that restores a push path revokes the run.

The merge method is part of the tree proof. The train result must have exactly two parents: the
target or previous train car first, and this merge request's source commit second. Fast-forward,
semi-linear, rebased, or squashed train shapes are not supported.

These requirements follow GitLab's
[merge-train enforcement](https://docs.gitlab.com/ci/pipelines/merge_trains/) and
[protected-branch](https://docs.gitlab.com/api/protected_branches/) contracts. The adapter checks
the live API response rather than trusting the service configuration to describe it.

## Pipeline execution policy

Keep the security policy project and included CI configuration outside the checked project and
under the operator who owns this lane. Use an enabled
[pipeline execution policy](https://docs.gitlab.com/user/application_security/policies/pipeline_execution_policies/)
whose complete shape is:

```yaml
pipeline_execution_policy:
  - name: Amiss documentation gate
    description: Runs Amiss on every merge-train result
    enabled: true
    pipeline_config_strategy: inject_policy
    content:
      include:
        - project: security/amiss-policy-job
          file: policy-ci.yml
          ref: eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee
    policy_scope:
      projects:
        including:
          - id: 101
    suffix: never
    skip_ci:
      allowed: false
    no_pipeline:
      allowed: false
    variables_override:
      allowed: false
      exceptions: []
      dotenv: respect_policy
```

The `content.include.ref` must be an immutable 40-character commit, not a branch or tag. This
closes a separate movement path: GitLab's OIDC `job_config.url` and `job_config.sha` identify the
security-policy YAML, while the included CI file contains the job itself. Pinning the include
makes the policy commit bind both. `suffix: never` makes a duplicate project job fail instead of
being renamed. Skip and no-pipeline allowances remain false, and no variable or dotenv exception
may alter the job. Scope the policy to the exact checked project.

The included job requests one
[OIDC ID token](https://docs.gitlab.com/ci/secrets/id_token_authentication/) with the exact
audience configured in the service. Its only security decision is the service response. A minimal
shape is:

```yaml
amiss:policy:
  stage: .pipeline-policy-post
  image: registry.example/security/http-client@sha256:<reviewed-image-digest>
  allow_failure: false
  id_tokens:
    AMISS_ID_TOKEN:
      aud: https://amiss.example/gitlab/policy/evaluate
  variables:
    GIT_STRATEGY: none
  script:
    - >-
      test "$(
        curl --fail-with-body --silent --show-error
        --output /dev/null
        --write-out '%{http_code}'
        --header "Authorization: Bearer ${AMISS_ID_TOKEN}"
        --header "Content-Type: application/json"
        --data "{\"merge_request_iid\":${CI_MERGE_REQUEST_IID}}"
        https://amiss.example/gitlab/policy/evaluate
      )" = 204
  rules:
    - if: '$CI_PIPELINE_SOURCE == "merge_request_event" && $CI_MERGE_REQUEST_EVENT_TYPE == "merge_train"'
    - when: never
```

Replace the image and endpoint placeholders with operator-owned, immutable values. Do not add a
fallback command, an alternate success path, or a project-controlled variable that can change the
endpoint, audience, headers, body, job name, or rules. The service also pins the exact
`job_config.url` and SHA reported in the OIDC token, so a job copied into the checked repository
does not authenticate as the policy job. The event-type test matters: detached and merged-result
merge-request pipelines also report `merge_request_event`, but only a merge-train pipeline reports
`CI_MERGE_REQUEST_EVENT_TYPE=merge_train`. The explicit status test rejects every response other
than `204`, including a successful-looking redirect, proxy page, or alternate 2xx response.

The job needs no GitLab API or repository credential. Its OIDC token is short-lived and specific
to that job. The controller keeps its API and Git credentials outside the pipeline.

## Credentials and OIDC keys

Use separate controller-owned credentials:

- an API token with read access to the configured project, jobs, pipelines, merge trains, merge
  requests, branches, commits, and protected-branch settings; and
- an HTTPS Git credential with read access to both the checked project and the pinned action
  repository.

Store each token as exact bytes in a private regular file. A trailing newline is part of the token
and makes the configuration invalid. The Git username is explicit; for a personal, project, or
group access token it is commonly `oauth2`, but use the value required by the chosen GitLab
credential.

The service does not fetch OIDC keys at runtime. Export the instance's current RSA signing keys
from its JWKS, review them, and pin each public key in a private file with its exact `kid` and a
local anchor name. One through sixteen unique keys are accepted. The issuer must be the configured
GitLab HTTPS instance, and the API root must be exactly `/api/v4`.

For key rotation, add the new key beside the old key and restart the service before GitLab starts
using it. Keep the old key until every job token it signed has expired and any in-flight request
has finished, then remove it and restart again. Removing a key revokes requests signed only by
that key.

The policy also names trusted runners. Enable GitLab-hosted runners only when they are part of the
deployment's trust boundary. Otherwise list the exact positive self-hosted runner IDs. A generic
“self-hosted” claim without a listed ID is rejected.

## Build and run

Build the nested workspace from source:

```sh
cargo build --manifest-path controller/Cargo.toml --release --locked \
  -p amiss-controller-gitlab-service --bin amiss-controller-gitlab
```

Pre-create the private scratch and ledger directories, then pass one absolute config path:

```sh
controller/target/release/amiss-controller-gitlab /etc/amiss/gitlab.json
```

The service listens on plain HTTP. Bind it to loopback or a private network and put an
operator-controlled TLS terminator in front. The proxy must preserve the `Authorization` header
and exact body and must cap connections plus total, header, body, idle, and slow-body time. Set
the policy job timeout above the service's API, Git, and bootstrap deadlines. `/healthz` reports
only process liveness.

`max_concurrent_evaluations` is an in-process cap from 1 through 64. The service takes a permit
after validating headers and before reading the body, then holds it through the complete blocking
evaluation. Capacity exhaustion returns `503`; the proxy's connection cap is still required.

## Configuration

Configuration is strict JSON. Unknown and duplicate fields are errors. All file and directory
paths are absolute. The scratch and ledger roots must already exist as separate real directories
outside the repository and action trees, and the bootstrap must match the loaded execution
constraint.

```json
{
  "listen": "127.0.0.1:8080",
  "evaluation_path": "/gitlab/policy/evaluate",
  "max_concurrent_evaluations": 4,
  "gitlab": {
    "instance": "gitlab.example",
    "api_base": "https://gitlab.example/api/v4",
    "api_token_file": "/etc/amiss/gitlab-api.token",
    "git": {
      "username": "oauth2",
      "token_file": "/etc/amiss/gitlab-git.token"
    },
    "oidc": {
      "issuer": "https://gitlab.example",
      "audience": "https://amiss.example/gitlab/policy/evaluate",
      "trust_set": "gitlab-oidc",
      "keys": [
        {
          "kid": "current",
          "anchor": "gitlab-key/current",
          "public_key_file": "/etc/amiss/gitlab-oidc-current.pem"
        }
      ]
    }
  },
  "policy": {
    "integration": "pipeline-execution-policy/1",
    "project_id": 101,
    "project_path": "acme/widget",
    "target_branch": "main",
    "job_name": "amiss:policy",
    "config_url": "https://gitlab.example/security/policies/-/blob/ffffffffffffffffffffffffffffffffffffffff/.gitlab/security-policies/policy.yml",
    "config_commit": "ffffffffffffffffffffffffffffffffffffffff",
    "gitlab_hosted_runners": true,
    "self_hosted_runner_ids": []
  },
  "plan": {
    "profile": "enforce",
    "execution_constraint_file": "/etc/amiss/execution-constraint.json",
    "organization_floor_file": "/etc/amiss/organization-floor.json",
    "debt_snapshot_file": null,
    "waiver_bundle_file": null
  },
  "paths": {
    "bootstrap": "/opt/amiss/amiss-bootstrap",
    "scratch": "/var/lib/amiss/scratch",
    "ledger": "/var/lib/amiss/ledger"
  }
}
```

`project_path` is the lowercase path with its complete nested group prefix. `target_branch` is one
branch name, not a full ref. `job_name` is the exact live GitLab job name. `config_url` and
`config_commit` must reproduce the policy job's `job_config` OIDC claims exactly. `config_url` is
GitLab's blob URL for the security-policy YAML, not the included CI file; copy the exact claim
rather than assembling the URL by hand.
`integration` is a controller identity for this policy binding; change it when the policy trust
boundary changes.

Only a root-mounted HTTPS instance without an explicit port is supported. The API root may end in
`/api/v4` or `/api/v4/`; credentials, alternate paths, query strings, fragments, redirects, and
insecure TLS are rejected. The checked project and action repository must both use that instance
and SHA-1.

The optional `limits` object overrides execution defaults:

| Fields | Defaults |
| --- | --- |
| `body_bytes`, `header_count`, `header_bytes` | 2 MiB, 64, 32 KiB |
| `queue_age_seconds`, `future_skew_seconds` | 86,400, 5 |
| `ledger_lease_seconds`, `ledger_records` | 60, 50,000 |
| `api_connect_millis`, `api_read_millis`, `api_write_millis` | 5,000, 15,000, 15,000 |
| `api_request_millis`, `git_request_seconds` | 20,000, 120 |
| `bootstrap_seconds`, `statement_validity_seconds` | 120, 300 |

`queue_age_seconds` remains part of the authenticated replay window; it does not create a raw
request queue for this synchronous lane.
The GitLab endpoint further clamps the effective body to 1 KiB and the header count to 32; a
smaller configured value still wins. Future skew cannot exceed 300 seconds, the API request
deadline must fit inside the ledger lease, and the bootstrap limit cannot exceed 120 seconds.
Provider responses share one 4 MiB budget per refresh. Git acquisition uses the fixed pack,
object, inflated-byte, resolved-byte, delta-depth, and one-thread indexing limits listed in the
[GitHub lane](provider-github.md#configuration).
Protected-branch lookup stops at ten pages of 100 rows; an unfinished or oversized list fails
closed.

The underlying shared ceilings are 8 MiB per request body, 128 headers, 32 KiB of aggregate
header bytes, 100,000 ledger rows, and 64 concurrent evaluations; GitLab's smaller endpoint clamps
win where they overlap. HTTP phase and operation timeouts cannot exceed 30 seconds, and Git
acquisition cannot exceed 120 seconds.

## HTTP result and replay

The endpoint returns:

| Status | Meaning |
| --- | --- |
| `204` | The exact result was saved, the final refresh still matched, and the conclusion was pass. |
| `400` | The configured endpoint was called with a query string. |
| `401` | The merge-request hint or OIDC bearer token, signature, key, time, or required claims were invalid. |
| `403` | The authenticated request did not select the configured provider route or check plan. |
| `412` | The controller completed without an exact published pass, including block, unavailable, stale, busy, or duplicate work. |
| `413` | The body limit was crossed. |
| `431` | The header count or byte limit was crossed. |
| `503` | Capacity, trusted time, storage, provider access, acquisition, or evaluation was unavailable. |

The service checks bounds, OIDC, and the configured plan before opening the ledger or starting API,
Git, or runner work. The policy job must treat only `204` as success. Do not turn `400`, `401`,
`403`, `412`, or `503` into a warning or retry them inside the same script. A GitLab job retry
receives a new job and token; the adapter will bind that new run independently if the merge-train
car is still active.

There is no raw webhook inbox. The synchronous request remains open while the controller uses its
ordinary-file ledger, acquires both repositories, runs the bootstrap, and performs the final
refresh. The OIDC `jti`, runner ID, and authenticated issue time form a bounded replay identity.
Completed rows remain through their inclusive replay end and can then be removed by ledger
cleanup. A clock rollback cannot reopen an expired row.

The final “publication” step makes no GitLab API write. It refreshes the same job and gate one last
time, stages that exact result in the ledger, and lets the endpoint status decide the already
running policy job. The local record and HTTP response are not one transaction. If the service
completed but the `204` reply was lost, replaying the same token and request does not invent a
second success; it fails closed as a duplicate.

For a policy, plan, bootstrap, control, project, target, job, runner, or action change, update the
independently owned policy configuration, pin its new `config_commit`, choose a new `integration`
identity, and restart the service with the matching files. Existing train jobs from the old
policy commit are rejected. Preserve the ledger until its bounded replay rows have expired.

## What the job proves

A successful job proves that the independently owned policy supplied the configured job, GitLab
signed its exact job and train claims, the live project still enforced the required merge path,
and the service accepted an Amiss pass for that exact train-result tree. The API token, Git
credential, OIDC keys, policy project, runner set, protected-branch administrators, GitLab
instance, service host, TLS boundary, bootstrap, controls, scratch root, and ledger root remain
inside the trust boundary.

The engine report remains unchanged and self-asserted. It has no provider signature or
`provider_verified` field. GitLab origin lives in the protected policy job and enforced merge
train, not in copied report bytes.
