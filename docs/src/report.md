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
`findings` array; and the `errors` array of analysis errors the run kept. Every finding
carries its kind, its location with byte offsets, its attribution, the policy steps that
set its final disposition, and the digests of the facts underneath it. The `key_input`
that produced the finding's identity is included, so an external system can recompute any
finding's identity from the report alone.

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
meaning only with a version bump.
