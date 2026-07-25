# Mutation and fuzz runs are installed with recorded baselines

A suite that only ever runs green says little about whether it would catch a regression. Two
non-gating runs answer that question separately from the gates: a weekly mutation run, cron
`17 4 * * 1`, and a nightly coverage-guided fuzz run, cron `43 2 * * *`.

The first mutation run recorded 2,728 mutants with 664 missed on 2026-07-18. Excluding the
fixtures crate, which exists to be exercised by its callers and whose mutants say nothing about
the engine, the comparable baseline is 2,672 and 616. Those numbers are the point of recording
them: a later run that misses far more has either lost tests or gained untested code, and
without a baseline nobody can tell which.

The first run also paid for itself immediately. It showed the release-manifest laws untested,
and 323 lines of tests were added to cover them, which is the intended use of a mutation run:
not a score, a list of places where a lie would go unnoticed.

Both runs are deliberately non-gating. A weekly signal converted into a merge gate becomes a
flaky merge gate, and a fuzz run that must finish before a merge is a fuzz run that stops
looking hard. They inform instead, and no property here is called stable until two months pass
without an unexplained regression, which is a slower claim than most projects make and the only
one this evidence supports.

Installed and excluded from the fixtures crate in [#83](https://github.com/hardmax71/amiss/pull/83); the untested manifest laws the
first run exposed were covered in [#87](https://github.com/hardmax71/amiss/pull/87). The schedules are
[`.github/workflows/mutants.yml`](https://github.com/hardmax71/amiss/blob/main/.github/workflows/mutants.yml) and
[`.github/workflows/fuzz-long.yml`](https://github.com/hardmax71/amiss/blob/main/.github/workflows/fuzz-long.yml).
