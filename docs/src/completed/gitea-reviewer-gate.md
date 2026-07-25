# The Gitea family gate is checked, not assumed

An approval only gates a merge if the branch rule actually requires that approval and closes
every other way in. Those are separate facts, and the provider reports them separately.

The gate requires one approval restricted to the dedicated reviewer, closed direct-push and
bypass paths, stale and rejected review blocking, an up-to-date pull request, and
administrator enforcement. The adapter checks the distinct Gitea and Forgejo capability shapes
rather than guessing which forge it is talking to from headers, because the two report
overlapping fields with different meanings. Wildcard protection rules are supported through
effective-rule lookup, though one exact rule stays the easier setup to audit.

The checks are [`controller/gitea/src/live/refresh.rs`](https://github.com/HardMax71/amiss/blob/main/controller/gitea/src/live/refresh.rs). Completed in
[#107](https://github.com/HardMax71/amiss/pull/107); live instances later corrected the reviewer permission and the transient
mergeable state in [#131](https://github.com/HardMax71/amiss/pull/131).
