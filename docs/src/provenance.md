# Provenance

Amiss started life as a different tool. The original design was a review tracker with
memory: a committed ledger of what had been checked, paragraphs trusted when edited, an
`ok` command that recorded a human's acceptance, and a refresh job that kept the ledger
current. Point it at a repository and it would tell you which paragraphs had gone stale
against the code they cited, and remember your answer.

The pre-implementation review killed that design, and the reasons survived a second,
adversarial review. Identity rules that were safe for automatic observations were unsafe
for recorded human attestations sharing the same storage. A committed ledger file
conflicted between branches at rates that would have made merge queues the real product.
Trusting a paragraph because it was edited blessed pages that were already wrong. And the
deepest cut: the system's central promise, that a recorded acceptance meant a human had
checked the prose against the evidence, is not something any mechanism can make true. The
project's own experiments provided the counterexamples: of five replayed real-world cases,
three had their paragraph edited while the reference stayed broken. The numbers and
stories behind that verdict are in [The evidence base](evidence.md).

What survived is the part that never needed memory. A link either resolves in a tree or it
does not. Bytes either changed between two commits or they did not. A paragraph either
moved with the file it cites or it did not. Those are pure functions of two snapshots. They
need no ledger, no lock, no refresh, and no belief about anyone's intent, and every one of
the rejected designs had agreed on them. The v0 scanner is that surviving part, built
alone, under the review's discipline: fail closed, report every count, guess nothing.

The full investigation, market survey, adversarial reviews, experiment data, and the frozen
contracts this implementation was built against live in the repository's history as a
complete dossier. The machine-readable contracts, schemas and canonical examples, ship in
[`spec/`](https://github.com/HardMax71/amiss/tree/main/spec). Where this book and the
dossier disagree, the shipped code and its tests are the authority for what the tool does;
the dossier remains the authority for why.
