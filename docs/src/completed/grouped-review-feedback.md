# Review feedback is grouped, ordered, and bounded

An engine that posts one comment per finding turns a documentation check into noise, and
noise gets muted. Feedback is therefore engine-owned rather than left to whatever the calling
workflow improvises.

Related findings are grouped by the target they concern. Fixes come before Checks, so the
things that must change are read first. Existing findings never take a pull-request
annotation, because annotating code the author did not touch is how a check earns a mute. Scan
errors stay separate from findings. The command line and the Action show at most ten items,
and the overflow stays in the full report rather than being dropped.

The grouping is [`crates/amiss-scan/src/feedback.rs`](https://github.com/HardMax71/amiss/blob/main/crates/amiss-scan/src/feedback.rs), shipped in [#95](https://github.com/HardMax71/amiss/pull/95). The
annotation boundary is the `annotations` input in [`action.yml`](https://github.com/HardMax71/amiss/blob/main/action.yml), and the summary
behaviour landed in [#68](https://github.com/HardMax71/amiss/pull/68).
