# A false missing target is a bug, not a statistic

A checker that reports references which actually resolve teaches maintainers to ignore it, and
a muted check is worse than no check because it also consumed the attention it was meant to
protect. The usual industry answer is a false-positive rate. This project does not have one.

A false `explicit-target-missing` on a supported reference is a resolver defect. It gets a
pinned test and the accepted count is zero. That distinction is what makes
[The scan ledger](../ledger.md) readable: a nonzero missing count in a row is either a real
break or a non-adoption class named on the page, never a tolerated error margin, so nobody has
to guess which of the three a number belongs to.

Holding that line costs a large test surface, because every supported reference shape needs a
case. [`crates/amiss-scan/tests/resolve.rs`](https://github.com/hardmax71/amiss/blob/main/crates/amiss-scan/tests/resolve.rs) runs to
around 1,450 lines for that reason, covering component splitting in RFC order, line-selection
bounds as structural outcomes, LFS pointer targets, exact target digests, directories resolved
identically through a commit and through the index, paths compared as bytes with no case
folding and no normalization, and the GitHub, GitLab, and Gitea URL dialects each resolved
against the tree rather than pattern-matched.

The same rule is why a GitHub URL needs the whole trusted chain before it resolves, pinned by
`github_urls_need_the_whole_trusted_chain`: guessing that a URL belongs to this repository, and
reporting it missing when the guess is wrong, would be the same defect wearing a different hat.
