# Security model

The repository being scanned is treated as the attacker. Its documents, paths, Git objects,
packfiles, index, and policy file all came from whoever wrote the pull request, and the
scanner's whole job is to be a safe, pure function of those hostile bytes.

The engine executes nothing. No plugin system, no configurable commands, no formatter
calls, no `git` subprocess. A policy file that names a command or a plugin is not a feature
request to decline politely: the field is unknown, the configuration is invalid, the run
ends incomplete, and the emitted report cannot be mistaken for a complete pass. Process creation belongs to
the separate `amiss-bootstrap` executable; it is not a capability exposed by the scanner
engine.

The engine has no network acquisition interface and does not fetch missing objects. It never
writes to the repository, which the
[no-write tests](https://github.com/HardMax71/amiss/blob/main/crates/amiss/tests/no_write.rs)
check both by comparing the tree and by scanning a read-only repository. Attempts to make it
read outside the repository run into the never-follow-links rule described in
[Snapshots](snapshots.md).

Parsers are the biggest attack surface and receive fuzz targets and pinned conformance
corpora. Document byte admission is charged before parsing. Parser node and nesting totals,
however, are measured and charged only after the grammar returns; they are output budgets,
not a general CPU deadline inside the parser. The order is explicit in the
[scan pipeline](https://github.com/HardMax71/amiss/blob/main/crates/amiss-scan/src/scan.rs).
One budget does act inside the parse: every candidate close of an MDX code region charges
the accumulated region against the `aggregate-embedded-code-evaluation-bytes-per-snapshot`
ceiling before the lexical scan reads it, which bounds the one measured quadratic case.
The history of that case is in the
[corpus notes](https://github.com/HardMax71/amiss/blob/main/corpus/README.md).

A Markdown parser panic is caught and converted to `PARSER_PANIC` against the document that
caused it instead of aborting the process. The known panic fixtures live in the conformance
corpus and tests pin that classification. This protects the run from that failure mode; it
does not turn the post-parse node limits into a wall-clock guarantee.

Output is part of the surface too. Repository paths end up in terminals and CI logs, so
the human format escapes every byte outside printable ASCII. An ANSI escape sequence, a
carriage return, or a forged `::error::` workflow command embedded in a filename reaches
the log only as harmless `\uXXXX` text. A path that is raw bytes rather than text renders
each such byte as the two-digit escape of its value, never inventing a character the
bytes never encoded. The JSON report keeps fidelity its own way, the exact original
string for a UTF-8 path and a `bytes_hex` object for anything else, because the log needs
safety and the report needs fidelity, and those are different channels with different
rules. The Action separately HTML-escapes repository-controlled targets before placing
them in its Markdown summary and applies GitHub workflow-command escaping to annotation
paths and messages.

Two delivery paths need different trust descriptions. The root
[Action dispatcher](https://github.com/HardMax71/amiss/blob/main/action.yml) makes conventional source release tags usable by delegating to the same version's immutable
[`action/vX.Y.Z` runtime](https://github.com/HardMax71/amiss/blob/main/crates/amiss/action/runtime.yml). That immutable second ref is part of a source-tag or source-commit pin; users that require one complete tree can pin the generated runtime tag or commit directly. The runtime is a GitHub event adapter. It verifies
the selected engine's digest against the release manifest carried in the same action tree,
then launches the engine directly. That detects an inconsistent tree, but the manifest is
not an independently acquired trust anchor, and this lane does not use bootstrap's
supervisor; it enforces its own wall-clock watchdog, 120 seconds unless the workflow
sets the `watchdog-seconds` input, and a scan that outlives the window ends with no
result.
The manifest's build-source host is supplied explicitly and its repository identity is
forge-neutral, as pinned by the
[release validation tests](https://github.com/HardMax71/amiss/blob/main/crates/amiss-bootstrap/tests/release.rs); that prevents a
format-level `github.com` assumption but does not authenticate the supplied identity.

The separately executable
[`amiss-bootstrap`](https://github.com/HardMax71/amiss/blob/main/crates/amiss-bootstrap/src/main.rs)
implements the stronger mechanism: validate an action tree and execution constraint as
data, launch the selected engine with a cleared environment and fixed arguments, and enforce
a 120-second wall-clock timeout. The request and control formats can bind an open forge
identity plus a provider/run namespace, but they cannot authenticate their own source.
Provider-authenticated request acquisition, adapters beyond the current GitHub event path,
and integration of the wrapper into the public required-check lane remain future work. The
JavaScript launcher is pinned manifest data and refuses if invoked directly; the current
composite Action does not invoke it.
