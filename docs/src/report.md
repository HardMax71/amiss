# The report

`--format json` writes exactly one line to stdout: the canonical JSON of the report, then a
newline. Canonical means [RFC 8785](https://www.rfc-editor.org/rfc/rfc8785) canonical JSON:
keys sorted, one byte sequence per possible document, so the same input through the same
engine binary always produces the same bytes. The payload facts agree across platforms; the
envelope's own digests differ by build, because they name the exact binary that ran. Duplicate keys are rejected everywhere on input, and
the contract's numbers are integers, never floats.

The outer envelope carries the payload plus three self-descriptions: `payload_digest`, a
hash of the payload's canonical bytes; `engine_digest`, a hash of the binary that produced
the report; and `compatibility`, which says `experimental` for the v0 series. Every digest
in the system is domain-separated, meaning the hash input starts with a label naming its
purpose, so a digest computed for one context can never be replayed as a digest for
another.

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
  "schema": "amiss/scanner-report-envelope/v2",
  "compatibility": "experimental",
  "engine_digest": "sha256:…",
  "payload_digest": "sha256:…",
  "payload": {
    "evaluation": {},
    "result": { "status": "fail", "complete": true, "exit_code": 1 },
    "summary": {},
    "documents": [],
    "findings": [],
    "errors": []
  }
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

The schema for all of this ships in the repository under
[`spec/`](https://github.com/HardMax71/amiss/tree/main/spec), with canonical example
files. The test suite validates the emitted bytes against that schema using an independent
validator. The schema is the compatibility contract: fields appear, move, or change
meaning only with a version bump, and the move from v1 to v2 is exactly one such change,
the path union above. Every digest computed under a v1 preimage comes out identical under
v2, because a text path serializes to the same bytes in both and the object form was not
producible before, so the identities in old reports stay valid rather than orphaned. The
move to v3 repeats the law for the identity: the host opens from a github.com constant to
any declared spelling, the owner to slash-joined group paths, the evaluation names the
recognition dialect in a nullable `forge` field, and the reference summary counts one
`same_repository` total because a run has exactly one dialect. No v2 writer could emit any
of that, so every inner digest keeps its meaning and its bytes; only the envelope and
payload constants moved.
