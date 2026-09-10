# amiss-fixtures

Shared test fixtures for the Amiss workspace, including writers that put hostile bytes
straight into Git object stores and index files so the same fixtures exist on every
platform. Internal and unpublished.

data/github-webhook-repository.json is the unchanged repository member from
[Octokit's published workflow completion example](https://github.com/octokit/webhooks/blob/7dd7fa56498a827a08b71919fae89428f5e8e283/payload-examples/api.github.com/workflow_run/completed.with-pull-requests.payload.json),
pretty-printed with a trailing newline. It is a published example, not a live
capture. Signed test producers change identity fields explicitly after decoding;
their resulting events are synthetic, not unchanged upstream deliveries.

data/github-webhook-check-run.json is the complete, unchanged
[published completed check-run event](https://github.com/octokit/webhooks/blob/7dd7fa56498a827a08b71919fae89428f5e8e283/payload-examples/api.github.com/check_run/completed.payload.json).
The signed ingress and JSON contract tests share it, including both app records,
the nested check suite, and pull-request references.
