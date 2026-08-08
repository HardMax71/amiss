# The report

`--format json` writes exactly one line to stdout: the canonical JSON of the report, then a
newline. Canonical means [RFC 8785](https://www.rfc-editor.org/rfc/rfc8785) canonical JSON:
keys sorted, one byte sequence per possible document, so the same input through the same
engine binary always produces the same bytes. The payload facts agree across platforms; the
envelope's own digests differ by build, because they name the exact binary that ran. Duplicate keys are rejected everywhere on input, and
the contract's numbers are integers, never floats.

The outer envelope has three members: its schema, the payload, and `payload_digest`, a hash
of the payload's canonical bytes. The payload carries its own schema, `compatibility`
(the wire's own version, `experimental` while the contract rolls), and an engine block whose `engine_digest` names the
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
The sealed commit-pair path, including every provider lane, still reports
`explicit-commit-pair` and `explicit-replay`. Provider event and publication facts remain outside
the engine report.

A repository path anywhere in the payload has exactly one spelling. Valid UTF-8 bytes
travel as a plain string; anything else travels as `{"bytes_hex": "..."}` naming the raw
bytes as lowercase hex. A writer never uses the object form for bytes that decode as
text, so every derived digest stays whole.

An external destination is recorded where it is seen and nowhere else. The occurrence keeps
the URL in `external_destination`, after the format's own decoding so that
`https://example.com/x?a=1&amp;b=2` is recorded as the address a fetcher would request rather
than as the bytes the source spells. No finding is raised, and the summary counts it
under `external_out_of_scope`, because the engine never fetched it and so decided nothing.
[Amiss and link checkers](comparison.md) shows the one command that turns those rows into a
list for the tool that does fetch.

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
  "description": "a reference names a repository path, a line range inside one, or a heading anchor no known renderer publishes; restore the target or correct the link",
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
second source to act on a report. Beside it sits `fix`, a machine-applicable rewrite or
null, whose own `description` is one of a closed set of engine-owned sentences named by
`FixKind`: when the engine can prove the exact edit, the field names the candidate document,
the byte span to replace, and the replacement text, and a finding whose correct content
is not derivable carries null rather than a guess. Three producers emit one today: the broken
value claim carries its definition respelled to expect the target's current line,
proven by classifying the rewrite back through the claim grammar (see
[Claims](claims.md)), and a lone drifted heading anchor carries its fragment
respelled to the one published identity it names apart from case and separator style,
over bytes the adapter located verbatim, and a lone case-drifted path carries its
written path part respelled to the one tracked path it matches apart from case.
[`amiss fix`](invocation.md) applies these spans to the staged working tree in
place, refusing any document whose bytes moved since the evaluation. The sentences live in one place,
[`FindingKind::meaning`, `AnalysisErrorCode::meaning`, and `FixKind::meaning`](https://github.com/HardMax71/amiss/blob/main/crates/amiss-wire/src/report.rs);
the lists in [Profiles and findings](profiles.md) and [Limits and refusals](limits.md)
and the shipped example are checked against that source in CI. The human format prints
the result plus at most ten grouped feedback items, replaces every byte outside printable ASCII with a
`\uXXXX` escape so a hostile filename cannot inject terminal control codes or a forged CI
command into a log, and states any overflow explicitly. It keeps raw totals and prints
descriptions only for errors; finding kinds and their descriptions stay in JSON. The JSON is never
cut short: a serialized report that would cross the `machine-json-bytes` ceiling
ends the run incomplete with `OUTPUT_LIMIT_EXCEEDED` instead of shortening the list, and
the findings count has its own separate ceiling in [Limits and refusals](limits.md).

`--format sarif` writes exactly one line to stdout: a SARIF 2.1.0 log projected from the
same payload. Every finding row becomes a result under its kind's rule, `fail` as `error`,
`warn` as `warning`, and `record` as `note`, with the row's own `description` as the message,
and a row carrying a `fix` projects it as a SARIF fix with the byte region and replacement,
which GitHub renders as a suggested edit
and the finding key riding as the stable `partialFingerprints` entry, so an ingesting
scanner deduplicates across runs by the same identity the report uses. A location renders
when the wire path is printable text, percent-encoded into the artifact URI so a hostile
path cannot break it. Retained analysis errors become tool execution notifications, an
incomplete run reports `executionSuccessful` false, and a rejected machine invocation
still answers in SARIF with exit class 2. Like the human form, the projection cannot
change facts, ordering, totals, or the exit class; the canonical report stays the only
wire, and consumers that need the full evidence read it there.

`--format codequality` projects the same payload as GitLab's Code Quality artifact: a
JSON array with one issue per finding row in report order, the row's `description` as
the issue text, its kind as `check_name`, and `fail` as `major`, `warn` as `minor`, and
`record` as `info`. The finding key rides as the fingerprint, so GitLab's diff of target
against head recognizes the same finding across runs by the identity the report uses.
GitLab requires a path and a first line on every issue, so a byte-named document answers
with the wire's hex spelling and a byte-only span reads as line one. The format has no
shape for analysis errors or a refusal: a rejected invocation answers with a valid empty
artifact, the exit class still carries the truth, and error detail stays on the JSON and
human lanes. The same projection bounds apply.

The report is evidence of engine evaluation, not a self-authenticating provider attestation. A
control row with `status: "verified"` means that the engine accepted the supplied digest and
repository, target-ref, tree, time, or run relationships required for that control. A caller that
can supply the request can still make those assertions; the enum does not identify or
authenticate the caller. The sealed bootstrap additionally checks the requested identities and
digests against the returned envelope, but republishes the accepted bytes unchanged.

The [provider lanes](provider-controls.md) leave separate provider evidence: an App-owned Check
Run on GitHub's test merge, a protected GitLab policy-job result on a merge-train commit, or a
dedicated Gitea-family review. GitHub's Check Run and the Gitea-family review carry the staged
summary and report digest; GitLab's provider-visible evidence is the exact policy job's outcome.
The controller's saved result binds the plan, execution constraint, and gate identity in every
lane. When a report is present, it also binds the report digest. No provider signs or adds fields
to the report. Moving the same report bytes away from that gate therefore loses the provider
context; there is still no provider attestation inside the current report contract.

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
[documentation contract test](https://github.com/HardMax71/amiss/tree/main/crates/amiss/tests/documentation_contracts).

The wire is versioned by its own `compatibility` field, not by the engine release:
`experimental` means one rolling contract, and the planned `1` means additive-only from
then on, with the conditions in the [roadmap](roadmap.md). While it rolls, only the
unsuffixed schema and examples linked above describe public report output, the schema,
examples, parsers, and writer change together, and consumers that need a stable
integration must pin an Amiss release and its shipped schema.
