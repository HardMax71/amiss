# The report

`--format json` writes exactly one line to stdout: the canonical JSON of the report, then a
newline. Canonical means [RFC 8785](https://www.rfc-editor.org/rfc/rfc8785) canonical JSON:
keys sorted, one byte sequence per possible document, so the same input through the same
engine binary always produces the same bytes. The payload facts agree across platforms; the
envelope's own digests differ by build, because they name the exact binary that ran. Duplicate keys are rejected everywhere on input, and
the contract's numbers are integers, never floats.

The outer envelope has three members: its schema, the payload, and `payload_digest`, a hash
of the payload's canonical bytes. The payload carries its own schema, `compatibility`
(`experimental` for the v0 series), and an engine block whose `engine_digest` names the
binary that produced it. Every digest in the system is domain-separated, meaning the hash
input starts with a label naming its purpose, so a digest computed for one context cannot be
replayed as a digest for another.

Inside the payload: which trees were compared and how; the result block with `status`,
`complete`, and `exit_code`; the PR-facing `feedback` projection; the summary counts; a `documents` array with one row per
discovered document, its classification, and whether its content was available; the
`findings` array; and the `errors` array of analysis errors the run kept.

The evaluation records `candidate_ref` and `target_ref` separately. The candidate ref is the
source branch used for same-repository URL resolution; the target ref is the protected branch
to which branch-scoped controls were matched. Either may be null on a local, self-asserted run,
and the direct CLI currently leaves the target null. Both values enter the candidate-identity
preimage. They describe the exact inputs the engine evaluated; their presence does not prove who
selected or authenticated them.
The current sealed commit-pair path still reports `explicit-commit-pair` and
`explicit-replay`; no provider adapter supplies a provider event kind or finality today.

A repository path anywhere in the payload has exactly one spelling. Valid UTF-8 bytes
travel as a plain string; anything else travels as `{"bytes_hex": "..."}` naming the raw
bytes as lowercase hex. A writer never uses the object form for bytes that decode as
text, so every derived digest stays whole.

Every finding carries its kind, its location with byte offsets, its attribution, the
policy steps that set its final disposition, and the digests of the facts underneath it.
The `key_input` that produced the finding's identity is included too, so an external
system can recompute any finding's identity from the report alone.

`feedback` is the smaller review surface derived by the engine from those exact findings.
Related introduced problems become one `fix` per target, changed targets under unchanged
prose become one `check`, and `existing_count` counts grouped pre-existing subjects without
turning them into items. Each item retains its affected-location count and contributing
finding kinds. A Fix may carry one candidate-side text-path annotation; Checks never do.
The report retains every item. An incomplete comparison instead emits exactly
`{"status":"unavailable"}`, so scan failure cannot look like zero feedback.

The envelope, down to its top-level keys:

```json
{
  "schema": "amiss/scanner-report-envelope",
  "payload": {
    "schema": "amiss/scanner-report-payload",
    "compatibility": "experimental",
    "engine": { "engine_digest": "sha256:..." },
    "evaluation": {},
    "controls": {},
    "result": { "status": "fail", "complete": true, "exit_code": 1 },
    "feedback": { "status": "available", "items": [], "existing_count": 0 },
    "summary": {},
    "documents": [],
    "observations": [],
    "findings": [],
    "errors": []
  },
  "payload_digest": "sha256:..."
}
```

And one finding row from a real failing run, abridged to its skeleton:

```json
{
  "kind": "explicit-target-missing",
  "description": "a reference names a repository path, or a line range inside one, that the named tree does not hold; restore the target or correct the link",
  "attribution": "introduced",
  "effective_disposition": "fail",
  "location": {
    "path": "docs/src/introduction.md",
    "side": "candidate",
    "span": { "start_line": 49, "start_column": 1, "end_line": 49, "end_column": 38,
              "start_byte": 2912, "end_byte": 2949 }
  },
  "finding_key": "sha256:56a75485757d90b5959298c05f6b0531139b016533db320905ee532e5dd42512"
}
```

Findings are sorted by finding key, a domain-separated hash of kind plus scope. Every
finding and error row carries a `description`: the fixed engine-owned sentence for its
kind or code, stating what the row means and what to do about it, so no consumer needs a
second source to act on a report. The sentences live in one place,
[`FindingKind::meaning` and `AnalysisErrorCode::meaning`](https://github.com/HardMax71/amiss/blob/main/crates/amiss-wire/src/report.rs);
the lists in [Profiles and findings](profiles.md) and [Limits and refusals](limits.md)
and the shipped example are checked against that source in CI. The human format prints
the result plus at most ten grouped feedback items, replaces every byte outside printable ASCII with a
`\uXXXX` escape so a hostile filename cannot inject terminal control codes or a forged CI
command into a log, and states any overflow explicitly. It keeps raw totals and prints
descriptions only for errors; finding kinds and their descriptions stay in JSON. The JSON is never
cut short: a serialized report that would cross the 64 MiB `machine-json-bytes` ceiling
ends the run incomplete with `OUTPUT_LIMIT_EXCEEDED` instead of shortening the list, and
the findings count has its own separate ceiling in [Limits and refusals](limits.md).

The report is evidence of engine evaluation, not a self-authenticating provider attestation. A
control row with `status: "verified"` means that the engine accepted the supplied digest and
repository, target-ref, tree, time, or run relationships required for that control. A caller
that can supply the request can still make those assertions; the enum does not identify or
authenticate the caller. The sealed bootstrap additionally checks the requested identities and
digests against the returned envelope, but republishes the accepted bytes unchanged. The
provider-controller foundation likewise defines orchestration and publication boundaries but
does not yet sign a report or implement a provider-published check. Independent provider
evidence therefore does not exist in the current report contract.

Sandbox provenance is separate again. The present writer reports `self-asserted` assurance,
`local-process` enforcement, and null verification. The sealed bootstrap requires that honest
projection. Runtime-closure validation, a cleared environment, fixed input, and a watchdog do
not satisfy the report schema's provider-verified OCI or microVM mechanisms.

The machine contract is the
[current report schema](https://github.com/HardMax71/amiss/blob/main/spec/scanner-report.schema.json), its
[readable example](https://github.com/HardMax71/amiss/blob/main/spec/examples/scanner-report.json), and the corresponding
[canonical bytes](https://github.com/HardMax71/amiss/blob/main/spec/examples/scanner-report.canonical.json). The test suite validates
emitted bytes with an independent schema validator, checks the canonical example, and checks
that the schema identifiers match the writer constants in the
[documentation contract test](https://github.com/HardMax71/amiss/blob/main/crates/amiss/tests/documentation_contracts.rs).

This is one rolling, unversioned wire contract during the pre-1.0 `experimental` series.
Only the unsuffixed schema and examples linked above describe public report output. The
schema, examples, parsers, and writer change together. Consumers that need a stable
integration must pin an Amiss release and its shipped schema.
