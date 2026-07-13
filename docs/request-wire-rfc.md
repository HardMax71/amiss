# Request-wire RFC: the wrapper-to-engine input lane

Status: accepted for the experimental lane. This RFC publishes the input lane that
[machine-contracts.md](./machine-contracts.md#digest-registry) reserved and
[scanner-v0-spec.md](./scanner-v0-spec.md) required before any wrapper may exist: root request
schemas, framing laws, handle ordering, and the acceptance law. It amends the dossier by defining
that lane; it does not open the provider gates listed at the end, and nothing built on this RFC may
be registered or described as a required stable check until those gates open under their own
review.

## What this lane is

The scanner engine evaluates exactly what a trusted wrapper hands it. Until now the only lawful
invocation was the disposable public CLI, whose grammar has no control-supply surface and whose
report states every external control as `none`. The dossier reserved three request-digest domains
for a future wrapper-to-engine API and ruled that hidden flags, environment variables, and
implementation-private streams cannot assert external trust. This RFC defines the only input lane:
three request documents, each one complete bounded byte stream with a published schema, consumed by
the engine exactly as written.

The trust boundary is the request itself. Whoever composes the requests asserts the embedded trust
sources and expected digests; the engine verifies internal consistency (every semantic digest
recomputes, every binding law holds) but performs no provider authentication. In the future
required deployment the provider-controlled wrapper composes the requests from authenticated
sources. In the experimental lane the caller composes them, which is why the experimental binary
cannot be a required check.

## The three request documents

| Request | Schema | Digest domain |
| --- | --- | --- |
| Evaluation | [scanner-evaluation-request-v1](./spec/scanner-evaluation-request-v1.schema.json) | `HB("assure/scanner-evaluation-request/v1", exact bounded request bytes)` |
| Snapshot | [scanner-snapshot-request-v1](./spec/scanner-snapshot-request-v1.schema.json) | `HB("assure/scanner-snapshot-request/v1", exact bounded request bytes)` |
| Controls | [scanner-controls-request-v1](./spec/scanner-controls-request-v1.schema.json) | `HB("assure/scanner-controls-request/v1", exact bounded request bytes)` |

The evaluation request carries the run identity: profile, mode, object format, nullable repository
and refs, and the exact commit OIDs. `candidate_commit_oid` is null exactly when `mode` is
`index`. The snapshot request carries materialization: `git-objects` pairs with `commit-pair` and
`index` with `index`; any other pairing is `INVALID_INVOCATION`. Its `repository_handle` is the
constant ordinal `3` from the handle table below, and `pre_acquired: true` restates the networkless
promise: every object the evaluation needs, including adoption-reproduction objects, is already in
the primary object database before the engine starts. The controls request carries the five
external controls; each supplied member embeds the exact control JSON value beside the
independently acquired expected semantic `HJ` digest and its external trust source, and absent
members are null.

## Framing laws

Each request is one complete byte stream read from byte zero through EOF under a
16,777,216-byte cap. The diagnostic request digest is the domain-tagged `HB` over those exact
bytes and is non-null exactly when EOF was obtained within the cap; prefix digests, shell quoting,
environment variables, and display strings are forbidden. Request digests are diagnostic
identities, never accepted configuration. After capture, each stream is parsed under the strict
JSON laws every control already obeys: UTF-8, duplicate keys rejected, unknown fields rejected,
closed consts and enums exact.

A stream that cannot be fully captured, or whose bytes fail strict parsing or its root schema, or
whose cross-request consistency laws fail, makes the evaluation unavailable: the payload carries
the unavailable evaluation with all applicable reasons in enum order (`request-unreadable` for
capture and parse defects, `invalid-invocation` for consistency defects, `invalid-profile` where
the profile value itself is the defect), the matching invocation-phase anchor errors, and each
stream's request digest where capture completed. The run is incomplete, exit 2. Producers never
speculate about unseen bytes: a `request-unreadable` reason may coexist only with defects
established safely before the unreadable boundary.

Embedded control values are verified after the requests parse, in the fatal order the dossier
already fixes. The canonical bytes of each embedded value are bounded by `control-input-bytes`
before typed parsing. The recomputed semantic digest of each parsed control must equal its
`expected_digest`; a mismatch is `configuration`/`DIGEST_MISMATCH`/null path, makes controls
unavailable with `invalid-external-control`, and exits 2 with the resolved snapshot identities in
the report. Every downstream law is unchanged: floor binding, tightened ceilings, debt and waiver
binding with adoption reproduction, trusted-time verification against the candidate identity, and
the trace and exception laws all apply exactly as published.

## Handle ordering

The future subprocess lane passes process handles positionally so no path, environment variable,
or flag carries semantic input. The table is published now so request bytes and goldens are
identical across lanes:

| Handle | Ordinal |
| --- | --- |
| Repository worktree root, opened read-only, no-follow | 3 |
| Report output | 4 |
| Private temporary directory | 5 |
| Evaluation request stream | 6 |
| Snapshot request stream | 7 |
| Controls request stream | 8 |

The experimental in-process lane supplies the same values through the launcher grammar below; the
snapshot request's `repository_handle` stays the constant `3` in both lanes. The evaluator still
starts with an empty process-environment mapping and consults no ambient configuration.

## The experimental launcher

The experimental binary is `amiss-wrapper`. Its grammar is fixed:

```
amiss-wrapper check
  --repository <path>
  --evaluation-request <file>
  --snapshot-request <file>
  --controls-request <file>
  [--output <file>]
```

`--repository` names the primary non-bare repository root under the frozen handle-acquisition
contract. The three request files are the streams above. The accepted envelope is written to
`--output` when given and to standard output otherwise; nothing else is written, and the
repository is never modified. Usage defects follow the invocation error contract: exit 2 with the
fatal unavailable-evaluation envelope. Exit classes are the frozen 0/1/2 process contract.

The launcher composes nothing and authenticates nothing: it reads the requests, runs the engine
in-process, and applies the acceptance law to the produced envelope.

## The acceptance law

A wrapper accepts a result only when it can parse one complete schema version and verify:

- the payload-only digest recomputes and equals the envelope's `payload_digest`;
- the evaluated identities equal the request: base commit, and candidate commit for
  `commit-pair`;
- the engine digest equals the wrapper's own engine provenance;
- the organization-floor digest equals the supplied expected digest whenever a floor was supplied
  and the controls resolved;
- the completeness flag agrees with the exit class, and `finding_count` equals the findings array
  length.

The experimental binary applies this law to its own emitted envelope before publishing any bytes;
a violation is `REPORT_CONSTRUCTION_FAILED`, exit 2, and no accepted envelope. Text printed before
a crash is never interpreted as a result.

## What stays blocked

This RFC deliberately does not open, and the experimental binary does not implement:

- provider authentication and event folding: no GitHub App identity, no authenticated
  owner/name/event derivation, no `UNSUPPORTED_PROVIDER_HOST`/`INVALID_EVENT` sources beyond the
  local derivations;
- organization-source acquisition: the wrapper duties of fetching floors, debt, waivers, and
  constraint descriptors from rulesets or required-workflow sources and verifying that those
  sources apply to the current repository and ref;
- trusted-time issuance: the controller workflow, its provider authentication, and the
  pre-publication recheck against the controller clock;
- networked object pre-acquisition: shallow or provider framing for materializing adoption and
  snapshot objects;
- sandbox verification receipts and the `stable-v1` execution-constraint cross-bindings, including
  required-status registration.

Each remains a target property under its own gate. Until they open, every run through this lane is
experimental, its compatibility field stays `experimental`, and its trust extends exactly as far
as whoever composed the requests.
