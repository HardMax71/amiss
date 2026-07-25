# The Gitea family gate is checked, not assumed

An approval gates a merge only if the branch rule actually requires that approval and closes
every other way in. Those are separate facts, reported separately, and either one missing makes
the approval decorative.

The gate requires one approval restricted to the dedicated reviewer, closed direct-push and
bypass paths, stale and rejected review blocking, an up-to-date pull request, and administrator
enforcement. The adapter checks the distinct Gitea and Forgejo capability shapes rather than
guessing which forge it is talking to from headers, because the two report overlapping fields
with different meanings and a wrong guess produces a confident wrong answer. Wildcard protection
rules work through effective-rule lookup, though one exact rule stays easier to audit.

Two facts about this only surfaced against live instances. Reading the rule needs repository
administrator access, not write: below that, `/branch_protections/{rule}` answers `403` and the
branch route leaves `effective_branch_protection_name` empty, so the lane cannot read the rule
it is required to check. The documentation said write access, which cannot work. And the gate is
verifiable in both directions now: with the rule intact the lane approves, with direct push
re-enabled it publishes `unavailable / authorization-revoked`, and restoring the rule returns it
to approving the same content.

Completed in [#107](https://github.com/hardmax71/amiss/pull/107); the reviewer permission and the transient mergeable state were
corrected in [#131](https://github.com/hardmax71/amiss/pull/131). The checks are
[`controller/gitea/src/live/refresh.rs`](https://github.com/hardmax71/amiss/blob/main/controller/gitea/src/live/refresh.rs), and the
revoked-control runs are in [Retained provider runs](../provider-evidence.md).
