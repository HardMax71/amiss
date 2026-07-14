# Security model

The repository under evaluation is the attacker. Everything in it, documents, paths, objects,
packfiles, the index, the policy file, arrived from whoever authored the pull request, and
the scanner's job is to be a safe pure function of those hostile bytes.

The engine therefore never executes anything. There is no plugin interface, no configured
command, no formatter invocation, no `git` subprocess. A policy that names a command or a
plugin is not a feature request the scanner declines politely: it is an unknown field, the
configuration is invalid, the run is incomplete, and there is no report to mistake for a
pass. A pre-commit hook in this repository bans `process::Command` from the engine crates
outright, so the property is enforced at authorship time, not just claimed.

The engine never touches the network, and its dependency closure contains no network crate.
It never writes to the repository, which the suite proves by diffing a full tree snapshot
around every command and by running the scanner against a tree it has no permission to write.
Hostile inputs that try to make it read elsewhere hit the no-follow handle chain described in
[Snapshots](snapshots.md).

Parsers are the largest attack surface, and they are treated as such. Every parser that
consumes untrusted bytes has a fuzz target, the corpus of a pinned upstream grammar suite,
and resource ceilings charged before and during the parse. A parser panic is caught,
classified `PARSER_PANIC`, attributed to the document that caused it, and the run continues;
a hostile document cannot take the scanner down with the evaluation. The two known
panic-inducing documents for the pinned markdown grammar are in the conformance corpus, and
the contract's answer to them is exactly that classification.

Output is part of the surface. Repository paths appear in terminal output and CI logs, so
the human projection escapes every byte outside printable ASCII; an ESC sequence, a carriage
return, or a forged `::error::` workflow command embedded in a filename reaches the log only
as an inert `\uXXXX` escape. The JSON report carries the exact bytes as a JSON string, losing
nothing, because forensic fidelity and log safety are different channels with different laws.

The CI trust chain runs the same direction. The action tree that will eventually ship is
validated as data by a separately protected wrapper, `amiss-bootstrap`, which launches a
verified engine with a cleared environment and fixed arguments under a wall-clock watchdog.
The tree's own JavaScript launcher never validates anything: letting code the action tree
supplies decide whether the action tree is valid would run the attacker's code to check the
attacker, so the launcher's only behavior is to refuse with exit 2 when invoked directly.
