# The report

`--format json` emits one line: the canonical JSON of the report envelope, then one newline,
and nothing else on stdout, ever. Canonical means [RFC 8785](https://www.rfc-editor.org/rfc/rfc8785)
JCS with a restricted profile: sorted keys, no floats where the contract says integer, no
duplicate keys accepted anywhere on input. The same repository and the same two commits
produce byte-identical reports on Linux, macOS, and Windows.

The envelope carries the payload plus a `payload_digest` over its canonical bytes, an
`engine_digest` naming the exact binary that produced it, and a `compatibility` field that
says `experimental` for the v0 series. Digests are domain-separated: every hash in the system
is computed over a domain string plus the canonical bytes, so a digest from one context can
never be replayed as a digest from another.

Inside the payload: the evaluation identity (which trees, which mode, which profile), the
result block (`status`, `complete`, `exit_code`), the summary denominators, the `documents`
array with one row per discovered document and its classification and availability, the
`findings` array in canonical order, and the `errors` array of retained analysis errors. Every
finding carries its kind, location with byte spans, attribution, the policy steps that led to
its effective disposition, and the digests of the facts it rests on. The `key_input` that
produced each finding key is in the row, so an external system can recompute the identity of
any finding from the report alone.

Findings sort by finding key, which is a domain-separated digest of kind plus scope. The
human projection prints the same facts in the same order, escapes every scalar outside
printable ASCII as a `\uXXXX` sequence so a hostile path cannot smuggle terminal control
bytes or a forged workflow command into a log, and stops at two hundred findings. The JSON is
never truncated; when the findings ceiling of the contract is crossed, the run is incomplete
with `OUTPUT_LIMIT_EXCEEDED` rather than silently shortened.

The report schema itself is versioned and shipped in the repository under
[`spec/`](https://github.com/HardMax71/amiss/tree/main/spec), together with canonical example
vectors. The test suite validates emitted bytes against that schema with an independent
validator, and the schema is the compatibility contract: a field appears, moves, or changes
meaning only with a version bump.
