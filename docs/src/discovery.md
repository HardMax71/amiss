# Discovery

Discovery decides which files count as documents, and it is deliberately narrow. Markdown
and MDX files are documents, classified `structured-markdown` and `structured-mdx`. A small
fixed list of well-known extensionless names, `llms.txt` among them, is classified
`plain-advisory` and scanned by a profile that extracts nothing but honestly reports that
the file exists and was not parsed. Every other file is a possible link target, not a
document.

Seven directory names are always skipped, wherever they appear in a path:

```text
node_modules  vendor  third_party  dist  build  .next  target
```

The list is fixed and cannot be narrowed by configuration. Skipping is visible in both
directions: skipped documents still show up in the report's counts, as excluded. This
repository relies on the rule itself: its vendored parser test corpus lives under
`corpus/third_party/` exactly so that fixture files full of deliberately broken links are
never read as prose.

Every count is reported: discovered, scanned, unsupported, excluded, unlinked. A document
that nothing links to and that links to nothing is also reported as an `unlinked-document`
finding, because a page nobody can reach is worth knowing about even though it blocks
nothing.

Paths are treated as bytes. Amiss does not fold case and does not normalize Unicode,
because Git addresses files by exact bytes, and a checker that guesses two names are
equivalent will eventually insist that two different files are the same file. A path the
report format cannot even write down, bytes that are not valid UTF-8, or a name containing
a backslash, is not quietly dropped. The run stops as incomplete, the error is recorded as
`UNREPRESENTABLE_PATH`, and the exit is 2. Dropping such an entry silently would be the
worst bug this tool could have: the report would come back green with a document missing
from it, and a missing row is the one defect no reader can notice.
