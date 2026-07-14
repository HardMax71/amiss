# Resolution

Parsing turns each document into a list of occurrences: inline links and images, reference
style links, and autolinks. Each occurrence keeps two spellings of its destination. The raw
one is the exact bytes from the source. The semantic one is what those bytes mean after the
format's own decoding. So `[a](&amp;b)` records both `&amp;b` and `&b`, and a change to
either the spelling or the meaning is visible later.

What the parser cannot see into is declared instead of skipped. Raw HTML blocks and MDX
expressions become opaque regions, reported with their size and place as
`opaque-html-region` and `opaque-mdx-region` findings, so a link hidden inside JSX is a
stated blind spot rather than an invisible one.

Each destination then resolves against the tree, and only three shapes are in scope. A
relative path resolves from the document's own directory and must stay inside the
repository; `../../etc/passwd` is an `invalid-reference`, not a file read. A
repository-rooted path resolves from the root. And when the invocation provides the
`--repository` triple, a GitHub blob or tree URL that names this same repository and a
branch the scan can vouch for is converted to the path it points at. Every other URL is
`external-out-of-scope`: counted, reported, left alone.

Resolution is exact, and the small rules matter. A trailing slash means the author promised
a directory, so `sub/` must be a tree and `guide.md/` is a type mismatch even though
`guide.md` exists. Percent-encoding is decoded exactly once, so `%252F` stays as the
literal three characters `%2F` in the name instead of turning into a second slash. Query
strings and fragments are recorded as digests but ignored for resolution, because a tree
has no queries and no anchors. Heading anchors, site routes, code symbols, and
version-pinned references are all reported as `unsupported-reference-semantics`: real
checks for those belong to tools that have the right information, and a guess here would
turn honest ignorance into a false pass.

Each resolved target is read from the object store and hashed, so the comparison knows the
exact bytes and file mode on both sides. A symlink or submodule target is
`unsupported-target-kind`, because following one leaves the world of exact bytes where the
guarantees live. A [Git LFS](https://git-lfs.com) pointer file is recognized and its
declared content digest is carried, so swapping the large file behind a pointer counts as
a change even though the pointer text barely moves.
