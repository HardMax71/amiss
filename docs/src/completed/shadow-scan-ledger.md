# Ten public repositories were scanned and the counts kept

A tool that reports missing references can only be evaluated against repositories it did not
grow up with. The first six such scans lived in a session scratchpad, one crash away from
gone, which made the adoption argument a memory rather than evidence.

They became a book page instead, [The scan ledger](../ledger.md), and then grew to ten. One
row is one scan: a public repository, a base and candidate commit pair, the observe profile, a
release build. Each row records the commit range, references extracted, missing count,
advisory rows, changed documentation lines, the historical density per hundred changed lines,
and the class of any finding a maintainer would reject. Every raw value comes from the kept
machine report or from `git diff --numstat` over the same commit pair. Derived columns are
derived from those two artifacts and never remembered.

The result splits three ways. Four repositories came back spotless. Three carried only real
breaks: one introduced in helix, twelve pre-existing in bat across four translated READMEs
whose relative links carry the wrong prefix, and one pre-existing in alacritty where the
escape-sequence docs moved into the manpage and `docs/features.md` still links the deleted
page. The remaining three mapped systematic non-adoption classes rather than defects, and that
is how declared generated targets became the largest measured adoption blocker in the
[Roadmap](../roadmap.md) instead of a hunch: ruff's `settings.md` alone accounts for 59
references, and most of starship's 242 missing rows are preset pages the site router resolves.

The page also fixed its own column definitions and a small-denominator rule before more rows
accumulated, because a density figure over nine changed lines is noise and would otherwise be
quoted as a finding. The rule that keeps a row honest is stated on the page as what a row must
be.

Committed in [#82](https://github.com/hardmax71/amiss/pull/82) with six rows, grown to ten in [#86](https://github.com/hardmax71/amiss/pull/86).
