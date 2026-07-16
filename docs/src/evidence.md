# The evidence base

The design was not reasoned from an armchair. Before any code, one real repository was audited
end to end and used as the requirements generator. The project record calls it user zero: a
Scala compiler with machine-checked proofs, three code-generation targets, a published docs
site, 22 CI workflows, and roughly a dozen hand-built drift defenses already in place,
transcluded snippets, executable CLI examples, five golden-file suites, a proof-extraction
diff gate, a link checker. A repository that tries that hard and still drifts tells you what a
tool must actually do.

The removed working dossier remains available at the immutable pre-extraction commit: the
[repository audit](https://github.com/HardMax71/amiss/blob/26df8f76f84ee0e8bbee3f8c7a5ab49a44eaaadc/docs/repo-audit.md)
contains the observed cases, and the
[experiment index](https://github.com/HardMax71/amiss/blob/26df8f76f84ee0e8bbee3f8c7a5ab49a44eaaadc/docs/experiments/README.md)
points to the recorded measurements summarized here.

The audit found seven live drift classes despite all those defenses. A few, concretely:

- The architecture page said ten workflows; the tree had 22. It named a workflow file that had
  never existed. Three different module counts coexisted on one page.
- The CLI reference omitted a public subcommand and documented a three-value exit-code
  contract while the code used four. The executable snippets on the same page were all green,
  because examples protect only the paths they execute.
- A published OpenAPI file, claimed identical to compiler output, differed from the current
  golden by five stale lines.
- The railroad diagrams regenerated on every docs build, from a second copy of the grammar
  embedded in the generator script by hand. Regeneration succeeded forever while faithfully
  reproducing a stale input. Freshness of a generation step proves derivation from the step's
  input, not truth.
- CLI transcripts kept passing against a stale compiled binary and only failed when someone
  rebuilt it. A green check against the wrong environment is worse than no check.
- Hand-written counts ("ten workflows", "23 theories") were wrong in every place they
  appeared, because humans cannot maintain embedded aggregates.

Measurements set the tool's scale expectations. Conservative discovery found 109 documents.
Of 55 same-repository GitHub links, exactly two were broken, and the other measured explicit
references all resolved under their real semantics. Replaying history showed the surviving
reference graph would have produced 773 target-impact events across 393 first-parent commits,
which is reviewer workload, not 773 defects, and it is why a file changing under an unchanged
paragraph never blocks. And the experiment that shaped the architecture most: a single
committed state file, updated from branches, conflicted in 0%, 18%, and 99% of trials as
update counts per branch grew. That number is a large part of why the shipped scanner keeps
no state at all.

Two observed conditions draw the sharpest boundaries. A page that was edited the day before
the audit was already wrong when edited, so any scheme that trusts an edit blesses false
prose. And every mechanism that let a person clear findings in bulk was, in the audit's
words, the gate's cheapest bypass. Both conditions killed the ledger design described in
[Provenance](provenance.md), and both explain why the scanner only ever reports what two
trees say.
