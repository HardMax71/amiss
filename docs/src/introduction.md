# Introduction

Amiss checks documentation against the tree it describes. It reads the documents in a
repository, follows the references they make into that same repository, and reports when a
reference stops resolving, or when the file behind it changed while the prose around it did
not. It reads structure, not meaning: it will not tell you whether a sentence is true, and it
does not guess.

Four questions, and nothing else:

1. Does a same-repository reference in a document resolve in the exact tree being evaluated?
2. Did the bytes or the Git mode of a referenced file change between the base and the candidate?
3. Did the source block holding that reference change with it, stay byte-identical, disappear,
   or become impossible to correlate without guessing?
4. What document and reference surface was discovered, excluded, opaque, unsupported, or
   unlinked?

The fourth carries as much weight as the first three. A checker that quietly skips what it
cannot parse is worse than no checker at all, because it reports a success it never earned.
Every document Amiss cannot read, every reference it cannot follow, and every region it cannot
see into is a row in the report, and a document it cannot decode fails the run instead of
vanishing from it.

There is no baseline, no state directory, no ledger, and no lockfile. Amiss remembers nothing
between runs, so there is nothing to migrate and nothing to drift. It accepts no claims,
waivers, or annotations from the repository it is scanning, because a check whose subject can
switch it off is not a check. Where that stance came from, and what it replaced, is the story
in [Provenance](provenance.md).

Each guarantee below is a test in the suite, not a promise:

- It never writes to the repository. The suite snapshots the whole tree before and after every
  command and compares, and it runs the scanner against a tree it has no permission to write.
- It never runs anything from the repository, and it never shells out to `git`. It reads
  objects, packs, and the index itself.
- Every read goes through a directory handle that is never followed. A symlink, junction, or
  reparse point at the root, at `.git`, at `objects`, or anywhere in an object's path is
  refused, not followed, and never mistaken for an absent object.
- It never touches the network, and the engine's dependency closure contains no network crate.
- The same repository and the same commits produce the same bytes, on every platform.
- Every limit is a number in the contract. Crossing one is a typed error carrying both the
  limit and what was observed, never a hang, a crash, or a silent truncation.
