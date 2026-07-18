# The scan ledger

The validation phase runs on counted scans of other people's repositories, and this page
is where the counts live. One row is one scan: a public repository, a base and candidate
commit pair, the observe profile, a release build. Every number is read out of the run's
machine report or out of `git diff` over the same commit pair, never remembered.

Three column definitions are fixed before the numbers accumulate. Advisory rows are
findings whose effective disposition is `warn`; records are excluded because nothing asks
a person to look at them. Changed documentation lines are the added plus removed lines
`git diff --numstat` reports for Markdown files between the row's two commits. Reviewer
burden divides advisory rows by changed documentation lines and scales to one hundred.
The burden column is recorded, not gated: the [roadmap](roadmap.md) picks a threshold
only after ten rows exist on repositories that are not this one, and a row with fewer
than one hundred changed lines states its ratio without counting toward that choice,
because a small denominator makes the ratio noise.

## July 2026

Ten repositories, scanned 2026-07-18 with the v0.5.1 release build under
`--profile observe`, each from its latest release tag to that day's default-branch head.
Two bases bend that convention: ripgrep tags rarely, so its base is the 150th ancestor
of its head, and alacritty tags on release branches, so its base is the latest stable
tag's merge point with master.

| Repository | Range | References | Missing | Advisory | Doc lines | Per hundred | Rejection class |
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
small-denominator case the burden rule sets aside.

The three rows with a named rejection class map the adoption boundary, and none of their
missing counts is a resolver bug. mdBook's 36 all live inside its own link-handling test suite, deliberately
broken fixtures under `tests/testsuite`; its range changed no Markdown at all. starship's
242 are extensionless clean URLs its site router resolves and the tree does not,
concentrated in translation mirrors of the preset pages. ruff's 102 name targets its
docs build generates and the repository never holds, `settings.md` and `rules.md`
mostly, plus three literal template placeholders. Amiss reads every one of these
correctly against the tree; the maintainers would still close the report, and they would
be right to, which is what makes the class worth recording. These are the measured
adoption boundary behind the declared-generated-targets candidate on the
[roadmap](roadmap.md).

The four later rows were picked deliberately from repositories without a docs-site
generator, and they produced no rejection class at all: every nonzero count there is a
real break. bat's twelve are pre-existing and live in four translated READMEs whose
relative links carry the wrong prefix, `doc/LICENSE-MIT` for a root file and a doubled
`doc/doc/` for siblings, and each renders as a 404 on GitHub today. alacritty's one is
the same shape as helix's: a commit moved the escape-sequence docs into the manpage and
`docs/features.md` still links the deleted `escape_support.md`. fd and hyperfine were
spotless. On this evidence the rejection classes are a docs-site phenomenon; a plain
tree yields either zero or the genuinely broken.

## What a row must be

A row enters this page only from a recorded run: the machine report kept, the commit
pair stated, every column computed from those two artifacts, on a repository that is not
this one. The validation phase's exit criterion counts these rows; it needs ten or more
repositories, a false-missing count that stays at zero, and a burden threshold chosen
from the accumulated column rather than from taste.
