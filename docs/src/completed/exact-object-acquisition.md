# Objects are fetched by exact name under fixed limits

Fetching a branch and trusting what arrives lets the remote choose what gets scanned. Fetching
without limits lets it choose how much memory the controller uses. Both are the same mistake:
letting the answer decide the question.

Acquisition speaks Git protocol v2 with exact authenticated SHA-1 wants for the repository
commit and the pinned action commit, so the remote answers a question rather than proposing an
answer. One deadline covers network receipt and validation together, because a deadline that
stops at the socket lets a slow validator hang after a fast download.

The pack limits are fixed constants, not configuration, and every one of them fails closed:

```rust
pack_bytes: 2_147_483_648,
objects: 2_000_000,
object_bytes: 134_217_728,
inflated_bytes: 4_294_967_296,
resolved_bytes: 4_294_967_296,
delta_depth: 128,
```

`REF_DELTA` is rejected outright, since a delta against an object outside the pack is a request
to resolve something the sender did not send. Pack indexing uses one thread, which trades
throughput for a bounded and reproducible cost profile.

Completed in [#107](https://github.com/hardmax71/amiss/pull/107). The protocol client is
[`controller/git/src/protocol.rs`](https://github.com/hardmax71/amiss/blob/main/controller/git/src/protocol.rs) and pack handling is
[`controller/git/src/pack.rs`](https://github.com/hardmax71/amiss/blob/main/controller/git/src/pack.rs).
