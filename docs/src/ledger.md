# The scan ledger

The completed validation phase used counted scans of other people's repositories, and
this page retains the counts. One row is one scan: a public repository, a base and
candidate commit pair, the observe profile, a release build. Raw values come from the
run's machine report or `git diff` over the same commit pair; historical density is
derived and rejection class is assigned from those recorded artifacts, never remembered.

These scans predate the grouped PR-feedback contract, so their row-level numbers remain
historical evidence rather than a product threshold. Advisory rows are findings whose
effective disposition was `warn`; records are excluded. Changed documentation lines are
the added plus removed lines `git diff --numstat` reports for Markdown files between the
row's two commits. The final numeric column is the old advisory-row density per hundred
changed lines. It is retained to reproduce the study, not interpreted as reviewer effort
or used as a gate; small denominators make it especially noisy.

## July 2026

Ten repositories, scanned 2026-07-18 with the v0.5.1 release build under
`--profile observe`, each from its latest release tag to that day's default-branch head.
That build resolved no heading anchors, so every reference counted here is a path or a line
range. The same ten trees later supplied the anchor measurement behind
[What twelve renderers call a heading](anchor-rules.md), which is a separate study and not a
row on this page.
Two bases bend that convention: ripgrep tags rarely, so its base is the 150th ancestor
of its head, and alacritty tags on release branches, so its base is the latest stable
tag's merge point with master.

| Repository | Range | References | Missing | Advisory | Doc lines | Historical density | Rejection class |
| --- | --- | ---: | ---: | ---: | ---: | ---: | --- |
| helix | `5cda70e86637..f6f3eb1fe4a7` | 3,249 | 1 | 47 | 2,166 | 2.2 | none |
| ripgrep | `a6e0be3c909c..227381db0ee8` | 766 | 0 | 6 | 214 | 2.8 | none |
| just | `2fd820433b02..e19eb9c379bc` | 3,101 | 0 | 1 | 9 | 11.1 | none |
| mdBook | `2ea30c00f006..69287f26827e` | 1,206 | 36 | 35 | 0 | undefined | test fixtures |
| starship | `fca92d8dcbd5..3c3aaf4f7ed2` | 7,508 | 242 | 844 | 84,485 | 1.0 | clean URLs |
| ruff | `0177a7e0d2c4..5055442b5875` | 5,244 | 102 | 102 | 1,146 | 8.9 | generated targets |
| bat | `979ba22628bc..78951393e29b` | 451 | 12 | 27 | 214 | 12.6 | none |
| fd | `7027d45303b4..1bfeea237a48` | 96 | 0 | 1 | 79 | 1.3 | none |
| hyperfine | `975fe108c4ee..f12f3d9f86f3` | 48 | 0 | 1 | 37 | 2.7 | none |
| alacritty | `a0be6eb8240c..852e971cddfa` | 87 | 1 | 5 | 65 | 7.7 | none |

helix's one missing reference was a real introduced break: a guide page linked
`./themes.md` where the page lives one directory up, invisible to mdBook's own build. A
community pull request
([helix-editor/helix#16034](https://github.com/helix-editor/helix/pull/16034)) was
already in flight with the identical one-character fix, which is independent confirmation
of the finding rather than a missed contribution. ripgrep and just were spotless on
missing references; just's single advisory row sits on a nine-line change, the
small-denominator case that shows why the historical ratio is not a product rule.

The three rows with a named rejection class map the adoption boundary, and none of their
missing counts is a resolver bug. mdBook's 36 all live inside its own link-handling test suite, deliberately
broken fixtures under `tests/testsuite`; its range changed no Markdown at all. starship's
242 are extensionless clean URLs its site router resolves and the tree does not,
concentrated in translation mirrors of the preset pages. ruff's 102 name targets its
docs build generates and the repository never holds, `settings.md` and `rules.md`
mostly, plus three literal template placeholders. Amiss reads every one of these
correctly against the tree; the maintainers would still close the report, and they would
be right to, which is what makes the class worth recording. These are the measured
adoption boundary that put declared generated targets on the roadmap, and the answer has
since shipped from the tracked ignore file, recorded in
[Reference coverage](completed/reference-coverage.md).

The four later rows were picked deliberately from repositories without a docs-site
generator, and they produced no rejection class at all: every nonzero count there is a
real break. bat's twelve are pre-existing and live in four translated READMEs whose
relative links carry the wrong prefix, `doc/LICENSE-MIT` for a root file and a doubled
`doc/doc/` for siblings, and each renders as a 404 on GitHub today. alacritty's one is
pre-existing in the recorded range: an earlier commit moved the escape-sequence docs
into the manpage and `docs/features.md` still links the deleted `escape_support.md`. fd
and hyperfine were spotless. On this evidence the rejection classes are a docs-site
phenomenon; a plain tree yields either zero or the genuinely broken.

## The same ten trees, rescanned

Heading anchors resolve since [What twelve renderers call a heading](anchor-rules.md), so the
ten trees were scanned again on 2026-07-26 with that work's release build. These are not
rows. Each is a whole-tree count at that day's head against a synthetic empty base, so there
is no commit range, no changed-line denominator, and no density figure. They are kept
because the class mix that [Reference coverage](completed/reference-coverage.md) later
answered moved.

| Repository | Head | References | Missing | Anchor | Other spelling | Absent |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| helix | `079a789e8cb0` | 3,249 | 10 | 9 | 0 | 1 |
| ripgrep | `f9c05a949d1a` | 766 | 0 | 0 | 0 | 0 |
| just | `af06bc49df4d` | 3,221 | 0 | 0 | 0 | 0 |
| mdBook | `4f8c9460977e` | 1,206 | 37 | 1 | 6 | 30 |
| starship | `eebb9a3c7ddc` | 7,509 | 344 | 103 | 241 | 0 |
| ruff | `a5cdc6d5813b` | 5,289 | 104 | 2 | 0 | 102 |
| bat | `78951393e29b` | 451 | 19 | 7 | 0 | 12 |
| fd | `ca51233d277e` | 96 | 1 | 1 | 0 | 0 |
| hyperfine | `f12f3d9f86f3` | 48 | 0 | 0 | 0 | 0 |
| alacritty | `852e971cddfa` | 87 | 1 | 0 | 0 | 1 |

Missing splits three ways, and the July study's two rejection classes are two of them.

247 name a target the tree holds under another spelling: `X` where `X.md` is present, or an
`.html` output name where the `.md` source is. All 241 of starship's are that, eleven preset
page names across twenty-two translations, and its `presets/README.md` links the same file
twice in one paragraph, once as `./plain-text.md` and once as `./plain-text`. mdBook's six
are its own output extension, inside its guide and its fixtures. These resolve now, against
[the spellings harvested from the routers themselves](route-spellings.md). Two columns of
this table read differently on the current build for that reason and one more: starship
reads 103 missing, and mdBook reads none at all, its whole count having been fixtures under
`tests/`, which [discovery](discovery.md) now skips by name.

146 name a target no spelling reaches, and 102 of those are ruff's generated pages, 63 of
them into `docs/settings.md`. That is the class a docs build writes and a tree never holds.

123 are heading anchors no rule publishes, which the July build could not see because it
resolved none. 122 are real breaks in five repositories: 103 in starship's translated pages,
where the heading was translated and the English fragment stayed, every one of them checked
against the rendered page on starship.rs and absent there; 9 in helix from one reference
definition whose section moved; 7 in bat from case and translation; 2 in ruff and 1 in fd
from changelog entries that moved out of the file. The remaining one is deliberate, inside
mdBook's link-handling fixtures.

Those 516 have since become 238, and the arithmetic closes: 247 resolve as
[router spellings](route-spellings.md), 30 left with the fixture trees
[discovery](discovery.md) now skips, and one was the defect below. What remains is 122 heading
anchors and 116 targets no spelling reaches, with mdBook joining the three that were already
clean.

The rescan also found one false missing, which is a defect and not a class: just's README
titles itself with `<h1 align=center><code>just</code></h1>`, github.com anchors that, and
the rule table did not. It got a
[pinned harvest and a fix](https://github.com/HardMax71/amiss/pull/140), which is why just
reads zero above.

## The Sphinx yield, measured

The v0.14 release taught the reStructuredText adapter the two Sphinx roles and the label
table behind `:ref:`. These are that work's counts, taken 2026-07-31 on two Sphinx-native
trees with the main build carrying the two lexer fixes below, engine
`sha256:5d1df7f8a4756f23aa7a78330ebc030438dc246f89cb06a7bb676062abc81c92`. Like the rescan
above these are whole-tree counts at that day's head, base one commit back, so there is no
range and no density figure. Django, the tree that motivated the adapter, was still
unmeasurable when these counts were taken: its reStructuredText lives in `.txt`, which the
built-in rows refuse. A policy include can now bind the `rst` adapter to exactly those
paths, stated in [discovery](discovery.md), and the measured row follows below.

| Repository | Head | Documents | References | Labels | Resolved | Duplicate | Inventory | Missing |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| pytest | `f306da747e70` | 305 | 1,408 | 460 | 449 | 0 | 6 | 5 |
| cpython | `ac8ba0ca5a04` | 1,307 | 4,704 | 3,366 | 3,365 | 1 | 0 | 0 |

CPython's Doc tree reads clean: every one of its 3,366 `:ref:` uses resolves through the
label table except a single genuine duplicate declaration. pytest's five label misses split
two ways: `package_env`, twice in its changelog, is a tox setting no pytest label can
satisfy and renders unresolved in its own Sphinx build, a real break found live; the other
three are prefixless references an intersphinx inventory satisfies at build time, which no
tree-only scanner can tell from drift, the boundary
[resolution](resolution.md) states. The six inventory rows are the colon-prefixed form,
declared unsupported rather than missing. The remaining missing counts in each tree's
summary, twenty and forty-two, are ordinary path references outside this study's question.

The measurement earned its keep the way the `just` defect above did, twice over. The
v0.14 build's first pass read 131 of pytest's labels as missing, and triage proved the
docs innocent both times: pytest declares 180 labels in the backtick-quoted phrase form
the lexer kept quotes on, and CPython's three "dead" labels were declarations sitting in
grid-table cells and a list item. Both false-missing classes got pinned fixes
([#203](https://github.com/HardMax71/amiss/pull/203),
[#205](https://github.com/HardMax71/amiss/pull/205)), which is why the table reads as it
does.

## The Django yield, measured

The binding shipped and Django stopped being the counterexample. These counts were taken
2026-08-10 at that day's default-branch head, on the main build carrying the three lexer
fixes below, engine
`sha256:3a7263e876ec5ccd55f3b4899f8189af4567b8b21051bec255d93ffba0257a34`. The method
bends one convention and states it: Django's tree carries no Amiss policy, so the
candidate is a local commit whose only change is `.amiss/scanner-policy.json`, then holding
674 document includes that bind the `rst` adapter to every `.txt` under `docs/`, and the
base is the unmodified upstream head. The whole tree read in 1.1 seconds on an ordinary
development machine. The exact commit tree was checked again on 2026-08-22: it still holds
674 regular `.txt` blobs and 66 differently suffixed blobs under `docs`. The current policy
grammar names the same measured set with one `{"path": "docs", "kind": "tree",
"suffix": ".txt", "adapter": "rst"}` selector without admitting those 66 other files.

| Repository | Head | Documents | References | Labels | Resolved | Duplicate | Inventory | Missing |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| django | `c9eb16a87e60` | 674 | 3,008 | 1,462 | 1,430 | 0 | 10 | 22 |

Every one of the 22 label misses is a reference an intersphinx inventory satisfies at
build time, into Python's own documentation (`old-string-formatting`,
`context-managers`, `tut-packages` and kin) or into Sphinx's generated `genindex` and
`modindex`, the same tree-only boundary [resolution](resolution.md) states and pytest's
row hit first. The ten inventory rows are the colon-prefixed intersphinx form, declared
unsupported rather than guessed. No label is declared twice.

The first pass read 43 rows as missing, and triage proved twelve of them innocent in
three classes, which became the lexer fixes this section's build carries: an indirect
hyperlink target (`.. _MySQL manual: MySQL_`, and the embedded `<Granian_>` form) is an
alias to another target, not a path; the `:file:` role is presentation markup that makes
no link, distinct from the `csv-table` option the extractor exists for; and a figure
ending `.*` is Sphinx's builder-resolved glob. Each shape now extracts nothing, pinned
one line at a time in the adapter's tests.

What remains after the fixes is nine path references that are genuinely dead in the
tree: relative directory links like `../middleware/` and `../settings/` in five pages,
`docs/topics/i18n/translation.txt` holding four, left from the documentation structure
Django retired when it moved to Sphinx, plus `../url_dispatch/` in the sitemaps
reference. Each names a route the current tree does not hold under
[the spellings a documentation router serves](route-spellings.md), which is the same
class as helix's one-character break: real, pre-existing, and invisible to a build that
never resolves them.

## The same ten trees, a third time

Scanned 2026-08-10 on the main build the Django row used, engine
`sha256:3a7263e876ec5ccd55f3b4899f8189af4567b8b21051bec255d93ffba0257a34`, each tree at
that day's default-branch head against the same synthetic empty base as the second pass,
depth-one clones. The wall column is new: one process, one scan, measured around the
whole invocation on an ordinary development machine. These are the book's first recorded
timings.

| Repository | Head | References | Missing | Anchor | Absent | Wall (ms) |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| helix | `079a789e8cb0` | 3,249 | 10 | 9 | 1 | 1,086 |
| ripgrep | `3fce3b5bb023` | 766 | 0 | 0 | 0 | 347 |
| just | `4f41f609278e` | 3,234 | 0 | 0 | 0 | 1,111 |
| mdBook | `b90df240a318` | 922 | 0 | 0 | 0 | 400 |
| starship | `545c0621a209` | 7,510 | 103 | 103 | 0 | 14,671 |
| ruff | `78cad66655dd` | 5,383 | 10 | 2 | 8 | 1,795 |
| bat | `2ba8db9c14e5` | 399 | 19 | 7 | 12 | 132 |
| fd | `ee20f426ddf3` | 96 | 1 | 1 | 0 | 64 |
| hyperfine | `f12f3d9f86f3` | 48 | 0 | 0 | 0 | 24 |
| alacritty | `1b2b36a64e88` | 87 | 0 | 0 | 0 | 57 |

The second pass's arithmetic predicted 122 real heading anchors, and the current build
measures exactly 122, in the same five repositories at the same counts: 103 in
starship's translated pages, 9 in helix, 7 in bat, 2 in ruff, 1 in fd. The absent class
fell from 116 to 21. The tracked-ignore answer emptied ruff's generated-target class
whole, alacritty's one break was fixed upstream between passes, and mdBook stays clean
at a newer head. Reference totals move with coverage and with the trees themselves:
mdBook's count drops because discovery now skips its fixture suites, bat's because its
documentation moved.

Every remaining absent row was read at its source line, and none is a resolver defect.
helix still carries the one-character `./themes.md` break, its community fix
([helix-editor/helix#16034](https://github.com/helix-editor/helix/pull/16034)) open
since July. bat's twelve are the translated-README 404s of the first study, unchanged.
ruff's eight split three ways: five angle-bracket placeholders in an agent-skill
template, one literal teaching example inside a changelog entry, and two broken relative
links inside `ty`'s markdown-based test fixtures under `resources/mdtest`, a fixture
tree the deliberately closed skip list does not name. A maintainer would close all
eight, and the rejection classes of the first study still describe them.

The timings say what the architecture promises: a scan is one pass over two snapshots,
so every plain tree answers in under two seconds, and Django's 674 bound documents in
the section above answered in 1.1. starship is the outlier its 7,510 references across
twenty-two translation mirrors earn, and even that finishes inside fifteen seconds.

Remeasured 2026-08-11 after the anchor lane learned to reuse discovery's own parse:
starship reads 5.7 seconds on the same tree, engine
`sha256:3423bdfa922c64f9c6ebc7be0fb6242ac1afb5d977b0504f7c0c273f8ac44fd0`. The removed cost was a second full parse of every
distinct anchor target; what remains is the parse-and-discovery baseline over the
mirrors' 17 MB, a recorded fact with a weekly bench now watching the mirror shape.

## Notebook Markdown yield, measured and held

The notebook question was measured on 2026-08-26 before admitting another document
format. Ten pinned trees supplied 3,878 exact lowercase `.ipynb` blobs and 461,907,733
bytes. Every tree with at most 120 notebooks supplied all of them; larger trees supplied
120 paths at evenly spaced indexes after lexicographic sorting, including the first and
last. That deterministic sample held 762 blobs and 104,270,321 raw bytes.

| Repository | Head | Notebooks | Empty | Over 4 MiB | Sample |
| --- | --- | ---: | ---: | ---: | ---: |
| openai/openai-cookbook | `a7c8782de788` | 271 | 0 | 4 | 120 |
| microsoft/ML-For-Beginners | `d0d0ea2b2d22` | 2,856 | 224 | 0 | 120 |
| jakevdp/PythonDataScienceHandbook | `d66231454ef7` | 136 | 0 | 0 | 120 |
| fastai/fastbook | `e8baa81d89f0` | 44 | 0 | 0 | 44 |
| tensorflow/docs | `35e0922e059d` | 188 | 0 | 0 | 120 |
| pandas-dev/pandas | `668be9d6d677` | 1 | 0 | 0 | 1 |
| matplotlib/matplotlib | `e519c449e932` | 3 | 0 | 0 | 3 |
| jupyter/notebook | `062a2e41d3d2` | 16 | 0 | 0 | 16 |
| anthropics/claude-cookbooks | `35f2eec7e448` | 98 | 0 | 1 | 98 |
| keras-team/keras-io | `7990430c3246` | 265 | 0 | 0 | 120 |

Fourteen sampled blobs were empty, all from the Microsoft tree. The other 748 were
nbformat 4.0 through 4.5 documents. Their Markdown `source` values were joined exactly as
the [notebook format](https://nbformat.readthedocs.io/en/stable/format_description.html)
requires and each cell was passed independently through the production Markdown
extractor. No code, output, attachment, kernel, or repository program was interpreted.
The extractor accepted all 15,245 cells.

| Repository | Valid sample | Markdown cells | With reference | With path-like | Path-like occurrences | All occurrences | Output share |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| openai/openai-cookbook | 120 | 2,041 | 111 | 52 | 211 | 1,006 | 40.4% |
| microsoft/ML-For-Beginners | 106 | 938 | 104 | 20 | 68 | 765 | 90.4% |
| jakevdp/PythonDataScienceHandbook | 120 | 3,000 | 116 | 110 | 926 | 1,711 | 93.0% |
| fastai/fastbook | 44 | 2,429 | 26 | 17 | 137 | 342 | 82.5% |
| tensorflow/docs | 120 | 3,366 | 120 | 58 | 272 | 2,366 | 7.8% |
| pandas-dev/pandas | 1 | 86 | 1 | 1 | 53 | 73 | 0.0% |
| matplotlib/matplotlib | 3 | 25 | 0 | 0 | 0 | 0 | 81.8% |
| jupyter/notebook | 16 | 149 | 7 | 3 | 11 | 49 | 4.6% |
| anthropics/claude-cookbooks | 98 | 1,202 | 66 | 32 | 109 | 502 | 73.4% |
| keras-team/keras-io | 120 | 2,009 | 119 | 13 | 38 | 1,100 | 5.1% |

`Path-like` is deliberately the lower-bound lexical class: no URI scheme, no leading
fragment, no `attachment:` scheme, and no network-path `//` prefix. It does not count a
same-repository forge URL which the resolver could answer. Even under that restriction,
306 of 748 valid notebooks held 1,825 candidates for the existing repository resolver.
The full extraction held 7,914 occurrences: 5,836 scheme-bearing or network-path
destinations, 231 cell-local fragments, and 22 attachments in addition to those 1,825.
The format has real coverage yield rather than merely category adjacency.

The cell bodies are small. The file row covers all 3,878 tree entries; the other two rows
cover the sample. Each percentile selects the sorted value at zero-based index
`round((n - 1) * p / 100)`, with half-way ranks rounded to even.

| Value | p50 | p95 | p99 | Maximum |
| --- | ---: | ---: | ---: | ---: |
| notebook bytes | 26,351 | 494,513 | 678,502 | 11,132,496 |
| Markdown-cell bytes | 221 | 1,504 | 2,785 | 38,276 |
| Markdown-cell lines | 3 | 19 | 41 | 569 |

Only five tree entries exceed the current 4 MiB document ceiling, and none exceeds 16
MiB. Size is not the reason to refuse the format. Materializing the notebook is: code-cell
`outputs` alone occupied 69,870,711 of the sample's 100,218,023 whitespace-free JSON bytes,
69.7 percent, before counting execution state or notebook metadata. Outputs were present
in 342 notebooks, exceeded half the bytes in 204, and exceeded 90 percent in 106. Five
more notebooks carried 15,230,286 bytes of Markdown attachments. A future reader must
skip those values while decoding rather than build an owned notebook tree or feed them
to the Markdown parser.

Location is the unsatisfied gate. Of 15,245 Markdown cells, 15,189 used an array source
and 56 one string; 104 of 78,157 array members themselves held more than one source line.
A decoded Markdown span can therefore cross JSON strings, quotes, commas, and escapes. It
is not one physical notebook byte span. Cell IDs do not rescue the current contract:
only 2,429 of 27,601 cells had one, and the sample contained one duplicate. An index plus
an optional valid ID and a cell-local span can name the source honestly, but the current
report location has only repository path and physical document span.

The provider UIs cannot consume a substitute. GitHub Check annotations require a path
and physical start/end lines, while its rich notebook diff still requires switching to
the raw source diff to comment on a line
([Checks API](https://docs.github.com/en/rest/checks/runs),
[notebook diff limitation](https://github.blog/changelog/2023-03-01-feature-preview-rich-jupyter-notebook-diffs/)).
GitLab transforms notebook diffs on commit and compare pages, explicitly not on merge
request pages, and offers no code suggestions for notebooks
([GitLab notebook diffs](https://docs.gitlab.com/user/project/repository/files/jupyter_notebooks/)).
A raw-JSON annotation would be clickable but would name the serialization rather than the
Markdown source the finding describes.

Notebook parsing is therefore held, not rejected. Shipping waits for a cell-aware report
location and at least one provider or design partner that can use it. That change must be
reviewed as a wire migration before the parser: cell source stays isolated, output and
metadata stay skipped, empty or malformed notebooks fail visibly, and a cell-local
finding is never disguised as a contiguous raw-JSON span.

## MyST, Quarto, and Org yield, measured and held

The three adjacent markup formats were measured on 2026-08-27 before admitting another
parser. Thirteen pinned trees supplied every exact lowercase source suffix outside the
nine built-in excluded directory names: `.md` in four MyST-family trees, `.qmd` in four
Quarto trees, and `.org` in five Org trees. There was no sampling. The resulting 1,559
documents held 22,015,633 bytes.

For MyST and Quarto, every document was first passed unchanged through the production
Markdown extractor. A second, source-positioned pass counted only the documented dialect
forms outside ordinary code fences and escaped examples: MyST directives and roles, and
exact Quarto shortcodes and cross-references. Triple-brace Quarto examples are escapes, not
shortcodes. Org bracket
links and keywords were read with lossless `orgize` 0.10.0-alpha.10 and cross-checked
against Pandoc 3.1.11.1. No code cell, Babel block, plugin, shortcode, included file, or
repository program was executed. A repository candidate was then resolved under the
format's documented relative-path rule against the pinned Git tree; generated output and
renderer context were never guessed into existence.

### MyST

| Repository | Head | Documents | Markdown references | Markdown path-like | Dialect repo targets | Existing | Internal roles |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| jupyter-book/mystmd | `d41a821e2244` | 312 | 1,343 | 255 | 129 | 126 | 728 |
| jupyter-book/jupyter-book | `fc05697264cc` | 42 | 857 | 39 | 5 | 5 | 2 |
| canonical/lxd | `44a4c0ded139` | 235 | 966 | 348 | 254 | 253 | 1,344 |
| pyOpenSci/python-package-guide | `10277117d7bd` | 53 | 1,118 | 253 | 35 | 35 | 0 |

These files are already discovered as Markdown, and the production adapter accepted all
642. It extracted 4,284 ordinary references, including 895 path-like repository
candidates. That is genuine existing coverage, but not MyST coverage. The dialect pass
found another 423 repository candidates in `{include}`, `{literalinclude}`, `{image}` and
`{figure}` directives and `{doc}` and `{download}` roles; 419 name an entry under the
documented source-relative spelling. It also found 2,074 internal role uses, predominantly
2,069 `{ref}`, `{numref}`, and `{eq}` uses whose label and inventory semantics CommonMark
does not have. Twenty-one dialect file targets are external.

The four file targets a plain source-relative lookup did not find are the useful boundary,
not four defects. Three are deliberate `my-file` teaching values in MyST's own reference
guide. LXD's root `CONTRIBUTING.md` supplies the fourth: `doc/contributing.md` includes it,
and its `{doc}` role reaches `doc/debugging.md` in the including Sphinx document's context.
The [MyST include contract](https://mystmd.org/guide/directives) makes the include path
relative to its source, while roles and extension points still require the parsed document
context. A suffix alias cannot reproduce that distinction.

### Quarto

| Repository | Head | Documents | Markdown references | Markdown path-like | Includes | Cross-references | Executable cells |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| quarto-dev/quarto-web | `db4c9fc6a00e` | 578 | 3,973 | 1,974 | 376 | 61 | 322 |
| ropensci-books/targets | `1d14652363c1` | 20 | 643 | 11 | 0 | 2 | 261 |
| r-universe-org/docs | `f5e288ed1e7e` | 25 | 199 | 31 | 0 | 0 | 15 |
| nasa/ECOSTRESS-Data-Resources | `d9155bc369ac` | 3 | 14 | 1 | 0 | 0 | 0 |

All 626 `.qmd` documents were valid input to the production Markdown adapter, which found
4,829 references and 2,017 path-like repository candidates. Amiss currently sees none of
them because `.qmd` is outside the document set. The Quarto-only pass found 376 exact
`include` shortcode targets, every one present in the pinned tree, and 63 cross-reference
uses. Initial YAML metadata held 35 file-bearing fields which expanded to 22 concrete
bibliography, resource, or body-include values.

That large safe-looking Markdown subset still does not justify binding `.qmd` to the
Markdown adapter. Quarto's [include contract](https://quarto.org/docs/authoring/includes.html)
preprocesses an include even inside a code fence and resolves references in the inserted
text from the main document rather than from the included file. The same corpus held 598
executable or diagram cells and 452 other renderer shortcodes, including 18 notebook
embeds. Cross-reference declarations can also be produced by executed cells. Those are
renderer evidence, not repository syntax the engine may simulate.

### Org

| Repository | Head | Documents | Bracket links | Relative files | Internal | Include/setup | Existing | Plain-tree miss | Source blocks |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| bzg/org-mode | `76e4dbe07f93` | 45 | 486 | 4 | 370 | 7 | 11 | 0 | 274 |
| org-roam/org-roam | `903bd4ec56d2` | 1 | 60 | 3 | 10 | 0 | 3 | 0 | 59 |
| tecosaur/orgmode.org | `770916ecb648` | 16 | 212 | 54 | 0 | 32 | 64 | 22 | 25 |
| SystemCrafters/systemcrafters.github.io | `6ccb1aa279f7` | 186 | 942 | 192 | 0 | 0 | 82 | 110 | 752 |
| caiorss/C-Cpp-Notes | `98ccfc4b0858` | 43 | 7,870 | 271 | 0 | 39 | 267 | 43 | 4,284 |

The lossless pass found 9,570 double-bracket links: 524 relative file links, 380 internal
targets, 8,644 external schemes, one absolute file, and 21 custom or fuzzy shapes. Sixty-one
`INCLUDE` and seventeen `SETUPFILE` keywords raise the explicit repository-candidate total
to 602. Of those, 427 reach an entry by the plain source-relative rule and 175 do not.
Pandoc produced 12,628 link or image nodes because its Org reader also recognizes plain
external URLs; those add no same-repository coverage and were not substituted for the
lossless source counts.

The misses split along renderer boundaries. `orgmode.org` links generated manual, guide,
PDF, and HTML outputs absent from the source tree. System Crafters' committed
`live-streams.org` is a generated sitemap whose sibling-looking links resolve from the
`content/live-streams` publishing project, not from the file's physical `content` parent.
The C++ notes repeatedly include an absent `theme/style.org`, while two encoded file names
need Org's decoding rule. The [Org file-link contract](https://orgmode.org/manual/External-Links.html),
[include grammar](https://orgmode.org/manual/Include-Files.html), and
[HTML export rewrite](https://orgmode.org/manual/Links-in-HTML-export.html) are separate
semantics; recognizing brackets alone would turn build context into false missing rows.

One Org document, `Rosetta_Stone_Translation.org`, is 7,154,544 bytes and exceeds the
default 4 MiB document ceiling. It carries 2,189 bracket links, three repository
candidates, and thirteen source blocks. The other 290 Org documents fit the current byte
limit. The size crossing remains a visible per-document refusal and is not a reason to
raise the built-in per-document ceiling for a new parser.

### Admission decision

The measurement establishes real yield but admits none of the three formats yet. MyST
needs a distinct, policy-bindable grammar for roles, directives, labels, and transclusion
context; the production dependency graph has no pure-Rust MyST parser pinned to that
contract. Quarto needs a dedicated adapter which preserves the measured Markdown subset
while representing includes and refusing execution- or plugin-derived targets; calling it
Markdown would overstate coverage. Org needs a bounded parser pinned against the GNU Org
renderer plus explicit publishing-context evidence; the pure-Rust parser used for
measurement is still an alpha, not the production contract.

Unlike notebooks, all three formats can identify their authored constructs with physical
byte spans in the source file. None needs a new location shape or wire v2; an admitted
adapter would still move the normal additive schema and examples. What is missing is
renderer conformance and, for Quarto and Org publishing, trustworthy build context.
Shipping waits for that evidence and a provider or design partner that needs the format;
executable outputs and plugins remain external evidence in every design.

## What a row must be

A row enters this page only from a recorded run: the machine report kept, the commit
pair stated, every raw value sourced from and every derived or classified column traceable
to those two artifacts, on a repository that is not this one. The validation phase used
the ledger to retain the ten-repository adoption and false-missing evidence; focused PR
feedback is now a separately tested product invariant.
