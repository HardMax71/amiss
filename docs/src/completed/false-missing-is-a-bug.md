# A false missing target is a bug, not a statistic

A checker that reports references which actually resolve teaches maintainers to ignore it.
The failure is worse than a miss, because it spends the reviewer attention the tool exists to
protect.

So a false `explicit-target-missing` on a supported reference is not tracked as a rate. It is
a resolver defect, it gets a pinned test, and the accepted count of such defects is zero. The
distinction matters when reading the ledger: a nonzero missing count there is either a real
break or a non-adoption class named on the page, never a tolerated error margin.

The resolver cases are pinned in [`crates/amiss-scan/tests/resolve.rs`](https://github.com/HardMax71/amiss/blob/main/crates/amiss-scan/tests/resolve.rs), and the
rule that keeps ledger rows honest about which is which is in
[The scan ledger](../ledger.md).
