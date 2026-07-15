# Discovery

Discovery decides which files count as documents, and it is deliberately narrow. Markdown
and [MDX](https://mdxjs.com) files are documents, classified `structured-markdown` and `structured-mdx`. A small
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

Five paths through the classifier:

```text
docs/guide.md               structured-markdown   scanned
site/page.mdx               structured-mdx        scanned
llms.txt                    plain-advisory        scanned, nothing extracted
vendor/lib/README.md        excluded              the vendor component is in the closed set
src/parser.rs               not a document        a reference target only
```

Every count is reported: discovered, scanned, unsupported, excluded, unlinked. A document
that nothing links to and that links to nothing is also reported as an `unlinked-document`
finding, because a page nobody can reach is worth knowing about even though it blocks
nothing.

Paths are treated as bytes. Amiss does not fold case and does not normalize Unicode,
because Git addresses files by exact bytes, and a checker that guesses two names are
equivalent will eventually insist that two different files are the same file. A name whose
bytes are not valid UTF-8 is still a name: the entry is classified by the same suffix
rules, scanned, and reported, with its path written as a `bytes_hex` object naming the raw
bytes as lowercase hex, since JSON text cannot carry them directly. Only a name outside
the path grammar itself, one containing a backslash or a NUL byte, or a bare `.` or `..`
segment, is refused. That refusal is never quiet: the run stops as incomplete, the error
is recorded as `UNREPRESENTABLE_PATH` with the exact bytes in `path_bytes_hex`, and the
exit is 2. Dropping such an entry silently would be the worst bug this tool could have:
the report would come back green with a document missing from it, and a missing row is
the one defect no reader can notice.
