# Mutation and fuzz runs are installed with recorded baselines

Test suites that only ever run green say little about whether they would catch a regression.
Two non-gating runs answer that separately from the gates: a weekly mutation run and a nightly
coverage-guided fuzz run.

The first mutation run recorded 2,728 mutants with 664 missed on 2026-07-18. Excluding the
fixtures crate, which exists to be mutated by its own callers, the comparable baseline is
2,672 and 616. Both runs are deliberately non-gating: they inform, and a property is not
called stable until two months pass without an unexplained regression. Treating either as a
merge gate would convert a slow signal into a flaky one.

The schedules are [`.github/workflows/mutants.yml`](https://github.com/HardMax71/amiss/blob/main/.github/workflows/mutants.yml) weekly and
[`.github/workflows/fuzz-long.yml`](https://github.com/HardMax71/amiss/blob/main/.github/workflows/fuzz-long.yml) nightly. The fixtures crate was excluded in
[#83](https://github.com/HardMax71/amiss/pull/83), and the release-manifest laws the first run showed untested were covered in
[#87](https://github.com/HardMax71/amiss/pull/87).
