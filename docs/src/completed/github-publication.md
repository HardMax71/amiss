# The verdict lands on the commit GitHub actually merges

A check attached to the head commit describes the branch. A merge queue merges something else:
the test-merge commit. Publishing to the wrong one produces a green branch and an unchecked
merge.

Publication attaches `success`, `failure`, or `cancelled` to GitHub's authoritative test-merge
commit. The summary binds the gate, provider run, refs, commits, trees, plan, execution
constraint, report digest, and a stable unavailable reason, so the Check Run states what was
evaluated rather than only how it ended. The evaluation ID reconciles one exact visible retry.
A create that GitHub accepted but whose reply was lost can still leave a duplicate, because
GitHub and the local ledger do not share a transaction, and that is stated rather than papered
over. A final pull-request refresh turns an out-of-order publication into a no-op once its
staged head, base, refs, or gate is no longer current.

Publication is [`controller/github/src/live/`](https://github.com/HardMax71/amiss/tree/main/controller/github/src/live). Completed in [#107](https://github.com/HardMax71/amiss/pull/107).
