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
references did not move by one. What it does move is the directory a destination is read
from, and that is the `directory-url` rule below.

The vectors also keep a verdict this table does not model. mdbook rewrites a link to
`dir/README.md` into `dir/README.html` while writing that page to `dir/index.html`, so its
own source spelling is the single form it fails to serve. The tree holds that file, and a
file the tree holds resolves without any rule being asked.

## What a generator in the tree anchors

The three spellings hold in every tree, since `guide` reaching `guide.md` costs one 404 on a
site with no router and nothing more. A second set holds only where the tree carries the
generator's own configuration file, because each of these moves a destination to another
directory and would be a guess anywhere else. A rule turns on when one of its files is a blob
on the document's ancestor chain, nearest directory first. Only the file's presence is read,
never its contents.

<!-- amiss-doc-contract:declared-routers:start -->
| Router | Selected by | Serves |
| --- | --- | --- |
| `antora` | `antora.yml` | `antora-resource` |
| `docusaurus` | `docusaurus.config.ts`, `docusaurus.config.mts`, `docusaurus.config.cts`, `docusaurus.config.js`, `docusaurus.config.mjs`, `docusaurus.config.cjs` | `site-alias`, `content-root` |
| `mkdocs` | `mkdocs.yml`, `mkdocs.yaml` | `directory-url` |
| `sphinx` | `conf.py` | `source-root` |
<!-- amiss-doc-contract:declared-routers:end -->

`antora-resource` reads an AsciiDoc destination as an Antora resource ID,
`[module:][family$]relative`, in a document under `modules/<name>/` of the component whose
root holds `antora.yml`. The relative part is anchored at the family directory of the named
module, or of the document's own module when none is named. So `xref:index.adoc[]` in
`modules/api/nav.adoc` is `modules/api/pages/index.adoc`, `include::partial$success.adoc[]`
in a page of that module is `modules/api/partials/success.adoc`, `xref:cli:index.adoc[]` is
`modules/cli/pages/index.adoc`, and `image::diagram.png[]` is `modules/api/images/diagram.png`.
The families are `page`, `partial`, `example`, `attachment` and `image`. An xref defaults to
the page family and an image to the image family. An include without a family coordinate
stays relative to the file that includes it, which is Antora's own compatibility rule for the
plain include. This rule replaces the relative reading rather than following it: a page under
`pages/sub/` that writes `xref:index.adoc[]` means the family root, and Antora never looks
beside the file. A version coordinate (`2.0@`) or a component coordinate
(`component:module:page.adoc`) names a catalogue this tree does not hold, so such a
destination keeps the reading it had before, and a `./` or `../` relative is Antora's own
page-relative form and stays beside the document.

`site-alias` and `content-root` are the order Docusaurus's `resolveMarkdownLink` tries
directories in, read from `packages/docusaurus-utils/src/markdownLinks.ts`, for a document
under the directory holding `docusaurus.config.ts` or its `.mts`, `.cts`, `.js`, `.mjs` and
`.cjs` spellings, the list the site loader tries. `@site/blog/img/output.png` is
`blog/img/output.png` under that directory, for a link and for an image. A bare `.md` or
`.mdx` destination, one starting with neither `./`, `../` nor `/`, is tried beside the
document first, then under the plugin content path the document sits in, then under the
site directory. `[static folder](static-assets.mdx)` in `docs/api/themes/configuration.mdx`
reaches `docs/static-assets.mdx` that way. The content paths read are the plugin defaults,
`docs`, `blog`, `src/pages` and `versioned_docs/<version>`. A plugin configured to read
another directory gets the site-directory step alone, and a localized tree under `i18n/` is
not read. A `./` or `../` destination is beside the document and nowhere else, which is
Docusaurus's rule too.

`directory-url` reads a raw HTML destination in a document under a `mkdocs.yml` the way the
browser does. mkdocs rewrites the destination of a Markdown link and leaves an `<a href>` or
an `<img src>` written by hand alone, so that one is resolved against the URL the page is
served at rather than against the source file. A page is published at a directory of its own
name, or at its own directory when the source is that directory's `index.md` or `README.md`.
So `<a href="getting-started/">` in `docs/index.md` reaches `docs/getting-started.md`, and
`<img src="../../img/light.png">` in `docs/user-guide/choosing-your-theme.md`, published at
`user-guide/choosing-your-theme/`, reaches `docs/img/light.png`. A trailing slash names a page
here rather than a tree, since every page URL ends in one, so the destination is asked without
it and the three spellings answer. The document's own directory is tried after the published
one, so a raw destination that already reached a file beside the source still reaches it.

`source-root` is Sphinx's `:doc:` role with a leading slash, in a document under the
directory holding `conf.py`. `` :doc:`/testing` `` in `docs/tutorial/deploy.rst` is
`docs/testing.rst`: the docname under the source directory, with the `.rst` suffix an
extensionless name takes, the same suffix the relative form already took. A plain hyperlink
with a leading slash is still a site route, since Sphinx emits it as written, and a `:doc:`
target in a tree with no `conf.py` above the document stays the declared site route it was.

Each of these widens what resolves and nothing else, like the three spellings: an anchored
destination is looked up in the tree and is missing when the tree does not hold it, so
`xref:load-templates.adoc[]` in `modules/api/pages/index.adoc` is still missing while
`modules/api/pages/load-templates.adoc` is not there. One thing does move: the intent. A
routed `guide` keeps `guide` as the path the author meant, while an Antora xref's intent is
the family path and a Sphinx `:doc:` target's is the docname under `conf.py`, because the
author never meant a sibling file. A Docusaurus bare path keeps the sibling as its intent,
because Docusaurus does try the sibling first.

One opening is not a path under any rule. A bundler's inline request syntax reserves it for
the loaders the request disables, so `[assets](!file-loader!./asset.pdf)` names a loader chain
ending in a resource rather than a file. That is an `invalid-reference` and not a missing path
with a loader name inside it. Only the opening is read, so a name carrying the character
anywhere else is an ordinary path.

<!-- amiss-doc-contract:bundler-requests:start -->
| Bundler | Inline request opens with |
| --- | --- |
| `webpack` | `!`, `-!` |
<!-- amiss-doc-contract:bundler-requests:end -->

The two forms webpack documents for its own loaders are both here: `!` disables the
configured normal loaders and `-!` the pre-loaders, and the `!!` that disables every loader
opens with the first of them.

## What this costs

A repository with no site at all now resolves `./guide` when `guide.md` exists, and on
github.com that link is a 404. This is the same trade
[the renderer rules](anchor-rules.md) already make for heading identities, taken for the
same reason: a false missing target teaches maintainers to ignore the tool, and the union
of what real renderers do is the honest way to avoid one. The three spellings are selected
by nothing a repository declares. A generator rule is selected by a configuration file, which
is a lever the pull request under review can pull, and the lever is bounded the same way the
spellings are: it can move a destination onto a file the tree already holds, and it cannot
clear a destination the tree lacks.

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

What Hugo and Jest need instead is the generated class, arriving there as transclusion,
as a repository's own render hook, and as an identifier that was never a path. The exact-path
core of that class is answered now, from the tracked ignore file recorded in
[Reference coverage](completed/reference-coverage.md), and all three of those arrivals sit
outside it.
