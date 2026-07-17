# Introduction

Amiss checks structural relationships between documentation and the repository tree it
describes. It discovers a closed set of Markdown and MDX documents, extracts supported
explicit references, and resolves their repository targets. When a supported reference
points at a file or line range that is gone, it reports that. When the selected target bytes
changed while the paragraph containing the reference did not, it reports that too. It never
reads meaning: it cannot tell you whether a sentence is true, and it does not try.

“Supported explicit reference” is an important boundary. Bare path-like prose is not
inferred, raw HTML and MDX code regions are opaque, and site routes, heading semantics, code
symbols, live URLs, and other repositories need information this engine does not have. Numeric
line fragments are the narrow exception: they select bytes, not language symbols or meaning. The
exact document classifier and resolver are linked from [Project status](status.md), while
[Discovery](discovery.md) and [Resolution](resolution.md) describe the visible boundary rows.

A run compares two exact states of the repository: the base (usually where your branch
started) and the candidate (usually the commit being reviewed). Amiss answers four questions
about them, and nothing else:

1. Does every supported explicit reference still point at something in the candidate tree?
2. Did the selected content or file mode of a referenced target change between base and candidate?
3. Did the paragraph holding the reference change too, stay exactly the same, disappear, or
   become impossible to match up without guessing?
4. What did the scan actually see: which documents it read, skipped, could not parse, or
   found unreachable?

The fourth question matters as much as the first three. A checker that silently skips what
it cannot handle is worse than no checker, because its green result claims more than it
checked. So everything Amiss cannot read or follow becomes a visible row in the report, and
a document it cannot decode at all fails the run rather than dropping out of it.

Amiss keeps no state. There is no baseline file, no cache, no database, and nothing committed
to your repository. Run it twice on the same commits with the same engine binary and you get
byte-identical reports. Repository policy may expand discovery and raise three structural
finding kinds, but it cannot downgrade a disposition or suppress a finding. How the project
arrived at that stance is told in [Provenance](provenance.md), and the exact policy boundary
is in [Controls and policy](controls.md).

Each promise below is enforced by a test in the suite:

- It never writes to your repository. The
  [no-write suite](../../crates/amiss/tests/no_write.rs)
  compares trees before and after commands and also scans a read-only repository.
- It never runs repository code and never calls the `git` command. It reads
  [Git](https://git-scm.com)'s objects, packs, and index through the
  [repository reader](../../crates/amiss-git/src/repo.rs).
- It never follows symlinks while reading. A link placed at the repository root, at `.git`,
  or anywhere along an object's path is refused, and the refusal is never confused with a
  missing file.
- It never touches the network. The engine has no acquisition or network interface; missing
  objects are refusals rather than fetch requests.
- The same repository, commits, and engine binary give the same report bytes, run after
  run.
- Stable public resource ceilings have names and published values. A measured crossing
  produces a typed error naming the limit and observed lower bound. Parser CPU work that
  occurs before node and depth accounting is a disclosed limitation in
  [Security model](security.md), not covered by a stronger “nothing can hang” promise.

The rest of this book walks those promises in the order a run does: what counts as input,
what gets scanned, how references resolve, what the report says, and where the boundaries
sit. Start with [Invocation](invocation.md) if you just want to run it.
