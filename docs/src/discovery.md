# Discovery

Discovery decides which files in the snapshot are documents, and it is deliberately
conservative. Markdown and MDX files are documents, classified `structured-markdown` and
`structured-mdx`. A small closed set of well-known extensionless names, `llms.txt` among
them, is classified `plain-advisory` and scanned with a zero-lexer profile that extracts
nothing but reports the document's existence and opacity honestly. Everything else is a
potential reference target, not a document.

Seven tree names are always excluded, matched on any path component:

```text
node_modules  vendor  third_party  dist  build  .next  target
```

The set is closed and not configurable downward, and the scanner prints it under
`--explain-scope`. Exclusion is honest in both directions: excluded documents appear in the
report's denominators as excluded, and this repository keeps its own vendored parser corpus
under `corpus/third_party/` precisely so that fixtures full of deliberately broken links are
never scanned as prose.

Every denominator is reported. Discovered, scanned, unsupported, excluded, unlinked: the
counts are in the report summary, and each unlinked document (one that nothing references and
that references nothing) is also a `record`-level finding, because a page nobody can reach is
a fact worth knowing even when it blocks nothing.

Paths are bytes. The resolver neither folds case nor normalizes Unicode, because the tree is
byte-addressed and a checker that guesses at equivalences will eventually claim two different
files are the same file. A path the scanner cannot even write down (bytes that are not UTF-8,
or a name `RepoPath` refuses, such as one carrying a backslash) is not quietly dropped: the
run is incomplete, the defect is a retained analysis error naming `UNREPRESENTABLE_PATH`, and
the exit is 2. Dropping that entry silently would be the worst bug this tool could have,
because the report would come back complete and passing with a document simply absent from
it, and the absence is the one thing nobody can see.
