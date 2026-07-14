# Resolution

Extraction reads each scanned document with a parser pinned to the CommonMark and GFM
conformance corpora, plus the MDX grammar for `.mdx`. What comes out is occurrences: inline
links and images, reference-style forms, and autolinks, each carrying two destination
representations. The raw destination is the exact source-token byte slice; the semantic
destination is the value after the construct's own decoding. `[a](&amp;b)` records both
`&amp;b` and `&b`, pinning the spelling and the meaning separately.

Regions the parser cannot see into are declared, not skipped. Raw HTML blocks and MDX
expressions become opaque intervals, reported as `opaque-html-region` and
`opaque-mdx-region` findings, so a link hidden inside JSX is a stated blind spot rather than
an invisible one.

A destination then resolves against the snapshot, and only three families of destination are
in scope. A relative path resolves against the document's own directory, contained within the
repository root: an escape like `../../etc/passwd` is an invalid reference, not a filesystem
probe. A repository-rooted path resolves from the root. And when the invocation supplies the
`--repository` triple, a GitHub blob or tree URL naming that same repository and a branch the
scan can vouch for is translated into the path it names; every other URL is
`external-out-of-scope`, counted and left alone.

Resolution is exact. A trailing slash is an authored directory hint, so `sub/` must be a
tree, and `guide.md/` is a type mismatch even though `guide.md` exists. Percent-encoding
decodes exactly once, and `%252F` therefore stays a literal `%2F` in the name rather than
becoming a second slash. A query string or fragment is carried as a digest in the report but
plays no part in resolution, because the tree has no queries and no anchors. Anchors, heading
slugs, site routes, code symbols, and version-scoped references are all explicit
`unsupported-reference-semantics` boundaries: real checkers for those exist at other layers,
and a guess here would convert honest ignorance into false confidence.

Each resolved target is read from the object store and hashed, so the evaluation knows the
exact bytes and Git mode on both sides of the comparison. A symlink or gitlink target is
`unsupported-target-kind`: following it would leave the byte-addressed world the guarantees
live in. An [LFS](https://git-lfs.com) pointer file is recognized and its declared object
digest carried, so a pointer swap is a change even though the pointer text barely moves.
