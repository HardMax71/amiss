# Review feedback is grouped, ordered, and bounded

Before this, consumers reshaped raw findings themselves. The command line did it one way, the
Action another. That leaked the engine's internal taxonomy into review, duplicated the
classification logic in two places that could disagree, and let harmless inventory rows crowd
out the two lines a reviewer needed to read.

The engine now owns one deterministic reviewer projection. `feedback` groups review work by the
target it concerns and classifies each item as Fix, Check, or Existing. Classification derives
from correlation, attribution, and location metadata rather than from a match over
`FindingKind`, so adding a finding kind does not mean editing the reviewer's view to keep it
sensible.

The ordering and the caps are the part that matters in practice. Fixes come before Checks, so
what must change is read first. Existing findings never take a pull-request annotation, because
annotating code the author did not touch is precisely how a check earns a mute. Scan errors
stay separate from findings, since "the run did not complete" is a different statement from
"this reference is broken". The human and Action views cap at ten combined items, and only
candidate-located displayed Fixes become annotations. Nothing is dropped: every exact finding
stays in the JSON report, which is the artifact for tooling, while the reviewer view is for
people.

An incomplete run reports feedback as explicitly unavailable rather than as an empty list,
because an empty list reads as "nothing to do" and that would be a lie about a run that failed.

Shipped in [#95](https://github.com/hardmax71/amiss/pull/95), which also made removed references recorded facts, and rendered in
[`crates/amiss-scan/src/feedback.rs`](https://github.com/hardmax71/amiss/blob/main/crates/amiss-scan/src/feedback.rs). The annotation
boundary is the `annotations` input in [`action.yml`](https://github.com/hardmax71/amiss/blob/main/action.yml); the summary behavior and
annotation flooding were addressed in [#68](https://github.com/hardmax71/amiss/pull/68).
