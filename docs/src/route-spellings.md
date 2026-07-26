# What a documentation router serves

`[Plain Text](./plain-text)` is dead in the tree and alive on the site. The file is
`plain-text.md`; the router elides the extension, and starship's own preset page links it
both ways in one paragraph, once with the extension and once without. A checker that reads
only the tree calls the second one broken. It is not broken, and 247 of the 516 missing
references in [the scan ledger's rescan](ledger.md) are that same shape: a target the tree
holds under a spelling the router maps.

So the resolver asks the same question the router does. A destination the tree holds is its
own answer. A destination the tree does not hold is looked up again under the spellings a
modelled router serves, and the first one that names a file resolves the reference to that
file. [Resolution](resolution.md) places this in the order; this page holds the spellings
and where they came from.

## The three spellings

| Spelling | A destination like | Reaches | Served by |
| --- | --- | --- | --- |
| `extensionless` | `guide` | `guide.md` | vitepress |
| `output-extension` | `guide.html` | `guide.md` | mdbook, vitepress |
| `readme-index` | `dir/index.md`, `dir/index.html` | `dir/README.md` | mdbook, vitepress when configured for it |

A spelling only ever names a file that is already in the tree, so it can widen what
resolves and can never invent a target. Everything a spelling does not reach stays exactly
as missing as it was, under the destination the author wrote.

Two spellings are never tried. A destination ending in `/` promised a directory, and the
tree answers a directory itself. A same-repository forge URL is read by the forge, which
serves the tree rather than a site, so `blob/main/docs/guide` stays missing even though
`docs/guide.md` exists.

## Where the spellings came from

The published expectations are in
[route-spelling vectors](https://github.com/HardMax71/amiss/blob/main/spec/examples/route-spelling-vectors.json),
harvested 2026-07-26. One probe tree of five pages, one destination per probe page so every
verdict names its own case, and each router asked in its own voice rather than read from its
documentation:

| Router | Version and configuration | How it answered |
| --- | --- | --- |
| `mdbook` | 0.5.4, default | the href it emitted, resolved against the built output tree |
| `vitepress` | 1.6.4, `cleanUrls` | its own dead-link report, corroborated by the output tree |
| `vitepress-readme` | the same, plus the README-to-index rewrites starship configures | the same |
| `mkdocs` | 1.6.1, default `use_directory_urls` | its unrecognized-link warnings |

mkdocs serves none of the three. It demands the source path and warns otherwise, which is
why a repository it publishes gains nothing here and loses nothing: ruff's 102 missing
references did not move by one.

The vectors also keep a verdict this table does not model. mdbook rewrites a link to
`dir/README.md` into `dir/README.html` while writing that page to `dir/index.html`, so its
own source spelling is the single form it fails to serve. The tree holds that file, and a
file the tree holds resolves without any rule being asked.

## What this costs

A repository with no site at all now resolves `./guide` when `guide.md` exists, and on
github.com that link is a 404. This is the same trade
[the renderer rules](anchor-rules.md) already make for heading identities, taken for the
same reason: a false missing target teaches maintainers to ignore the tool, and the union
of what real renderers do is the honest way to avoid one. Nothing a repository declares
selects a router, because a configuration file in the tree would be a lever the pull
request under review could pull.

Routers outside the table serve spellings this check will not match. Four repositories built
on them were run on 2026-07-26 to find out what a new row would have to answer, each read
whole against an empty base under the observe profile. Three completed, at hugoDocs
`620696ab3b07`, jest `f49721c78e19`, and jekyll `7697d249793d`, and their counts below are
from those reports. The fourth, docusaurus `16f537309e35`, produced no report: it ran to the
end of evaluation and then refused at output, so nothing is counted from it. For the three
that completed, a row is not the answer.

Hugo's own documentation writes `[glob pattern](g)` and resolves `g` in its own
`render-link.html`, 734 references to a path that exists nowhere. Its 101 missing anchors are
two further mechanisms: 71 name a definition-list term, which its configuration turns into an
identity with `autoDefinitionTermID`, so `module.md`'s `files` term is published as
`<dt id=files>` while the pinned grammar has no definition list to read at all; 28 name a
heading pulled in by an `{{% include %}}` shortcode. Jest, on Docusaurus, links a document by
the identity that document declares in its own front matter: `Configuration.md` opens with
`id: configuration`, its page is published at that name, and 104 references reach it by URL
rather than by path. The identity is in the tree, but reading it means parsing front matter
this engine keeps opaque and then indexing every document by what it declares.

Docusaurus itself refused at first, its findings serializing past the output reservation
described in [Limits and refusals](limits.md). With that raised it scans, and with the
identity its headings declare in an MDX comment now read it reports 198 missing references
rather than 807. Those are the `@site` alias, a webpack path with no tree meaning, and
identities that arrive through MDX imports of partial files.

Jekyll is the one that looks like a missing row, and the harvest says otherwise. Its own site
writes `reviewing-a-pull-request/` from a maintaining index, which reaches
`maintaining/reviewing-a-pull-request.md`, and `../ubuntu/` from an installation page, which
reaches nothing; jekyllrb.com serves the first and returns 404 for the second, so both of our
answers match the site. A trailing slash reaching the sibling source file is real there because
that site's permalinks mirror its paths, which is a configuration and not a property of Jekyll.
Asked the same destination, mdbook serves nothing, mkdocs rejects it in the source with a
warning naming the `.md` file, and vitepress emits it verbatim into a build holding only
`page.html`, dead on any host despite its own dead-link checker accepting it. One router, by
configuration, is not a rule.

What Hugo and Jest need instead is the generated class the [roadmap](roadmap.md) still
carries, arriving there as transclusion, as a repository's own render hook, and as an
identifier that was never a path.
