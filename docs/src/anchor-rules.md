# What ten renderers call a heading

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

An anchor resolves when any of them would publish it, or when the document declares it
outright. Adding a rule can only grow that set, so a rule missing from the table is the
only way a live anchor is reported absent, and nothing a repository declares can shrink it.

Two of the rows are configurations rather than renderers. mdBook ships with smart
punctuation on and MkDocs takes its slug function from `mkdocs.yml`, so both spellings are
carried rather than one being chosen for the reader.

An identity can also be written down rather than derived. Raw HTML declares one with `id`
or `name`, and the `attr_list` extension declares one with an attribute block, in any of the
spellings it accepts: `{#id}`, `{ id="id" }`, `{ id=id }`, among classes, and with
kramdown's leading colon. A block whose last line is nothing but an attribute block declares
that identity for itself, which is how `[](){#anchor-point}` and a `{#section}` line under a
paragraph work; an attribute block trailing other text on the same line declares nothing,
and one inside a fence is code. The extension reads the block in the
document's own literal text, so a block inside inline code is code and declares nothing.
Every declared identity joins the union whatever the renderer, because it is authored
rather than derived, and accepting one a given renderer would not publish can only leave a
finding unreported, never invent one.

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
remaining three are transcribed from Gitea's `CleanValue`, Forgejo's `prefixedIDs`, and
mdBook's `id_from_content`.

Seven documents, in
[`corpus/third_party/anchor-fixtures/`](https://github.com/HardMax71/amiss/tree/main/corpus/third_party/anchor-fixtures),
carry what a renderer actually published for them, harvested 2026-07-26:

| Document | Renderer | Identities |
| --- | --- | ---: |
| `probe.md`, this repository's own | github.com file view | 28 |
| `probe.md` | mdbook 0.5.4, default configuration | 28 |
| `probe.md` | python-markdown 3.10 with `toc` and `attr_list` | 28 |
| `probe-html.md`, this repository's own | github.com file view | 9 |
| `probe-attr.md`, this repository's own | python-markdown 3.10 with `toc`, `attr_list` and `fenced_code` | 7 |
| `awesome-gitea.md`, CC0 | gitea.com | 50 |
| `starship-ja.md`, ISC | starship.rs, VitePress | 32 |

The github.com column comes from the file view, `/repos/{owner}/{repo}/contents/{path}`
under the HTML media type, which is the renderer that publishes heading anchors.
`POST /markdown` renders the same Markdown and publishes none, so a re-harvest through it
would come back empty rather than disagreeing.

The Gitea pair is the only live evidence for that rule and the only place its missing
duplicate suffix is visible: that one page publishes fifteen identities twice, so an anchor
into it is ambiguous on Gitea and unique on Forgejo, for the same file.

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

## What is not modelled

Renderers outside the table publish identities this check will not match, and a repository
served by one of them can see an anchor reported missing that its own site resolves. Pandoc,
Hugo's non-github id types, Sphinx and Docusaurus's custom slug functions are the known
cases. The fix for any of them is another row, derived and pinned the same way, since the
union only grows.
