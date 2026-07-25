# The verdict lands on the commit GitHub actually merges

A check attached to the head commit describes the branch. A merge queue merges something else,
the test-merge commit, and a status on that commit takes precedence over the head. Publishing to
the wrong one produces a green branch and an unchecked merge, which is the failure mode worth
the most care in the whole lane.

Publication attaches `success`, `failure`, or `cancelled` to GitHub's authoritative test-merge
commit. The summary binds the gate, provider run, refs, commits, trees, plan, execution
constraint, report digest, and a stable unavailable reason, so the Check Run says what was
evaluated rather than only how it ended. A reader who distrusts the verdict can reproduce the
inputs from the check itself.

Idempotency is honest about its limit. The evaluation ID reconciles one exact visible retry, so
an ordinary retry updates rather than duplicates. A create that GitHub accepted but whose reply
was lost can still leave a duplicate, because GitHub and the local ledger do not share a
transaction, and no amount of local bookkeeping fixes that. The page says so rather than
implying exactly-once.

A final pull-request refresh turns an out-of-order publication into a no-op once its staged
head, base, refs, or gate is no longer current, so slow work cannot write a stale verdict onto
a newer gate.

Live evidence for both directions, a pass and a refusal with the ruleset disabled, is in
[Retained provider runs](../provider-evidence.md). Completed in [#107](https://github.com/hardmax71/amiss/pull/107); publication is
[`controller/github/src/live/`](https://github.com/hardmax71/amiss/tree/main/controller/github/src/live).
