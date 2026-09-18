# What twelve renderers call a heading

A heading anchor is not a property of Markdown. `## Setup & Config` has no identity until
something renders it, and the renderers disagree: github.com publishes `setup--config`,
VitePress publishes `setup-config`, and Gitea publishes neither if the heading is empty
after its filter. Checking `guide.md#setup` therefore means knowing whose rule applies, and
guessing one would report live anchors as missing.

[Resolution](resolution.md) describes what the resolver does with that. This page retains
what the rules are, where each came from, and what each was checked against, because a
slugging rule that quietly stops matching its renderer looks exactly like one that still
matches.

## The rules

| Rule | Serves | Distinguishing behavior |
| --- | --- | --- |
| `github` | github.com, GitLab, Docusaurus, Hugo's github type | keeps letters, marks, numbers and connector punctuation; one separator per space; the only rule that also anchors a heading written as raw HTML |
| `gitea` | Gitea 1.27 repository files and wiki pages | drops marks; publishes nothing for an empty identity; never suffixes a repeat |
| `forgejo` | Forgejo 16 repository files and wiki pages | Gitea's filter, but an empty identity becomes `heading` and repeats take `-1` |
| `mdbook` | mdBook with smart punctuation off | Rust's alphanumeric test, so Indic vowel signs survive where Gitea drops them |
| `mdbook-smart` | mdBook as it ships | the same, after `--` becomes an en dash and `...` an ellipsis |
| `goldmark` | goldmark embedders keeping its own ids | drops every multi-byte rune; `_` becomes a separator; empty becomes `heading` |
| `python-markdown` | MkDocs with the default toc slug | NFKD then ASCII fold, so `Café` is `cafe` and CJK is empty; repeats take `_1` |
| `pymdownx` | MkDocs configured with `pymdownx.slugs.slugify` | keeps Unicode; one separator per space; repeats take `_1` |
| `mdit-vue` | VitePress, VuePress | a wide punctuation class collapses to one separator; a leading digit takes `_` |
| `kramdown` | Jekyll, GitHub Pages | strips the leading run of non-letters; ASCII only; empty becomes `section` |
| `docutils` | Docutils and Sphinx | every non-alphanumeric run becomes one separator, so `foo_bar` is `foo-bar`; a leading digit run is stripped rather than prefixed; NFKD then ASCII fold, so `Ⅻ chapter` is `xii-chapter` |
| `asciidoctor` | Asciidoctor and Antora, at the default `idprefix` and `idseparator` | the only rule whose separator is `_` and whose every identity carries a fixed prefix; hyphens and dots survive as themselves; repeats number from `_2` |

The AsciiDoc rule is the one pinned to a configuration rather than to a renderer's only
behaviour. `idprefix` and `idseparator` are document attributes, and this engine evaluates no
attributes, so the rule holds their defaults and a document set that overrides either publishes
identities the rule does not know. Two divergences are known and unverified against a running
Asciidoctor: a title whose every character is filtered away publishes nothing here, and the
attribute-driven cases above.

An anchor resolves when any of them would publish it, or when the document declares it
outright. Adding a rule can only grow that set, so a rule missing from the table is the
only way a live anchor is reported absent, and nothing a repository declares can shrink it.

Two of the rows are configurations rather than renderers. mdBook ships with smart
punctuation on and MkDocs takes its slug function from `mkdocs.yml`, so both spellings are
carried rather than one being chosen for the reader.

## What a document declares

An identity can also be written down rather than left to a heading's slug, and then it
belongs to the author or to a construct no heading rule reads. Each of these spellings
joins the union for every renderer, because accepting an identity a given renderer would
not publish can only leave a finding unreported, never invent one. Six of the rows write
down no identity: they are the spellings a declared generator, hook, extension or layout
owns, four saying the page's identities are built elsewhere and two naming a reference.
Seven rows are gated on a file in the tree, the way the route rules are; the rest are read
wherever their profile is.

<!-- amiss-doc-contract:declared-identities:start -->
| Declaration | Spelling | Read in | Selected by |
| --- | --- | --- | --- |
| `html-id` | an `id` or `name` attribute on a raw HTML element, or on one written inside an `mdx-code-block` fence | `markdown`, `mdx` | any tree |
| `attr-list` | an attribute block alone on a block's first or last line, `{#id}` | `markdown` | any tree |
| `attr-list-inline` | an attribute block directly after an inline construct or a bracketed span, `**text**{#id}` or `[text]{#id}` | `markdown` | any tree |
| `definition-term` | a term line above a `: ` definition line | `markdown` | any tree |
| `mkdocs-snippet` | a `--8<--` line naming a quoted path, alone on the line | `markdown` | `mkdocs.yml`, `mkdocs.yaml` |
| `mkdocs-directive` | a `:::` line naming what a generator renders, alone on the line | `markdown` | `mkdocs.yml`, `mkdocs.yaml` |
| `mkdocs-shortcode` | an HTML comment naming a hook's shortcode, `<!-- md:name -->` | `markdown` | `mkdocs.yml`, `mkdocs.yaml` |
| `mkdocs-content-tab` | a content tab opening a quoted title, `=== "Title"` | `markdown` | `mkdocs.yml`, `mkdocs.yaml` |
| `hugo-shortcode` | a shortcode call alone on its line, `{{% name %}}` or `{{< name >}}` | `markdown` | `hugo.toml`, `hugo.yaml` |
| `myst-target` | a target alone on its line, `(name)=` | `markdown` | any tree |
| `myst-directive-name` | a directive's `:name:` option, or a `figure-md` opener's argument | `markdown`, `rst` | any tree |
| `myst-glossary` | a term of a definition list opening with `{.glossary}` | `markdown` | any tree |
| `myst-domain-object` | a `domain:type` directive's object name, `{py:class} widgets.Widget` | `markdown` | any tree |
| `myst-role` | a cross-reference role, `` {doc}`name` `` | `markdown` | `conf.py` |
| `myst-link` | a plain link naming a label, `[text](name)` or `[text](#name)` | `markdown` | `conf.py` |
| `mdx-comment` | an MDX comment ending a heading, `{/* #id */}` | `mdx` | any tree |
| `mdx-heading-id` | an MDX expression ending a heading, `{#id}` | `mdx` | any tree |
| `jsx-id` | an `id` attribute on a lowercase JSX element | `mdx` | any tree |
| `mdx-partial` | a default import of a relative document, rendered as an element | `mdx` | any tree |
| `asciidoc-anchor` | a block anchor alone on its line, `[[id]]` or `[#id]` | `asciidoc` | any tree |
| `asciidoc-inline-anchor` | an anchor in the flow of a line, `[[id]]` | `asciidoc` | any tree |
| `asciidoc-reference-text` | a section title a natural cross reference names | `asciidoc` | any tree |
| `rst-target` | an internal hyperlink target, `.. _name:` | `rst` | any tree |
<!-- amiss-doc-contract:declared-identities:end -->

`html-id` is every `id` and `name` a raw HTML region carries, wherever in the document it
sits. It reads an `mdx-code-block` fence too, because that fence is markup rather than
code: Docusaurus strips the two fence lines out of the file before anything parses it, so
the `<details id="node-env">` written between them opens an element of the built page and
the `id` on it is an identity of that page. Its loader unwraps the three- and
four-backtick spellings with nothing but the word on the opener, so those are the ones
read here and every other fence stays code. On the Docusaurus tree that spelling is
thirteen of the missing-target findings, one link written into thirteen versions of the
same page.

`attr-list` is the extension's own block, in any of the spellings it accepts:
`{#id}`, `{ id="id" }`, `{ id=id }`, among classes, and with kramdown's leading colon. A
block whose last line is nothing but an attribute block declares that identity for itself,
which is how `[](){#anchor-point}` and a `{#section}` line under a paragraph work; an
attribute block trailing other text on the same line declares nothing, and one inside a
fence is code. The extension reads the block in the document's own literal text, so a
block inside inline code is code and declares nothing. A quoted value is one attribute
however many spaces it holds, so
`### with pip <small>recommended</small> { #with-pip data-toc-label="with pip" }` still
names `with-pip`.

The other end of the block is MyST's `attrs_block`, which writes the identity of what
follows on the line above it rather than on the last line of the block itself.
`{#paragraph-target}` alone above a paragraph names that paragraph, and the identity is
the same one the trailing form declares, so both ends are read. A directive opener leaves
no blank line under it, so the opener and the block it holds share one paragraph and the
attribute block is its second line rather than its first. `{#mypara}` under `:::{note}`
still names the paragraph beneath it.

`attr-list-inline` is the extension's other half, the block that attaches to the inline
construct it directly follows rather than to the block around it. That is how
`*   **\`locale\`**{ #mkdocs-locale }: the locale used` names a list item's own term while
the sentence carries on after it, and the identity is the same one the block form declares.
The block opens the text that follows the construct, because anything between the two
breaks the pairing. A bracketed span is the other carrier, `[text]{#id}`, which MyST's
`attrs_inline` and Pandoc both write. A bracket pair naming no definition is plain text to
the Markdown grammar, so no node is built for one and the `]` is the construct's own end: a
block against one is read from the text it sits in wherever that text falls.

`definition-term` is the one row derived from the document's text rather than written
down by its author. Hugo publishes an identity for a definition-list term under
`autoDefinitionTermID`, slugged by the rule its headings take and numbered on the counter
its headings occupy, so a `matchers` term above a later `## Matchers` publishes `matchers`
and then `matchers-1`. The term is the line above a line opening with `: `, and a second
definition under one term names no new term. Hugo's own flag decides whether any of this is
published and this engine does not read it, so a term joins the union wherever the
spelling appears. A page that writes a definition list in a tree Hugo never builds gains an
identity nothing publishes, which can leave a finding unreported and cannot invent one.

The two MDX rows are the same heading identity written two ways, because the attribute
spelling is an expression in that grammar. `mdx-heading-id` is the classic
`### Hello {#hello}`, which Docusaurus escapes before MDX parses the file and then reads
back out of the heading text; it is on by default and off under
`future.v4.mdx1CompatDisabledByDefault`. `mdx-comment` is
`### \`noIndex\` {/* #noIndex */}`, the spelling that needs no escape. Either one ends the
heading or it declares nothing, which is `parseMarkdownHeadingId`'s own rule and the reason
`{/* #id */} after` names nothing, and either one is taken as written with its case intact.

`jsx-id` is the MDX answer to `html-id`, because MDX has no raw HTML and `<details id="x">`
in an MDX file is JSX. The split JSX itself makes is the one this rule uses: a tag name
starting with a lowercase letter is an HTML element, so its `id` reaches the page and is
read, and every other name is a component. A component's rendered output is unknown here,
so an `id` written on one is a prop rather than an identity, and nothing under one is read
either. That is why the option tables Docusaurus builds with `<APITable>` publish
identities this check cannot see, and why a link into one stays a finding.

`mdx-partial` is the one rule that reads another file. Docusaurus composes a page out of
several documents by importing one and rendering it, `import Tags from './_tags.mdx'` and
then `<Tags />`, and the headings that partial writes are headings of the page. The import
resolves through the same path rules a link uses, only to a document the tree already
holds, and only for a default import of a relative `.md` or `.mdx` file rendered as an
element, so a package, an alias, or a stylesheet declares no edge at all. A partial that
renders a partial expands too, under the include budget in
[Resolution](resolution.md), and two documents rendering each other leave the set past the
cycle undecided rather than recursing.

The identities flow one way. The page publishes what the partial writes, because the
partial's headings are on the page. A link written inside a partial that names a heading of
the page rendering it is a different question, and this check does not answer it: the same
partial can be rendered into several pages, so the identity that link names belongs to a
rendering context rather than to the file the link sits in. That is a known gap, and on the
Docusaurus tree it is eight of the findings.

`mkdocs-snippet` does the same composition in Markdown, and it is the one row this table
gates on a file. The syntax belongs to the `pymdownx.snippets` extension, so the line is
read only when `mkdocs.yml` or `mkdocs.yaml` sits on the document's ancestor chain, the same
test [Route spellings](route-spellings.md) applies to a generator's own routes. Without one
the line is ordinary text and includes nothing, which is what it is. The path is resolved
from the directory holding that file rather than from beside the document, because that is
where MkDocs runs, so a page whose whole body is `--8<-- "CONTRIBUTING.md"` publishes what
the repository's own contributing guide publishes. Only the single-line form with a quoted
path is read; a path carrying a section coordinate names part of a file, which this engine
cannot reproduce, so that edge is refused and absence in the page stays undecided.

`mkdocs-directive` is the line that pulls content in from outside the tree, and the one
edge this engine reads and then stops at. `::: pydantic.config` asks mkdocstrings for the
documentation of a Python object and `::: mkdocs-click` asks that plugin for a command
tree, so the headings the built page carries come from a program rather than from a file.
The page keeps everything it writes itself: an anchor naming one of its own headings still
resolves, and an anchor naming none of them is declared unsupported instead of reported
absent, which is the same answer a snippet the engine cannot follow gives. Only the form
with whitespace after the marker is an instruction, so the `:::note` and bare `:::` fences
Docusaurus writes name no generator; a VitePress container such as `::: tip` is the same
shape, and it is read only in a tree that declares MkDocs, which a VitePress tree does not.
That is the gate: without `mkdocs.yml` or `mkdocs.yaml` above the document, three colons are
three colons.

`mkdocs-shortcode` is the page a hook rewrites before anything is rendered. Material for
MkDocs ships one that replaces every `<!-- md:name argument -->` comment in the Markdown,
so `#### <!-- md:setting config.blog_dir -->` reaches the built page as a heading whose
identity is `config.blog_dir`. A comment carries no text under any rule in the table, so
what this engine reads there is an empty heading, which is not what the page publishes. The
page keeps every heading it writes itself and an anchor naming none of them is declared
unsupported instead of reported absent, which is the answer the generator instruction
gives. Without that declaration above the document a comment is a comment.

`mkdocs-content-tab` is the same answer for `pymdownx.tabbed`. A tab opens with
`=== "Title"`, and the extension publishes an identity for the title under the slug
function the site configures, combined with the heading above it where
`combine_header_slug` asks for that. Both settings sit inside `mkdocs.yml`, whose presence
is all this engine reads, so a page carrying a tab leaves its identity set incomplete
rather than guessing which of the two spellings the build wrote. Three equals signs with
nothing quoted after them are prose.

`hugo-shortcode` is the same answer for Hugo, where the call names a template rather than a
file. `{{% include "_common/store-methods.md" %}}` reads like an include, but `include` there
is a shortcode the site defines under `layouts/`, and it works by fetching a page and
rendering it, so what arrives is a template's output. The headings in that output are
headings of the built page, and under `autoDefinitionTermID` so are the terms
`definition-term` reads, which is why an anchor into either was reported absent before. A
page that calls one now keeps every identity it writes itself and leaves the rest undecided.
On the Hugo documentation tree that is all twenty-three of its missing-target findings, and
no repository outside a Hugo tree moves by one.

Only a call standing alone as a block is read, since one written in the flow of a sentence
renders inside that sentence and can open neither a heading nor a term. Both markers count,
the `{{% %}}` form whose output is rendered as Markdown and the `{{< >}}` form whose output
is raw HTML, because a heading can arrive through either. That tree does not separate the
two readings: none of the eighteen pages whose only call is inline is anchored into at all,
so the narrow rule is the smaller claim rather than the measured one. Without `hugo.toml` or
`hugo.yaml` above the document a pair of braces is a pair of braces.

`myst-target` and `myst-role` are the two MyST spellings, which is how a Sphinx project
writes its pages in Markdown. `(name)=` alone on its line is the target: the renderer writes
that identity onto the block after it, and Sphinx keeps the same name as a label, so it also
joins the label table a `{ref}` is answered from. The role is the reference,
`` {doc}`quickstart` `` where reStructuredText writes `` :doc:`quickstart` ``, and
[Resolution](resolution.md) says which roles are answered and which are counted and left
alone. The target is read wherever the spelling appears, because an identity can only widen
the set an anchor may match, while a role is read only under a `conf.py`, so a brace before a
code span in an ordinary Markdown file stays the prose it is.

`myst-directive-name` is the name a directive carries. Every docutils directive takes a
`:name:` option and MyST spells a directive as a brace-tagged fence, so `:name: build-note`
under `:::{note}` publishes `build-note` the way `(build-note)=` above the block would. The
option is read under a `{name}` tag and nowhere else, so the `:::note` fence Docusaurus
writes and an ordinary code fence declare nothing. An `eval-rst` body is reStructuredText
and its directives spell the same option, so a `.. figure::` carrying `:name: rst-fun-fish`
inside a Markdown page publishes that name too, and a reStructuredText document publishes it
directly. `figure-md` takes the name as its argument instead, `:::{figure-md} fig-target`,
since it is MyST's own Markdown figure; every other directive writes a path or a title
there, so the argument is read under that one tag and the domain tags the next row names.

`myst-domain-object` is the argument of the other kind of directive, the one that describes
an object rather than formatting a block. Sphinx stores what `{py:class}`, `{js:function}`
or any other `domain:type` opener names under that name as written, so `[](#widgets.Widget)`
finds the class its own page describes. The colon in the tag is what marks one, which keeps
`{note}` and `{figure}` out of the reading. What follows the name is the domain's own
signature grammar and this engine parses none of it, so a parameter list comes off the end
and anything still carrying a space declares nothing. `{py:function} open(name)` publishes
`open`, and `{cpp:class} template<typename T> Holder` publishes nothing at all.

`myst-glossary` is a definition list read the way Sphinx reads one. A list opening with a
`{.glossary}` attribute block is a glossary, and what a glossary term publishes is the term
itself, spaces and all, rather than a slug of it, which is how `[](<#my other term>)` finds
it. One list can therefore publish two identities, the term as written here and the slug the
`definition-term` row gives it, and a list with no `{.glossary}` above it publishes only the
slug. Both of these rows are read wherever the spelling appears, since an identity can only
widen the set an anchor may match, and both reach the label table a `{ref}` and a plain link
are answered from only where a `conf.py` governs the page that writes them.

`myst-link` reads those same names from the other side, so it is a reference rather than a
declaration. Sphinx keeps every name a page declares as a global label, and a plain Markdown
link can name one where a path goes: `[alert extension](syntax/alerts)` in myst-parser's own
admonitions page reaches the `(syntax/alerts)=` target in a second file, and
`[colon_fence](#syntax/colon_fence)` reaches one through a bare fragment. The tree answers
first, so a destination naming a file still resolves as that file, and the label table is asked
only once the tree has said no. A name no document declares keeps the missing target it had,
since an undeclared cross reference is as broken as an absent path. The reading needs a
`conf.py` above the document the way the role does, because a label is a global name and a path
is not, so an ordinary Markdown repository is untouched.

The last rows belong to the other two profiles. `asciidoc-anchor` is the anchor an
AsciiDoc author writes on a line of its own, `[[id]]` or `[#id]`, in either case with the
identity being everything before the first comma; a section title that carries one publishes
it beside the identity the `asciidoctor` rule generates for the title text.
`asciidoc-inline-anchor` is the same `[[id]]` spelling written in the flow of a line, on a
list item or mid-paragraph, which Asciidoctor reads as an anchor on the construct it sits in;
an escaped `\[[` and one inside a verbatim span declare nothing. `asciidoc-reference-text`
is the section title itself: Asciidoctor resolves a natural cross reference such as
`<<API entrypoints>>` by looking the target up under its reference text, and a section's
reference text is its own title, so a title carrying a space or a capital publishes it.
`rst-target` is the internal hyperlink target, `.. _name:`, which Docutils turns into an
identity on whatever follows it. That name is also what a Sphinx `:ref:` looks up, and
[Resolution](resolution.md) describes that lookup, which is a different question from
whether a fragment names an identity.

A heading can also be written as raw HTML, which many projects do for a centered title.
github.com anchors those, because its filter runs over the rendered document and sees
`<h1>` and `##` in one sequence: the text content of the element is slugged by the same
rule, nested tags and comments contribute nothing, and a repeat of an earlier identity
takes the next suffix. Forgejo does not, verified on
[its own README](https://codeberg.org/forgejo/forgejo), where `<h1 align="center">Welcome to
Forgejo</h1>` is the only heading rendered without an identity while all four `##` headings
carry one. The rules built from a Markdown tree,
mdBook, goldmark, python-markdown, pymdownx, mdit-vue and kramdown, never see the element
at all. So this is the `github` row's behavior alone, and the union carries it.

## What each rule was checked against

The published expectations are in
[heading-anchor vectors](https://github.com/HardMax71/amiss/blob/main/spec/examples/heading-anchor-vectors.json),
which names the implementation behind every column. Twenty-four cases carry the
divergences: punctuation runs, intraword underscores, precomposed and decomposed Latin, the
Turkish dotted capital, CJK, a Bengali virama, an emoji variation selector, a Roman numeral,
a no-break space, and a heading that filters to nothing.

Seven rules have a runnable implementation, and against those the table reproduces all
9,049 headings harvested from the ten repositories in [The scan ledger](ledger.md) with no
mismatch: github-slugger 2.0.0 and comrak 0.54.0 for `github`, goldmark 1.8.4,
python-markdown 3.10, pymdownx, `@mdit-vue/shared`, and kramdown's own generator. The
remaining five are transcribed and traced by hand: Gitea's `CleanValue`, Forgejo's
`prefixedIDs`, mdBook's `id_from_content`, Asciidoctor's `Section.generate_id`, and Docutils'
`make_id`. The
[published vectors](https://github.com/HardMax71/amiss/blob/main/spec/examples/heading-anchor-vectors.json)
name which of the twelve is which and what each transcription is not checked against.

Eight documents, in
[`corpus/third_party/anchor-fixtures/`](https://github.com/HardMax71/amiss/tree/main/corpus/third_party/anchor-fixtures),
carry what a renderer actually published for them, harvested 2026-07-26:

| Document | Renderer | Identities |
| --- | --- | ---: |
| `probe.md`, this repository's own | github.com file view | 28 |
| `probe.md` | mdbook 0.5.4, default configuration | 28 |
| `probe.md` | python-markdown 3.10 with `toc` and `attr_list` | 28 |
| `probe-html.md`, this repository's own | github.com file view | 9 |
| `probe-attr.md`, this repository's own | python-markdown 3.10 with `toc`, `attr_list` and `fenced_code` | 7 |
| `probe-mdx-heading.mdx`, this repository's own | `@docusaurus/utils` 3.10.2, `parseMarkdownHeadingId` | 3 |
| `awesome-gitea.md`, CC0 | gitea.com | 50 |
| `starship-ja.md`, ISC | starship.rs, VitePress | 32 |

The github.com column comes from the file view, `/repos/{owner}/{repo}/contents/{path}`
under the HTML media type, which is the renderer that publishes heading anchors.
`POST /markdown` renders the same Markdown and publishes none, so a re-harvest through it
would come back empty rather than disagreeing.

The Gitea pair is the only live evidence for that rule and the only place its missing
duplicate suffix is visible: that one page publishes fifteen identities twice, so an anchor
into it is ambiguous on Gitea and unique on Forgejo, for the same file.

`probe-mdx-heading.mdx` is the same question in MDX, answered by the function Docusaurus
parses headings with under its `mdx-comment` syntax. Three of its seven headings declare an
identity to that call and four do not: one whose comment is followed by text, one with no
identity in the comment, one whose identity carries a space, and the plain `{#id}`
spelling, which that call alone does not read. Its pair is compared as a subset, since the
loader runs the classic spelling too and `mdx-heading-id` carries it. The identity `a{b}`
is in the set because their expression allows it.

`probe-attr.md` is the declared identities: four heading spellings, an empty link carrying
one, and a paragraph carrying one on its own last line. Five forms declare nothing, and they
are pinned too: a block trailing text on the same line, one inside a fence, and three inside
inline code, where the extension reads the syntax as the code it is. Its pair is compared as a subset
rather than as a list, because these identities join the union beside every rule's own.

`probe-html.md` is nine raw-HTML headings and one Markdown heading among them, which is
where the wrapped element, the decoded reference, the stripped comment and the shared
duplicate counter are pinned. Its `<h2>` written across three lines publishes
`--wrapped-title`, the leading newline and two spaces intact, which is the kind of detail a
transcribed rule gets wrong and a harvest does not.

## How far apart the rules actually are

Over those 9,049 headings, `github` and comrak agree on every one, which is why GitLab reads
the same identities as GitHub. `gitea`, `forgejo` and `mdbook` sit within 28 of them, and the
28 are combining marks, no-break spaces and connector punctuation. The site generators are
the outliers: goldmark's default differs on about 1,117, python-markdown on 1,431, and
mdit-vue on 1,856.

Switching MkDocs to `pymdownx.slugs.slugify` moves 1,431 of the 9,049 and lands within 22 of
github-slugger, which is the measured reason a MkDocs answer is a configuration rather than a
renderer.

## Renderer drift

Two of these implementations were rewritten inside twelve months. Gitea moved heading
identities out of goldmark and into an HTML post-processor in January 2026, shipped in 1.27,
which is where its missing duplicate suffix comes from. mdBook rewrote its generator in
September 2025 under a new HTML pipeline. github-slugger's character class exists only
because of a 2021 commit to match GitHub on Unicode, and its one later change was a Unicode
data bump made for the same reason.

That is what the fixtures are for. A re-harvest that disagrees fails a test instead of
silently changing a verdict.

## Translated trees

Translation mirrors are where anchor breaks concentrate in the wild. In
[the ledger](ledger.md)'s survey, 103 of the 122 real anchor breaks sit in starship's
translated pages, every one with the same shape: the heading was translated, its slug
moved with it, and the English fragment stayed behind in the links. A translated tree is
a first-class scanned surface, since translated readers follow the same links, and
nothing scopes it out of a run.

Two repairs hold up. Linking the translated heading's own slug stays correct in each
language under the renderer rules this page pins. Pinning an explicit raw-HTML `id` on
the heading survives translation entirely, since the `id` is harvested as an anchor
identity ([Resolution](resolution.md)) and does not move when the heading text does.
Either way the check is the same one every other page gets: the fragment must name an
identity the target actually publishes.

## What is not modelled

Renderers outside the table publish identities this check will not match, and a repository
served by one of them can see an anchor reported missing that its own site resolves. Pandoc,
Hugo's non-github id types, Sphinx and Docusaurus's custom slug functions are the known
cases. The fix for any of them is another row, derived and pinned the same way, since the
union only grows.
