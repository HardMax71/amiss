# Security model

The repository being scanned is treated as the attacker. Its documents, paths, Git objects,
packfiles, index, and policy file all came from whoever wrote the pull request, and the
scanner's whole job is to be a safe, pure function of those hostile bytes.

The engine executes nothing. No plugin system, no configurable commands, no formatter
calls, no `git` subprocess. A policy file that names a command or a plugin is not a feature
request to decline politely: the field is unknown, the configuration is invalid, the run
ends incomplete, and there is no report to mistake for a pass. A pre-commit hook in this
repository bans `process::Command` from the engine's code outright, so the property is
enforced when code is written, not just claimed afterward.

The engine never touches the network, and its dependency tree contains no networking
library. It never writes to the repository, which the tests prove two ways: by comparing a
full snapshot of the tree before and after every command, and by running the scanner
against a tree it has no permission to write. Attempts to make it read outside the
repository run into the never-follow-links rule described in [Snapshots](snapshots.md).

Parsers are the biggest attack surface and are treated accordingly. Every parser that
consumes untrusted bytes has a fuzz target, a pinned upstream test corpus, and resource
ceilings charged before and during parsing. A parser panic is caught, recorded as
`PARSER_PANIC` against the document that caused it, and the run continues; a hostile
document cannot take the scanner down. The two documents known to panic the pinned
Markdown parser sit in the conformance corpus, and that classification is the contract's
answer to them.

Output is part of the surface too. Repository paths end up in terminals and CI logs, so
the human format escapes every byte outside printable ASCII. An ANSI escape sequence, a
carriage return, or a forged `::error::` workflow command embedded in a filename reaches
the log only as harmless `\uXXXX` text. The JSON report keeps the exact original bytes as
a JSON string, because the log needs safety and the report needs fidelity, and those are
different channels with different rules.

The CI trust chain points the same direction. The GitHub Action tree that will eventually
ship is validated as plain data by a separately installed wrapper, `amiss-bootstrap`,
which then launches a verified engine with a cleared environment, fixed arguments, and a
wall-clock timeout. The JavaScript launcher inside the action tree validates nothing:
letting code that came with the tree decide whether the tree is valid would mean running
the attacker's code to check the attacker. Its only behavior is to refuse, with exit 2,
if invoked directly.
