# Fuzzing the dependency boundary

Every parser that consumes untrusted bytes has a harness here: strict JSON
with canonicalization round-trips, the six control parsers, the three
request parsers, both document adapters under the contract ceilings, the
index-file grammar, the commit and tree grammars, and the human atom
renderer. The bodies live in `src/lib.rs` with their invariants asserted;
the libFuzzer targets and the stable smoke test share them.

Committed seeds live in `seeds/<target>/`; the working corpus a long run
grows lives in `corpus/<target>/` and stays untracked. The per-change smoke
runs on stable and replays every seed plus a deterministic mutation sweep:

```
cd fuzz && cargo test --locked --release
```

The coverage-guided long runs need nightly and cargo-fuzz. Pass both
directories so the run starts from the seeds and accumulates into the
untracked corpus:

```
rustup toolchain install nightly --profile minimal
cargo install cargo-fuzz
cargo +nightly fuzz run <target> --features harness corpus/<target> seeds/<target>
```

Targets: `json`, `controls`, `requests`, `markdown`, `git_index`,
`git_objects`, `human`.

CI runs the smoke on every pull request and push to main. The fuzz-long
workflow runs every target for twenty minutes nightly under
AddressSanitizer, carries the corpus across runs in a cache, and uploads
any crash input as an artifact.

The markdown target neutralizes the default panic hook: the engine
classifies caught parser panics per contract, and the hook would otherwise
abort at panic time before the sanctioned catch runs. A panic escaping the
harness still aborts through the unwind boundary. Reproduced parser panics
and other findings belong in `seeds/<target>/` as regression seeds, which
the smoke then replays forever.
