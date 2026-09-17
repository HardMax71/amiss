# A reason may grow

Closed September 2026. The wire promised that a report stays additive within a major, and a
closed enumeration broke that promise the moment it gained a value. A report carrying a
reason that did not exist in 0.29.1 was read by the released 0.29.1 binary as
`amiss render: the input is not a scanner report envelope`, exit 2, and the same report
without that value read fine. Three such values were already waiting in branches, and more
will arrive as documentation generators get modelled, so minting a major for each one was
never going to work. [The report](../report.md) stays the live chapter; this page records
what `3` changed and where the line fell.

The line runs between a reason and a verdict. A reason says why an answer could not be
given, and the answer sits beside it: a document row's `unsupported_reason` next to its
`status`, an `unsupported-semantics` row's `reason` next to its `kind`. A consumer that
meets an unfamiliar reason still knows the document went unscanned, or the reference went
unevaluated, so it loses nothing by keeping a spelling it cannot name. A kind, a
disposition, a status, a schema name and `compatibility` itself are the opposite. A
consumer that meets an unfamiliar one of those cannot judge the verdict at all, so they
stay closed and a reader still refuses them.

Two enumerations crossed. The document row's unsupported reason and the
unsupported-semantics reason are open strings under `3`. Both stay derive-only: strum
carries the closed spellings, and `#[strum(default, transparent)]` on a variant holding a
`String` gives the tolerant `FromStr` and the `Display` that writes it back, with nothing
written by hand. An unfamiliar reason therefore round-trips byte for byte, which is not a
nicety. The payload digest covers the reason, so a report the reader could not
re-spell exactly would fail its own seal. A contract test reads a report carrying an
invented reason, writes it back, and compares the bytes.

Making the semantics reason grow cost one shape. Under `2` each reason selected its own
body, so `query` and `code-fragment` carried a target, `fragment` carried a blob target,
and the four route reasons carried none. Under `3` the row is one shape for every reason,
the reason and the target it located when it located one, and a new reason needs no new
row. The engine still writes a target for `query`, `code-fragment` and `fragment` and for
nothing else, but the reader no longer refuses a target on a reason that would not carry
one. The `missing` reasons did not move, since each of those selects which evidence its row
carries and each feeds the finding key.

The mint itself followed `2`. The writers emit `3` from one wire constant, the schema pins
the same value by a contract test, and the example that opens the major is retained
permanently at
[`spec/examples/scanner-report.frozen-3.json`](https://github.com/HardMax71/amiss/blob/main/spec/examples/scanner-report.frozen-3.json),
byte-pinned like its two predecessors, with every later schema in the major required to
keep validating it. `frozen-2` stays in the tree as the record of major `2`, on the terms
`frozen-1` already had. The example the last release shipped keeps carrying `2` until the
release workflow refreshes it, and the contract test allows exactly that window. Every
reader of `3` refuses a `2` report: the render and refs verbs, the external plan, and the
controller. `3` is additive within its major as `2` was, and a reason can now grow without
minting `4`.
