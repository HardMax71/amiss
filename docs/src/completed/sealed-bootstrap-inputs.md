# The bootstrap takes canonical documents and nothing else

The bootstrap is the trusted edge. A provider lane acquires objects, then hands them to this
binary, and whatever it accepts is what the engine ends up believing. Every format it tolerates
is a format an attacker may write.

It accepts three canonical documents, checks their required bindings, and passes their exact
bytes to the verified engine in one closed input frame: the evaluation, the snapshot, and the
controls. Bytes in, bytes out, with no reformatting step in between where a difference could
hide. The documents have published schemas rather than being an internal convention, so an
operator can validate what a lane will present before presenting it:

- [`spec/scanner-evaluation-request.schema.json`](https://github.com/hardmax71/amiss/blob/main/spec/scanner-evaluation-request.schema.json)
- [`spec/scanner-snapshot-request.schema.json`](https://github.com/hardmax71/amiss/blob/main/spec/scanner-snapshot-request.schema.json)
- [`spec/scanner-controls-request.schema.json`](https://github.com/hardmax71/amiss/blob/main/spec/scanner-controls-request.schema.json)

The same wire library produces canonical execution limits and trusted-time statements, so the
documents a lane presents come from the code that validates them rather than from a second
implementation that agrees until it does not.

The executable itself is bounded at 33,554,432 bytes:

```rust
pub const BOOTSTRAP_EXECUTABLE_BYTES: u64 = 33_554_432;
```

That bound is load-bearing in a way that only shows up in practice. A fixture binary that
linked one crate too many crossed it during this project's own lane testing and every run
refused with `Unavailable` rather than running an unbounded executable, which is the ceiling
doing its job on the person who set it.

The crate has shipped since [#1](https://github.com/hardmax71/amiss/pull/1); it learned these documents with the sealed evaluation
foundation in [#98](https://github.com/hardmax71/amiss/pull/98). It is
[`crates/amiss-bootstrap/`](https://github.com/hardmax71/amiss/tree/main/crates/amiss-bootstrap).
