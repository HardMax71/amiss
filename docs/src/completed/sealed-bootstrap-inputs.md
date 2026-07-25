# The bootstrap takes canonical documents and nothing else

The bootstrap is the trusted edge: it is what a provider lane runs after acquiring objects,
and whatever it accepts is what the engine ends up believing.

It accepts the canonical evaluation, snapshot, and controls documents, checks their required
bindings, and hands their exact bytes to the verified engine in one closed input frame. Bytes
in, bytes out, with no reformatting step in between where a difference could hide. The same
wire library produces canonical execution limits and trusted-time statements, so the
documents a lane presents are produced by the code that validates them.

The reader is [`crates/amiss-bootstrap/`](https://github.com/HardMax71/amiss/tree/main/crates/amiss-bootstrap) and the documents are
[`spec/scanner-snapshot-request.schema.json`](https://github.com/HardMax71/amiss/blob/main/spec/scanner-snapshot-request.schema.json) and
[`spec/scanner-controls-request.schema.json`](https://github.com/HardMax71/amiss/blob/main/spec/scanner-controls-request.schema.json). The bootstrap crate ships from
[#1](https://github.com/HardMax71/amiss/pull/1); it learned these documents with the sealed
evaluation foundation in [#98](https://github.com/HardMax71/amiss/pull/98).
