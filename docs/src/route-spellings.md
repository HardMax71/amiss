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
| `extensionless` | `guide` | `guide.md`, `guide.mdx`, `guide.markdown` | vitepress |
| `output-extension` | `guide.html` | `guide.md` | mdbook, vitepress |
| `readme-index` | `dir/index.md`, `dir/index.html` | `dir/README.md` | mdbook, vitepress when configured for it |

A spelling only ever names a file that is already in the tree, so it can widen what
resolves and can never invent a target. Everything a spelling does not reach stays exactly
as missing as it was, under the destination the author wrote.

The elided extension is tried under each suffix a page source carries, `.md` first, then
`.mdx`, then `.markdown`. A router serving a page at a clean URL serves an MDX page there
the same way, so reading `guide` as `guide.md` alone leaves `[items](./item)` missing in a
tree whose page is `item.mdx`. Ionic's documentation writes that link in seven pages and
reaches `item.mdx` from every one, 24 references counted once per versioned copy. A
destination already carrying one of those suffixes names a source file rather than a route,
so `page.mdx` is never looked up as `page.mdx.md`.

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
on the document's ancestor chain, nearest directory first. Presence is what selects a rule.
Two of them then read one value out of the file that selected them, the Antora component name
and the Sphinx source suffix, and the rest never open it.

Which sites a tree declares is a question about the whole tree, and which site owns a document
is a question about that document. Where nothing on the chain declares a generator and the
tree declares it in exactly one directory, that directory owns the document, because a site in
`website/` reading `../docs` leaves every page it publishes off its own chain. A tree that
declares the same generator in two places fixes no owner, so a document outside both keeps the
chain's answer. Five rules stay on the chain whatever the tree holds: `astro`, `eleventy`,
`hugo`, `jekyll` and `mdbook-pages` answer for a page the build serves rather than for a file,
so turning one on withholds an answer instead of finding one, and so does the partial rule
further down. A configuration under a tree the scan already excludes declares nothing, which
is how sphinx's own repository carries 176 `conf.py` and declares one site.

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

A component is not one directory. Antora assembles it from every source root whose
`antora.yml` spells the same name, so a module coordinate is answered by whichever of those
roots holds the resource. Spring Boot has four roots and all four declare `name: boot`, which
is why `xref:gradle-plugin:packaging-oci-image.adoc` written under `documentation/` means a
page stored under `build-plugin/`. The name is the one thing read out of the descriptor, a
plain scalar on a line of its own, and a root naming another component answers nothing. The
document's own root is asked first, so a finding still names the path under the root the
author wrote in.

The descriptor also says when the tree is not the whole component. The `ext` block is what
Antora hands to the extensions that assemble a component while the site is built, and this
engine runs none of them, so a component reserving that block holds resources no tree walk
can enumerate. All four of Spring Boot's descriptors reserve it, and its build zips generated
pages and partials into `modules/appendix/partials/configuration-properties`,
`modules/api/partials/rest/actuator` and eleven more directories before Antora reads any of
them. A resource ID such a component does not answer is `unsupported-reference-semantics`
with the unmodelled-route reason rather than a missing file. Where no descriptor reserves the
block, as in Asciidoctor's own documentation, every claim stands.

`site-alias` and `content-root` are the order Docusaurus's `resolveMarkdownLink` tries
directories in, read from `packages/docusaurus-utils/src/markdownLinks.ts`, for a document the
site owns: one under the directory holding `docusaurus.config.ts` or its `.mts`, `.cts`,
`.js`, `.mjs` and `.cjs` spellings, the list the site loader tries, and every document in a
tree holding one such file and no other. `@site/blog/img/output.png` is
`blog/img/output.png` under that directory, for a link and for an image. A bare `.md` or
`.mdx` destination, one starting with neither `./`, `../` nor `/`, is tried beside the
document first, then under the plugin content path the document sits in, then under the
site directory. `[static folder](static-assets.mdx)` in `docs/api/themes/configuration.mdx`
reaches `docs/static-assets.mdx` that way. The content paths read are the plugin defaults,
`docs`, `blog`, `src/pages` and `versioned_docs/<version>`, under the site directory and,
for a document outside it, under the directory holding the site, which is where a site in
`website/` finds the `../docs` its configuration names. So `[View](view.md)` in react-native's
`docs/legacy/direct-manipulation.md` reaches `docs/view.md`, the file the archived copy of
that same page under `website/versioned_docs/` has always reached. A plugin configured to read
another directory gets the site-directory step alone, and a localized tree under `i18n/` is
not read. A `./` or `../` destination is beside the document and nowhere else, which is
Docusaurus's rule too.

Where no `docusaurus.config.*` sits above the document and the tree holds several elsewhere,
`@site/` is a destination this run cannot answer rather than a missing directory called
`@site`: the alias is expanded when the site is built, and with more than one site in the tree
nothing here says which directory it names. That is
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

This rule asks the document's own ancestor chain nothing at all. A site names its
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
`docs/testing.rst`: the docname under the source directory, with the suffix that root reads.
A docname is a file name without its suffix, so the dot in `/releases/1.1` is part of the name
and that target is `docs/releases/1.1.rst`, while a trailing slash is normalized away before
the name is looked up, the way `docname_join` does it, so `` :doc:`</ref/applications/>` `` is
`docs/ref/applications.rst`. A plain hyperlink with a leading slash is still a site route,
since Sphinx emits it as written. A `:doc:` target in a document with no `conf.py` above it is
answered by the one source directory the tree declares, and stays the declared site route it
was where the tree declares several.

Which suffix that root reads is the one thing read out of `conf.py`. A project that writes its
pages in another suffix says so in `source_suffix`, and Django writes all 677 of its pages in
`.txt`. Two forms are read, both on one line with the key opening it, which is where a Python
assignment binds a name at the top level: `source_suffix = ".txt"` is one suffix, and
`source_suffix = {".txt": "restructuredtext"}` is the mapping form, whose value has to name the
reStructuredText parser, since `{".md": "markdown"}` says the opposite of what this rule wants.
A commented-out line, an `add_source_suffix` call, a mapping left open across lines, and a list
naming no parser are all declined, and a root that declares nothing this reader spells out
plainly reads `.rst` as before. A root naming several reStructuredText suffixes is read under
the first, because a docname names one file. So `` :doc:`/ref/models/querysets` `` in Django's
tree is `docs/ref/models/querysets.txt`, and the same declaration is what makes those files
documents at all, which [Discovery](discovery.md) states.

A relative `:doc:` target keeps `.rst` whatever the root declares. The adapter spells that one
while it parses, before anything knows which root the document sits under, and rewriting it
afterwards would guess at a name the author never wrote. Django writes four relative targets in
677 pages and all four name another project's inventory, so the boundary costs it nothing;
a tree that writes relative docnames under another suffix would see them reported missing under
`.rst`, which is the honest reading of what this rule does not do yet.

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

The opening is what decides, and the closer need not be there at all. A destination ends
where the grammar holding it says, and that cuts an expression in half whenever the
expression quotes something of its own. HTML ends an attribute value at the next quote, or at
the first space when the value is unquoted, so `<a href="{{< ref "#LifecycleHandler" >}}">`
hands the resolver `{{< ref ` and `<a href={{ ref . "x" }}>` hands it `{{`. Read as a relative
path, each claims a file nobody wrote under a name nobody spelled. Kubernetes' website tree
writes the first shape 2,774 times.

A name carrying one brace is still an ordinary path and resolves as written, and so is one
carrying a closer alone. Jinja, Liquid, Nunjucks and Handlebars all spell an expression this
way, which is why the rows are the delimiters rather than the name of one generator. A tree
that holds a real file whose name spells an expression, as a cookiecutter template does, gets
this answer for it instead of the file, and so does one whose file opens an expression it
never closes.

## What the build answers instead of the tree

A generator that does not put its pages where its sources are breaks the assumption behind
every rule above, that a relative destination names a place in the tree. Hugo, Jekyll,
Eleventy and Astro each resolve one against the URL the page is served at, and that URL comes
out of a configuration this engine does not open. Twelve ordinary projects put a number on it:
of 1,596 missing targets, the 1,149 from repositories built on a generator with no rule here
held 41 real breaks. So `built-route` answers such a destination with
`unsupported-reference-semantics` and a reason saying the route model is not modelled, the way
an absolute site route already does, instead of naming a file nobody wrote.

A page's own source is out of that reach. A build reads `foo.md` and publishes what it made
from it under a URL of its own, so no URL it serves is called `foo.md`, and no route model is
needed to say the destination reaches nothing. `built-route` therefore reads the destination's
own suffix, the three that `extensionless` appends, and leaves the tree's answer standing. It
is the other end of the pair `built-page` reads: `.html` is what a build serves, and `.md`,
`.mdx` and `.markdown` are what it consumed. An asset keeps the boundary, because its own
prefix still moves: Jekyll's docs write `<img src="../../img/jekylllayoutconcept.png">` from a
page served at `/tutorials/convert-site-to-jekyll/`, the climb lands on the site root rather
than on the repository root, and `docs/img/jekylllayoutconcept.png` is there.

Sixteen destinations across a 56-repository corpus are that shape, out of the 1,523 the
boundary covered, and they carry twelve findings. Eleven are breaks. Kubernetes' website goes
from 89 missing targets to 97: six translated READMEs link a `code-of-conduct.md` that only
the repository root holds, and one Korean page copied two English links without their leading
slash. Jekyll gains the `docs/pages/team.md` its `docs/_docs/security.md` writes, which that
repository already reports from the copy of the same sentence under `.github/`. The eleventy
site gains two siblings written with a directory too many, and the twelfth finding is a page
of it showing `<a href="my-template.md">` as an example of markup rather than as a link.
hugoDocs, the rust book and spring-boot do not move, since no destination behind their
boundary names a source.

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
`../` climbs one level less than the file path does, which is 19 of that repository's 24
missing targets. Reading the template means opening the configuration file, and no rule here
opens one, so the boundary answers the whole `docs/` tree instead and those 19 go with it.
Three of the four real breaks sit in `.github/`, outside the tree that file declares, and the
fourth is the page source `docs/_docs/security.md` names.

## What a build's own configuration says

A declaration says two things: which directory holds the pages, and what path the site is
served at. A Hugo configuration already says both, so a repository keeping one is not asked to
write them again. `contentDir` names the directory, read against the directory the
configuration sits in, and the path of `baseURL` is what every route opens with. Where the
file names no directory, Hugo reads `content` and so does this.

Only the project's own bindings are read. A line indented under a table, and everything after
the first table header, belongs to that table rather than to the project, so a `contentDir`
under `[languages.fr]` names no root here. A multilingual site answers the routes into the
directory its project names and leaves the rest the site routes they were.

A root belongs to its project rather than to the pages beneath it, since one page of a site
routes to another wherever either sits. That is what lets a Japanese page on the Kubernetes
website reach the English page its route names. Read whole on 2026-09-21 under the observe
profile, its site routes fall from 28,075 to 13,626 and resolved references rise from 5,073
to 18,584, with no missing path arriving. Nineteen claims arrive, every one an anchor on a
page that only resolves now, and fifteen name a heading the page does not carry. The other
four are headings written as a shortcode, which this reads no better than before.

## What a repository declares about its own build

Every rule so far is selected by a file in the tree, so a repository that keeps no
configuration gets no rule at all. That is the common case for documents whose site is built
somewhere else. Of the 54 repositories behind this page, 16 hold no generator configuration at
all, 4,133 scanned documents between them. Some are plain trees that a forge renders, and they
want nothing here. The rest are content a generator reads from another place: Grafana's Hugo
configuration is in a Docker image and a sibling repository, and Babel's site is built from
`babel/website`.

So the repository says it, since nothing in such a tree can. `.amiss/router.yml` names one
router on a line of its own, `router: directory-pages`, and the directory holding that file
is the root the rule anchors at, where the configuration file would have been. The nearest
declaration above the document answers and nothing else in the file is read, the way a
component descriptor is read for its `name`.

<!-- amiss-doc-contract:declarable-routers:start -->
| Declared router | Turns on |
| --- | --- |
| `docusaurus` | `site-alias`, `content-root`, `document-id` |
| `mkdocs` | `directory-url` |
| `sphinx` | `source-root` |
| `zola` | `content-root` |
| `directory-pages` | `page-url` |
<!-- amiss-doc-contract:declarable-routers:end -->

A declaration turns on the spellings that resolve a destination and no others. `built-route`
and `built-page` are not in that table and cannot be reached from a file a repository writes,
because those two withhold an answer rather than serve a file: a repository that could reach
them could clear its own findings with one line. What a declaration does reach is bounded the
way every spelling is. It can move a destination onto a file the tree already holds, and it
cannot clear a destination the tree lacks, so declaring `hugo` or `jekyll` or `astro` turns on
no spelling at all. There is still no ignore file and no way to silence a finding. The `base`
key below is the other half of the file and is read whichever router the line above it names.

`page-url` is the row a declaration exists for. A site of that shape publishes a page at a
directory of its own name and rewrites no destination, so a relative destination resolves
against the page URL, one level deeper than the source file's directory, and a Markdown link is
read that way as well as an `<a href>`. A page bundle is published at the directory holding it,
so `_index.md` and `index.md` are the sources a destination naming that directory reaches. The
reading from the document's own directory comes first, so a destination that already reached a
file beside the source still reaches that file and still fixes the intent a finding names. The
page URL is a candidate added, never one replacing another.

The row is named for the shape because no one generator owns it. Hugo builds it by default,
Jekyll's `permalink: pretty` and its `/:collection/:path/` template produce it, and Eleventy
and Astro build it unless asked for a file. It was called `hugo-pages` in the release before
this one, so a Jekyll repository had to declare Hugo to describe its own site. A name for each
generator would be one reading under four spellings, every one a row to keep and a row to test.
mkdocs publishes this shape too and keeps its own row, since it rewrites a Markdown link back
to the source path and leaves raw HTML alone, so only raw HTML is read against the page URL
there.

This is the one row a configuration file does not select, and Hugo's own documentation is why.
`hugo.toml` says Hugo builds the tree and nothing about the URLs it serves: `uglyURLs` moves
every page, a permalink template moves it again, and a `render-link.html` render hook rewrites
the destination before any of that, which is what hugoDocs does to its own `[glob pattern](g)`.
Jekyll's `_config.yml` carries the same question in its `permalink` line, and neither answer is
in the tree, so a tree carrying either file keeps the `built-route` boundary it had. The
declaration is the repository's own word that its pages are published at a directory of their
own name with nothing rewritten, which is a claim only a person can make.

Grafana is the case it was built against. `docs/sources/setup-grafana/set-up-grafana-live.md`
writes `[ha_engine_address](../configure-grafana/#ha_engine_address)`, and the page it reaches
is `docs/sources/setup-grafana/configure-grafana/_index.md`, one directory deeper than the
source-relative reading, which is why 544 missing targets were reported for a tree whose
maintainers had not broken 544 links. With `router: directory-pages` under `docs/sources` that
count is 278. The 266 claims that went are one shape. A separate reading of the page URL was
run over the references that resolve where they stood: 270 of 271 match it, and the one left
over reached a file beside its own source once the trailing slash stopped naming a tree.
Nothing moved into the undecided class, which stayed at 1,153 rows, and every one of the 52
anchor claims still reports. Another thirty-one joined them, since a path that resolves has
its fragment read: `../../datasources/tempo/#span-filters` reaches the Tempo page, which
publishes no such heading now that span filters are a page of their own.

A page that moved leaves the URL it was served at behind, and that block is read under the same
declaration. Hugo lists those URLs under `aliases` in front matter, serves a redirect at each
one, and reads a relative entry against the directory the page's own URL sits in, one level
above the route the page is published at. Grafana's own blocks show it: 784 of its entries
carry the URL they produce in a trailing comment, 735 of those match that reading, and 6 match
the page URL itself. Hugo's documentation says the same, giving `old-name` and `../old/path` on
`content/examples/example-1.en.md` as `/en/examples/old-name/` and `/en/old/path/`.

The grammar is a key opening a line with no value of its own, then the lines under it opening
with a dash and a space, and the first line shaped any other way closes the block. A flow
sequence, a plain scalar, and an entry opening with a slash are declined rather than guessed
at, so this stays a reader of one block and not a reader of YAML. A URL two pages claim is left
out the way a route two pages publish is, and the page that answers is a file the tree holds,
so a block widens what resolves and cannot invent a target. Only the reading anchored at the
page URL is answered from it, since the reading beside the source is a path and no URL at all:
answering that one would clear `docs/sources/administration/cli.md`'s
`../developers/http_api/user/`, whose page URL is `administration/developers/http_api/user` and
is claimed by nothing.

Grafana's 278 becomes 149. Each of the 129 claims that went names a URL exactly one page
declares, out of the 1,276 single-claimant URLs its 1,336 relative entries spell, and no claim
arrived. The undecided class stayed at 1,153 rows again. Of the 195 paths that were missing,
152 resolve, and 23 of those become anchor claims because a path that resolves has its fragment
read: `../../panels-visualizations/visualizations/time-series/#connect-null-values` reaches the
time series page, which publishes no such heading now that its options come from a shared file.

Which side of a comparison the declaration is read from decides whether a repository can adopt
one at all. It is tree state, so the commit that writes the file has a base without it, and read
each side its own way, Grafana's adoption reports 476 claims resolved and 210 introduced, mostly
anchors that become readable the moment their path resolves. Every one of the 210 was already
broken and the author of that commit touched none of them. So both sides are read under the
declarations the candidate holds. Grafana's adoption then reports the 278 the tree has either
way, all of them pre-existing, and nothing introduced.

Removing one is that rule read backwards, and that direction is reported. The candidate declares
nothing, so neither side does, and Grafana's 544 claims come back, every one pre-existing. What
the symmetry cannot do by itself is say that a claim went: a fragment is read because its path
resolved, and once the declaration goes the path is missing instead, or, under a `built-route`
generator above the document, a declared boundary. So a declaration the base held that the
candidate does not hold identically is a `policy-weakened` control finding at the file that held
it, `router/declaration-removed` where the candidate declares nothing there and
`router/declaration-replaced` where it names another router or another base. That is the one
thing the declaration reports about itself, and it reports it in every profile.

The second key answers the destinations that open with a slash. Those are 54,717 of the 117,747
in-scope references across 56 repositories, 46.5 percent, more than the resolver answers, and
each one names a page of a site rather than a file. Which site is the part no tree holds: the
Kubernetes community repository writes `/docs/comms/slack/` and means kubernetes.io. So the
declaration says it. `base` is the URL path the directory it sits in is served at, and a route
opening with that base is a path under that directory:

```yaml
router: astro
base: /
```

Under that file at `src/content/docs`, `/en/guides/astro-components/` is
`src/content/docs/en/guides/astro-components`, which the extensionless spelling reads as the
`.mdx` beside it, and the trailing slash names the page rather than a tree the way it does
under `page-url`. Grafana serves `docs/sources` at `/docs/grafana/latest/` and writes that
prefix into its own links, so its base carries it and a route without it stays where it was.

The tree has to hold the page. A route that reaches no file keeps the boundary it had, because
a site serves routes its own build makes up and a tree can enumerate only what it holds:
Astro's translated pages fall back to English, Kubernetes generates its API reference from
OpenAPI, its blog is published at a permalink its sources do not spell, and its images come
from a directory the declaration does not cover. All four stay declined, and across five
declared trees not one missing-target claim arrived. What does arrive is anchors, because a
path that resolves has its fragment read: 61 anchor claims against 15,890 references resolved,
every one read by hand. Fifty-six name a heading the page it reaches does not publish, three
name one a Hugo shortcode writes, and two name an anchor a generated reference page spells
three ways. A declaration with no `base` line reads exactly as it did before.

A route naming a directory reaches that directory, since a tree holding `guides/index.mdx`
holds `guides`, and a fragment on a directory stays undecided the way it already does for
`../guides/#setup`. That is 1,106 of those references, with 302 fragments left unread. Nothing
else about the reading is new: the destination is a path under the declared directory, read by
the spellings every other path is read by.

Four repositories show what the file does, each read whole on 2026-09-20 under the observe
profile, once as it stands and once with the file below staged.

Astro's documentation keeps its pages under `src/content/docs` and serves that directory at
the root of its site, so all 10,323 of its slash-rooted destinations are routes nothing in the
tree answers. The file goes at `src/content/docs/.amiss/router.yml`:

```yaml
router: astro
base: /
```

The router line turns on no spelling there, since `built-route` withholds an answer rather than
serving a file, and the base is read whichever router the line above it names. Site routes fall
from 10,323 to 1,647 and resolved references rise from 3,470 to 12,133. Three claims arrive,
each an anchor on a page that only resolves now, and no missing path arrives at all.

Kubernetes serves its website out of `content/en` and its `hugo.toml` says so, which is why
that repository is no longer an example here: the configuration is read where it sits, and a
declaration staged beside it moves not one reference. The section below is what answers it.

Jekyll's own repository is why the row is not named after Hugo. `docs/_config.yml` sets
`permalink: "/:collection/:path/"`, which publishes every page at a directory of its own name,
and one line at `docs/.amiss/router.yml` says so:

```yaml
router: directory-pages
```

That resolves 19 references, 76 becoming 95, and moves no claim: the four that are missing sit
in `.github/`, outside the tree the file covers, and stay missing. It is the smallest useful
declaration on this page and the whole of what a Jekyll repository writes.

Grafana is the tree above, read again here on a newer clone than those paragraphs were written
against. Both keys go at `docs/sources/.amiss/router.yml`:

```yaml
router: directory-pages
base: /docs/grafana/latest/
```

Its claims fall from 546 to 153, and the 494 missing paths become 45. The 52 anchor claims
become 108, since a path that resolves has its fragment read. Without the base line the count
is 151 instead of 153, so the second key is worth two claims and 52 references on this tree,
where on Astro's it is worth all 8,663.

Nothing in the output suggests writing one, and nothing honestly could. A tree says whether a
generator is configured inside it; it never says whether its documents are published at all. So
one structural fact covers both a repository built elsewhere and a repository with no site,
whose missing targets are simply broken. A note keyed to that fact would have reached the
Kubernetes community repository's 254 missing targets, bat's 21 and vuejs-docs' one, none of
which a declaration should touch.

That repository is also the one to read before writing a base, because its slash-rooted
destinations do not name one site. `communication/slack-moderation.md` writes
`/docs/comms/slack/#reporting-a-problem` and means k8s.dev, other pages mean kubernetes.io, and
`/sig-list.md` and `/CLA.md` mean the repository root, which no site serves them from.
A `base: /` at its own root reads all of them against the tree, and 737 references resolve
where 828 routes were declined. Its 106 missing paths do not move, since a base can no more
invent a target than any other spelling can, but 50 of the newly resolved paths carry a
fragment the page they now reach does not publish, so 254 claims become 304. The key is one
claim about where one directory is served, and a tree whose links point at three sites has
none to make.

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
