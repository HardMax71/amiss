# Ten public repositories were scanned and the counts kept

A tool that reports missing references can be evaluated only against repositories it did not
grow up with. The validation phase scanned ten public repositories and retained every count,
so the adoption argument rests on recorded numbers rather than on impressions.

Four repositories came back spotless. Three carried only real breaks: one introduced in
helix, twelve pre-existing in bat, one pre-existing in alacritty. The remaining three mapped
systematic non-adoption classes, which is how declared generated targets became the largest
measured adoption blocker rather than a guess. Every row records reference and missing
counts, advisory rows, changed documentation lines, and the class of any finding a maintainer
would reject.

The rows and the contract for what may become a row are in
[The scan ledger](../ledger.md). Committed in [#82](https://github.com/HardMax71/amiss/pull/82) and grown to ten repositories in
[#86](https://github.com/HardMax71/amiss/pull/86).
