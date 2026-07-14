# Introduction

Amiss checks that your documentation and your code still agree. It reads the documents in a
repository, finds every link and path they mention, and follows each one into the same
repository. When a link points at a file that is gone, it reports that. When the file is
still there but its content changed while the paragraph describing it did not, it reports
that too. It never reads meaning: it cannot tell you whether a sentence is true, and it does
not try.

A run compares two exact states of the repository: the base (usually where your branch
started) and the candidate (usually the commit being reviewed). Amiss answers four questions
about them, and nothing else:

1. Does every link or path in a document still point at something in the candidate tree?
2. Did the content or file mode of a referenced file change between base and candidate?
3. Did the paragraph holding the reference change too, stay exactly the same, disappear, or
   become impossible to match up without guessing?
4. What did the scan actually see: which documents it read, skipped, could not parse, or
   found unreachable?

The fourth question matters as much as the first three. A checker that silently skips what
it cannot handle is worse than no checker, because its green result claims more than it
checked. So everything Amiss cannot read or follow becomes a visible row in the report, and
a document it cannot decode at all fails the run rather than dropping out of it.

Amiss keeps no state. There is no baseline file, no cache, no database, and nothing committed
to your repository. Run it twice on the same commits and you get byte-identical reports. It
also accepts no instructions from the repository it scans that would weaken the check, no
ignore comments, no severity overrides, because a check the checked code can switch off is
not a check. How the project arrived at that stance is told in [Provenance](provenance.md).

Each promise below is enforced by a test in the suite:

- It never writes to your repository. The tests snapshot the whole tree before and after
  every command and compare, and they also run it against a tree it has no permission to
  write.
- It never runs your code and never calls the `git` command. It reads Git's files directly:
  objects, packs, and the index.
- It never follows symlinks while reading. A link placed at the repository root, at `.git`,
  or anywhere along an object's path is refused, and the refusal is never confused with a
  missing file.
- It never touches the network. The engine's dependencies contain no networking library.
- The same repository, commits, and engine binary give the same report bytes, run after
  run.
- Every internal limit is a published number. Hitting one produces a typed error naming the
  limit and the observed value, never a hang or a silent cutoff.

For squeezing the last milliseconds out of large repositories, see the
[performance tuning guide](tuning.md).
