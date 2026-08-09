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
candidate is a local commit whose only change is `.amiss/scanner-policy.json`, holding
674 document includes that bind the `rst` adapter to every `.txt` under `docs/`, and the
base is the unmodified upstream head. The whole tree read in 1.1 seconds on an ordinary
development machine.

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

## What a row must be

A row enters this page only from a recorded run: the machine report kept, the commit
pair stated, every raw value sourced from and every derived or classified column traceable
to those two artifacts, on a repository that is not this one. The validation phase used
the ledger to retain the ten-repository adoption and false-missing evidence; focused PR
feedback is now a separately tested product invariant.
