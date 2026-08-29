# Retained provider artifacts

Provider summaries are intentionally short. A publication can carry more findings than GitHub's
Check Run or a Gitea-family review should display, and an external assessment is useful only with
the exact plan and evidence that produced it. Each provider lane therefore retains those bytes in
an operator-owned artifact store before it stages or publishes the result.

The store is evidence retention, not replay or acceptance authority. The
[`FileLedger`](file-ledger.md) still decides which delivery may run, retry, or complete. Repository
content cannot configure the artifact root, URL, token, lifetime, or capacity, and retained bytes
never suppress or approve a later finding.

## Published binding

Every report-bearing provider publication binds all of the following:

- the canonical report digest;
- when semantic evidence was accepted, the canonical semantic-input audit digest;
- an HTTPS report locator;
- the authorization scheme, `bearer`;
- the exclusive expiry instant in Unix milliseconds; and
- when external verification completed, the canonical assessment digest.

GitHub Check Runs and Gitea-family reviews carry these as `report`, `artifact`,
`artifact-auth`, `artifact-expires-unix-millis`, and optional `semantic-input`,
`semantic-input-artifact`, and `assessment` lines. GitHub leaves the Check Run's native details URL
unset because that browser link cannot supply the required bearer header; authorized clients use
the locator in the summary. A completed external assessment adds its direct `assessment-artifact`
locator and its refuted, unproven, and reachable counts; an incomplete one is named as incomplete
instead of inventing counts. GitLab returns the report locator as
`Link: <...>; rel="amiss-report"`, the semantic-input sibling as `rel="amiss-semantic-input"`, and
the assessment sibling as `rel="amiss-assessment"` when each exists. It returns authorization,
expiry, assessment state, and completed counts as `X-Amiss-*` headers. The component digests are
`X-Amiss-Report-Digest`, `X-Amiss-Semantic-Input-Digest`, and
`X-Amiss-Assessment-Digest`. The report URL always ends in `/<artifact-id>/report`; retained
semantic inputs use the sibling `/semantic`, and exact external inputs use `/plan`, `/evidence`,
and `/assessment`.

The controller writes the exact report, accepted semantic-input audit value, and optional external
chain before its final provider refresh and publication stage. The artifact identity binds the
evaluation ID, every component digest, and the external outcome. A retry first verifies the saved
reference and every retained component, then republishes the already staged value. It never reruns
the scanner, semantic producer, or external verifier. A changed head or gate after verification
stages a superseded result with the retained chain, not the old pass or block. Rebinding one
evaluation ID to different bytes is an error.

If retention, validation, or retrieval cannot be trusted, a new publication fails closed. A
summary without a retained locator says extra findings are “not displayed”; it never claims that
an inaccessible report exists. Expiry cannot change a provider verdict that already completed.
After expiry, a duplicate delivery may still be recognized by the delivery ledger but no longer
advertises an artifact.

## Authenticated retrieval

The configured `base_url` must be a canonical HTTPS URL with a non-root static path and no
credentials, query, fragment, empty path segment, or trailing slash. Route segments use only
letters, digits, `-`, `.`, `_`, and `~`. The TLS proxy must forward that path and preserve the
`Authorization` header without exposing the three private operator endpoints.

The service reads one 32-to-256-byte bearer token from a bounded regular file at startup and keeps
only a keyed verifier in memory. Give the token only to authorized authors or operators. Retrieve
the report exactly as published:

```sh
AMISS_ARTIFACT_TOKEN="$(</etc/amiss/artifact.token)"
curl --fail --silent --show-error \
  --header "Authorization: Bearer ${AMISS_ARTIFACT_TOKEN}" \
  'https://amiss.example/amiss/artifacts/<artifact-id>/report' \
  --output report.json
```

When the publication advertises a semantic-input component, retrieve its exact source templates
and candidate-bound envelopes with the same token from
`https://amiss.example/amiss/artifacts/<artifact-id>/semantic`. Recompute the advertised digest
before using any component as audit evidence.

Token files are exact bytes and cannot contain whitespace, including a trailing newline. Changing
the token requires a service restart but not a new artifact root. If consumers must keep access to
old locators, retain the old token until their published expiry instants.

An authorized `GET` returns the unchanged JSON bytes with `Content-Type: application/json`,
`Cache-Control: private, no-store`, and `X-Content-Type-Options: nosniff`. Missing or wrong
authorization returns `401`; an unknown component or expired artifact returns `404`; a query or
oversized header set returns `400`; unavailable storage or request capacity returns `503`.
Artifact requests share the configured endpoint header and concurrency bounds but do not change
the fixed provider-request counters. Their own semaphore additionally derives a response-memory
cap from `artifact_record_bytes`: at most 128 MiB of configured component bounds can run together,
except that one explicitly allowed component may itself be as large as the fixed 256 MiB machine
JSON ceiling. This is a concurrency bound, not a startup allocation.

## Storage and limits

`paths.artifacts` names a fourth pre-created private local directory, separate from scratch,
ledger, and the webhook inbox. One process owns the root. Shared and network filesystems,
symlinks, unknown entries, malformed metadata, missing payloads, and digest mismatches fail closed.
Metadata is checksummed, payloads are digest-checked, creation is metadata-last, and deletion is
metadata-first, so an interrupted operation is either recoverable debris or a complete record.

The optional execution-limit fields are:

| Field | Default | Hard ceiling |
| --- | ---: | ---: |
| `artifact_retention_seconds` | 604,800 (7 days) | 31,536,000 (365 days) |
| `artifact_records` | 1,000 | 100,000 |
| `artifact_bytes` | 1 GiB | 64 GiB |
| `artifact_record_bytes` | 64 MiB | 1 GiB |

All values must be positive, and one record's limit cannot exceed the total-byte limit. Record and
byte accounting includes metadata plus every retained component. Full capacity rejects new
evidence; it never evicts a live artifact. At the exact expiry instant the artifact becomes
inaccessible. Startup and store operations remove expired records, and a persisted clock high-water
mark prevents already removed bytes from returning after clock rollback.

The base URL, retention, and capacity limits are recorded with a root. Changing any of them
requires a new empty artifact root. Size the record limit for the largest report, semantic-input
audit value, and external chain the lane is allowed to publish, and size total bytes plus record
count for the expected publication rate over the retention period.
