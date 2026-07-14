# Provenance

Amiss began as a different tool. The original design was a review-impact graph with memory: a
committed ledger of baselines, blocks trusted on edit, an `ok` command that recorded human
acceptance, and a refresh lane that kept the ledger current. Point it at a repository and it
would tell you which paragraphs were stale against the code they cited, and remember your
answer.

The pre-implementation review killed it, for reasons that held up under adversarial re-review.
Identity and refresh semantics that were safe for automatic observations were unsafe for
governed attestations sharing the same store. A committed ledger conflicted at rates that
made the merge queue the real product. Trust-on-edit blessed pages that were already false.
And the deepest cut: the system's central claim, that a recorded acceptance meant a human had
verified prose against evidence, was not something the mechanism could make true. The
investigation's own experiments supplied the counterexamples, three of five replayed cases
had their containing block edited while remaining broken.

What survived is the part that never needed memory. A reference either resolves in a tree or
it does not. Bytes either changed between two commits or they did not. A block either moved
with its dependency or it did not. Those are pure functions of two snapshots, they need no
ledger, no lock, no refresh, and no belief about what a human meant, and they were the
component every rejected design agreed on. The v0 scanner is that component, built alone,
with the refusal discipline the review demanded: fail closed, report every denominator, guess
nothing.

The full investigation, market survey, adversarial reviews, experiment data, and the frozen
contracts the implementation was built against live in the repository history as a complete
dossier, and the machine-readable contracts (schemas and canonical vectors) ship in
[`spec/`](https://github.com/HardMax71/amiss/tree/main/spec). Where this book and the
dossier disagree, the shipped code and its tests are the authority for what the tool does,
and the dossier remains the authority for why.
