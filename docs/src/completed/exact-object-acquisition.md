# Objects are fetched by exact name under fixed limits

Fetching a branch and trusting what arrives lets the remote decide what gets scanned. Fetching
without limits lets it decide how much memory the controller uses.

Acquisition speaks Git protocol v2 with exact authenticated SHA-1 wants for both the
repository commit and the pinned action commit, so the remote answers a question rather than
choosing an answer. One deadline covers network receipt and validation together. The limits
are fixed and fail closed: 2 GiB of pack, 2,000,000 objects, 128 MiB for any inflated stream
or resolved object, 4 GiB aggregate for each of inflated and resolved bytes, and delta depth
128. `REF_DELTA` is rejected outright and pack indexing uses one thread.

The protocol client is [`controller/git/src/protocol.rs`](https://github.com/HardMax71/amiss/blob/main/controller/git/src/protocol.rs) with pack handling in
[`controller/git/src/pack.rs`](https://github.com/HardMax71/amiss/blob/main/controller/git/src/pack.rs). Completed in [#107](https://github.com/HardMax71/amiss/pull/107).
