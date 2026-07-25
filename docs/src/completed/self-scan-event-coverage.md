# The self-scan runs the event shapes it claims to support

The repository runs Amiss on itself, which is only evidence if the self-scan exercises the
event shapes real users hit. A gate that has only ever seen an ordinary push proves nothing
about a shallow checkout or a staged index.

Recorded runs cover push, same-repository pull request, depth-two shallow checkout, and the
staged-index path. The fork path uses the same unprivileged pull-request workflow rather than
a second privileged one. Fork and merge-group runs are not retained as phase gates: as of
July 2026 GitHub offers no merge queue to this public user-owned repository, so the
`merge_group` trigger and its event mapping stay in place for repositories that do have one,
untested here and stated as such.

The job is `self-scan` in [`.github/workflows/ci.yml`](https://github.com/HardMax71/amiss/blob/main/.github/workflows/ci.yml). History depth for the scan was
fixed in [#70](https://github.com/HardMax71/amiss/pull/70), the shallow and staged-index rows recorded in [#85](https://github.com/HardMax71/amiss/pull/85), and the
pull-request base derived from the merge commit itself in [#88](https://github.com/HardMax71/amiss/pull/88).
