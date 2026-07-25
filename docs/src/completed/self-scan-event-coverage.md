# The self-scan runs the event shapes it claims to support

This repository gates itself with its own action under `enforce`, which is only evidence if
the self-scan exercises the event shapes real users hit. A gate that has only ever seen an
ordinary push proves nothing about a shallow checkout, and the failure mode there is not a
crash: a scan with a truncated history silently compares against the wrong base.

Recorded runs cover push, same-repository pull request, depth-two shallow checkout, and the
staged-index path, the last of which runs `--base "$(git rev-parse HEAD^)" --index` against
the checkout's clean index on every CI run. Two corrections were needed to make those rows
mean anything. The self-scan first had to fetch full history, since a shallow clone gave it a
base it could not resolve. Then the pull-request base had to be derived from the merge commit
itself rather than from the event payload, because the payload's base is where the branch
started, not what the merge would actually compare against.

The fork path deliberately uses the same unprivileged pull-request workflow rather than a
second privileged one, so there is no second code path that only forks take and only forks can
break.

Fork and merge-group runs are not retained as phase gates, and the reason is recorded rather
than hidden: as of July 2026 GitHub offers no merge queue to this public, user-owned
repository. The `merge_group` trigger and its event mapping stay in place so a repository that
does have a queue is covered, and the workflow listens for the event so adopting a queue later
needs no ruleset surgery. That is a claim about readiness, not about testing, and the
distinction is the point.

The job is `self-scan` in [`.github/workflows/ci.yml`](https://github.com/hardmax71/amiss/blob/main/.github/workflows/ci.yml). History
depth fixed in [#70](https://github.com/hardmax71/amiss/pull/70), the shallow and staged-index rows recorded in [#85](https://github.com/hardmax71/amiss/pull/85), and
the base derived from the merge commit in [#88](https://github.com/hardmax71/amiss/pull/88).
