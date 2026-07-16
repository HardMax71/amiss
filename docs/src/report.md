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
`complete`, and `exit_code`; the summary counts; a `documents` array with one row per
discovered document, its classification, and whether its content was available; the
`findings` array; and the `errors` array of analysis errors the run kept. A repository
path in any of these is a plain string when its bytes are valid UTF-8, and otherwise an
object of the form `{"bytes_hex": "…"}` naming the raw bytes as lowercase hex; a writer
never uses the object form for bytes that decode as text, so one path has exactly one
spelling and every derived digest stays whole. Every finding
carries its kind, its location with byte offsets, its attribution, the policy steps that
set its final disposition, and the digests of the facts underneath it. The `key_input`
that produced the finding's identity is included, so an external system can recompute any
finding's identity from the report alone.

The envelope, down to its top-level keys:

```json
{
  "schema": "amiss/scanner-report-envelope",
  "payload": {
    "schema": "amiss/scanner-report-payload",
    "compatibility": "experimental",
    "engine": { "engine_digest": "sha256:…" },
    "evaluation": {},
    "controls": {},
    "result": { "status": "fail", "complete": true, "exit_code": 1 },
    "summary": {},
    "documents": [],
    "observations": [],
    "findings": [],
    "errors": []
  },
  "payload_digest": "sha256:…"
}
```

And one finding row from a real failing run, abridged to its skeleton:

```json
{
  "kind": "explicit-target-missing",
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

Findings are sorted by finding key, a domain-separated hash of kind plus scope. The human
format prints the same facts in the same order, replaces every byte outside printable
ASCII with a `\uXXXX` escape so a hostile filename cannot inject terminal control codes or
a forged CI command into a log, and stops after two hundred findings. The JSON is never
cut short: a serialized report that would cross the 64 MiB `machine-json-bytes` ceiling
ends the run incomplete with `OUTPUT_LIMIT_EXCEEDED` instead of shortening the list, and
the findings count has its own separate ceiling in [Limits and refusals](limits.md).

The machine contract is the
[current report schema](../../spec/scanner-report.schema.json), its
[readable example](../../spec/examples/scanner-report.json), and the corresponding
[canonical bytes](../../spec/examples/scanner-report.canonical.json). The test suite validates
emitted bytes with an independent schema validator, checks the canonical example, and checks
that the schema identifiers match the writer constants in the
[documentation contract test](../../crates/amiss/tests/documentation_contracts.rs).

This is one rolling, unversioned wire contract during the pre-1.0 `experimental` series.
Only the unsuffixed schema and examples linked above describe public report output. The
schema, examples, parsers, and writer change together. Consumers that need a stable
integration must pin an Amiss release and its shipped schema.
