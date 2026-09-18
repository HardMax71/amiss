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
| `docusaurus` | `docusaurus.config.ts`, `docusaurus.config.mts`, `docusaurus.config.cts`, `docusaurus.config.js`, `docusaurus.config.mjs`, `docusaurus.config.cjs` | `site-alias`, `content-root`, `document-id` |
| `mkdocs` | `mkdocs.yml`, `mkdocs.yaml` | `directory-url` |
| `sphinx` | `conf.py` | `source-root` |
| `mdbook-pages` | `book.toml` | `book-route`, `built-page` |
| `zola` | `config.toml` | `content-root` |
| `astro` | `astro.config.ts`, `astro.config.mts`, `astro.config.js`, `astro.config.mjs`, `astro.config.cjs` | `built-route` |
| `eleventy` | `eleventy.config.ts`, `eleventy.config.js`, `eleventy.config.mjs`, `eleventy.config.cjs`, `.eleventy.js` | `built-route` |
| `hugo` | `hugo.toml`, `hugo.yaml` | `built-route` |
| `jekyll` | `_config.yml` | `built-route` |
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

Where no `docusaurus.config.*` sits above the document, `@site/` is a destination this run
cannot answer rather than a missing directory called `@site`: the alias is expanded when the
site is built, and nothing here says which directory it names. That is
`unsupported-reference-semantics`, the answer an AsciiDoc `{attribute}` already gets for the
same reason. Only the opening is read, so a tree with a real `@internal/` directory resolves
that path as written.

`document-id` is the identity a document declares for itself. Docusaurus publishes a page at
the `id` in its frontmatter rather than at its file name, so jest's `docs/CLI.md` opens with
`id: cli`, the site serves it at `cli`, and every link to it writes `cli`, which names no file
in the tree. A `slug` overrides the id, read from the document's own directory or, with a
leading slash, from the content root it sits in, and a document declaring neither is published
at its own path without the source extension, which is how a bare `notes` reaches `notes.mdx`.
The routes are collected once per snapshot, from the documents it already holds, and a
destination the tree does not answer is looked up in that collection after the three
spellings, so a real file always wins. A route two documents claim is left out rather than
decided between them.

This is the one rule the document's own ancestor chain does not select. A site names its
content directories in the configuration file, which this engine never opens, and jest's site
sits in `website/` while the documents it publishes sit in `../docs`, so selecting on the
chain would miss every one of them. Any `docusaurus.config.*` in the tree turns the collection
on, and a tree with none has no collection at all. Inside a document the rule reads two
frontmatter keys written on lines of their own; the region stays opaque to the grammar, and a
value that is not a plain scalar is declined rather than guessed at. A page that declares a
site-absolute slug also moves its own URL, and a destination relative to that URL is not read.

A document under one of those content paths is not always a page. Docusaurus excludes every
name opening with `_` from routing, a directory as well as a file, so
`docs/api/plugins/_partial-tags-file-api-ref-section.mdx` in its own repository is served at
no URL at all and is rendered into the two pages that import it, `plugin-content-blog.mdx`
and `plugin-content-docs.mdx`.

<!-- amiss-doc-contract:unrouted-documents:start -->
| Router | Publishes no page for | Under |
| --- | --- | --- |
| `docusaurus` | a name opening with `_` | `docs`, `blog`, `src/pages`, `versioned_docs/*` |
<!-- amiss-doc-contract:unrouted-documents:end -->

A fragment written inside such a document names an identity of the page that renders it, and
one partial may be rendered into several, so the tree does not say which page that is. The
identities the file writes itself still answer, and a fragment nothing in it publishes is
undecided rather than absent, the same answer [Resolution](resolution.md) gives any target
whose identity set is incomplete. That is the whole of it: a path in the same document is
read from the file as written, since that is where Docusaurus reads one from wherever the
page ends up, and a path the tree lacks is still missing. The three links in that one partial
are 24 of the 70 missing targets a docusaurus clone reported, counted once per versioned
copy, and none of them is broken: that site sets `onBrokenAnchors: 'throw'` for the default
locale, so an anchor that really was absent would have failed its own build.

The name decides this rather than the import. An import says one document renders another,
which a page published at its own URL may also be, and the walk that finds one is bounded by
`references-per-document` and `parser-nesting`, so an edge nobody walked is not evidence that
a file is a page. The exclusion is Docusaurus's own, read from the name alone, and a tree that
declares no site keeps every anchor claim it had.

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

Each of the spellings above widens what resolves and nothing else, like the three: an anchored
destination is looked up in the tree and is missing when the tree does not hold it, so
`xref:load-templates.adoc[]` in `modules/api/pages/index.adoc` is still missing while
`modules/api/pages/load-templates.adoc` is not there. One thing does move: the intent. A
routed `guide` keeps `guide` as the path the author meant, while an Antora xref's intent is
the family path and a Sphinx `:doc:` target's is the docname under `conf.py`, because the
author never meant a sibling file.

Where a rule keeps the reading from the document's own directory, that reading is the intent
and the finding names it. A Docusaurus bare path keeps the sibling, since Docusaurus tries the
sibling first, and a raw `<img src="../../img/light.png">` under mkdocs keeps `img/light.png`,
the path a forge asks for, while the published directory still answers it with
`docs/img/light.png`. The published directory is a URL rather than a place in the tree, so
naming it in a finding puts a directory nobody wrote into the report: a destination under
`docs/en/docs/fastapi-people.md` that reached nothing used to be reported as
`docs/en/docs/fastapi-people/{{ sponsor.url }}`, and neither `fastapi-people/` nor the rest of
it is in that file.

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

An expression the build fills in is not a path either. `<a href="{{ sponsor.url }}">` in
fastapi's `docs/en/docs/fastapi-people.md` names a sponsor's site once the page is generated
and names nothing at all in the tree, so it is `unsupported-reference-semantics` with the
attribute-dependent reason rather than a missing file called `{{ sponsor.url }}`. That is 39
of the 79 missing targets a fastapi clone reported.

<!-- amiss-doc-contract:template-expressions:start -->
| Expression opens with | and closes with |
| --- | --- |
| `{{` | `}}` |
| `{%` | `%}` |
<!-- amiss-doc-contract:template-expressions:end -->

Both delimiters have to be there, in that order, so a file whose name carries one brace is an
ordinary path and resolves as written. Jinja, Liquid, Nunjucks and Handlebars all spell an
expression this way, which is why the rows are the delimiters rather than the name of one
generator. A tree that holds a real file whose name spells an expression, as a cookiecutter
template does, gets this answer for it instead of the file.

## What the build answers instead of the tree

A generator that does not put its pages where its sources are breaks the assumption behind
every rule above, that a relative destination names a place in the tree. Hugo, Jekyll,
Eleventy and Astro each resolve one against the URL the page is served at, and that URL comes
out of a configuration this engine does not open. Twelve ordinary projects put a number on it:
of 1,596 missing targets, the 1,149 from repositories built on a generator with no rule here
held 41 real breaks. So `built-route` answers such a destination with
`unsupported-reference-semantics` and a reason saying the route model is not modelled, the way
an absolute site route already does, instead of naming a file nobody wrote.

The selection is the same as every rule above: the file has to sit on the document's ancestor
chain, and only the path side moves. An anchor into a document the tree holds is still read,
because the identities a document publishes are enumerable whatever a site does with its URLs,
which is why helix keeps all nine of its anchor findings and its one missing path. A tree
carrying none of these files keeps every claim it had, which is how the Kubernetes community
repository keeps all 254 of its missing targets.

mdBook is modelled rather than declared, because its URLs are in the tree. A book's root is the
directory holding `book.toml`, its pages are the Markdown under `src`, and each page is served
one directory shallower than its source, so `second/src/ch01.md` is `second/ch01.html`. A
destination climbing past that root is therefore read back under the `src` of whichever book
holds the page it names, and that is `book-route`: the rust book's
`second-edition/src/ch09-02-recoverable-errors-with-result.md` writes
`../ch09-02-recoverable-errors-with-result.html`, means the current edition's copy, and 241 of
that repository's 403 missing targets are that one shape.

`built-page` is the rest of the same book. A destination ending in `.html` that no book source
answers names a page of the built site rather than a file. The rust book's `redirects/` tree
writes 79 of those and its chapters write 33 more, climbing to `std`, `reference` and
`nomicon` on the same domain. None of them is a break and none is placeable, so all take the
boundary, and what stays missing is what always was: 28 images under `nostarch/` and two paths
in a crate README.

Zola is modelled through the one prefix it spells. `@/` opens a path from the `content`
directory beside `config.toml`, so `@/documentation/page.md` in Zola's own repository is
`docs/content/documentation/page.md`, and 59 of its 80 missing targets resolve that way while
the other 21 stay the dangling theme files they are. That directory is also how a Zola
configuration is told from a Hugo one: both may be called `config.toml` and the name says
nothing, so Hugo is read from `hugo.toml` or `hugo.yaml`, the spelling it has preferred since
0.110, and a bare `config.toml` is read as Zola's only when a `content` directory sits beside
it. A Rust workspace's `.cargo/config.toml` has no content directory beside it, so it anchors
nothing and ripgrep, bat and helix keep every claim they had. Where a tree spells only
`config.toml`, the Zola reading wins, since it can add an answer and can never take a claim
away.

Jekyll's permalink template is what this leaves undone. `docs/_config.yml` in Jekyll's own
repository sets `permalink: "/:collection/:path/"`, every page URL then ends in a slash, and a
`../` climbs one level less than the file path does, which is 20 of that repository's 24
missing targets. Reading the template means opening the configuration file, and no rule here
opens one, so the boundary answers the whole `docs/` tree instead and those 20 go with it. All
three real breaks sit in `.github/`, outside the tree that file declares, and they stay.

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
end of evaluation and then refused at output, so nothing is counted from it. Of the three
that completed, jest's shape became a spelling and took a later survey to write, while Hugo's
and Jekyll's became rows that declare a boundary instead of serving a path.

Hugo's own documentation writes `[glob pattern](g)` and resolves `g` in its own
`render-link.html`, 734 references to a path that exists nowhere. Its 101 missing anchors are
two further mechanisms: 71 name a definition-list term, which its configuration turns into an
identity with `autoDefinitionTermID`, so `module.md`'s `files` term is published as
`<dt id=files>` while the pinned grammar has no definition list to read at all; 28 name a
heading pulled in by an `{{% include %}}` shortcode. Jest, on Docusaurus, links a document by
the identity that document declares in its own front matter: `Configuration.md` opens with
`id: configuration`, its page is published at that name, and 104 references reach it by URL
rather than by path. That one is the `document-id` spelling above, written after the same
shape turned up across six ordinary projects. On a later clone it answered 127 of jest's 130
missing paths, and the three left are real: `tutorial-react` is still in two versioned copies
and gone from the current documents and the two newest copies.

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
configuration, is not a spelling. What that configuration does decide is where every page
lands, and the `built-route` row says so rather than guessing at the template.

Hugo's `g` was the case for the generated class, arriving as transclusion and as a
repository's own render hook. The exact-path core of that class is answered now, from the
tracked ignore file recorded in [Reference coverage](completed/reference-coverage.md), and
both of those arrivals still sit outside it. What moved is the verdict rather than the class:
a destination a render hook rewrites is undecided instead of missing, so hugoDocs reports 123
missing targets where it reported 618, and every one of them is an anchor.
