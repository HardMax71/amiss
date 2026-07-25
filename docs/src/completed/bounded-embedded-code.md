# Embedded code cannot buy unbounded parse time

The pinned MDX lexer has a quadratic case on unterminated regions, recorded in the
[corpus notes](https://github.com/HardMax71/amiss/blob/main/corpus/README.md). A document that opens regions and never closes them can make a
parser spend time out of proportion to its size, which is a denial-of-service shape rather
than a correctness bug.

The cost is charged where it is spent. Every candidate close adds the accumulated region to
the `aggregate-embedded-code-evaluation-bytes-per-snapshot` ceiling, so crossing it produces
an ordinary resource row rather than a hang, and the trip is pinned by test. Outside the
engine the convenience Action carries a wall-clock watchdog whose default of 120 seconds
matches the bootstrap lane's window, so a run that finds some other way to take too long
ends with no result rather than an unbounded wait.

The accounting is in [`crates/amiss-md/src/accounting.rs`](https://github.com/HardMax71/amiss/blob/main/crates/amiss-md/src/accounting.rs) and the ceiling is
`AggregateEmbeddedCodeEvaluationBytesPerSnapshot` in
[`crates/amiss-wire/src/controls.rs`](https://github.com/HardMax71/amiss/blob/main/crates/amiss-wire/src/controls.rs). The watchdog is the `watchdog-seconds` input
in [`action.yml`](https://github.com/HardMax71/amiss/blob/main/action.yml). Bounded in [#81](https://github.com/HardMax71/amiss/pull/81).
