# Authoritative semantic artifacts

Closed August 2026. External producers can know facts the repository tree cannot, but trusting only
their normalized answer would make a later report impossible to audit. This phase bound the
operator's expectation, the provider run, the exact acquired bytes, the candidate-bound envelope,
and the retained report into one replayable chain. It also proved that a language specialist can
enter that chain without putting a language parser in the engine or provider services. The live
contract is [Trusted semantic evidence](../semantic-evidence.md).

## The input survives the producer artifact

[#587](https://github.com/HardMax71/amiss/pull/587) records the exact candidate-independent template
and the canonical candidate-bound envelope, including their acquisition and digest identities.
[#588](https://github.com/HardMax71/amiss/pull/588) retains those bytes with the accepted report so
restart and retry never reacquire mutable output. [#589](https://github.com/HardMax71/amiss/pull/589)
adds authenticated retrieval and a provider-visible locator, allowing the audit lifetime to outlive
the workflow artifact that supplied it.

The repository does not choose what workflow is trusted. The immutable expectation from
[#590](https://github.com/HardMax71/amiss/pull/590) fixes provider, repository, workflow, event,
artifact and sole payload names, producer context, and archive and file limits. GitHub archive
decoding in [#591](https://github.com/HardMax71/amiss/pull/591) refuses traversal, links, duplicate
or extra members, and decompression growth. Exact run selection and bounded signed download arrive
in [#592](https://github.com/HardMax71/amiss/pull/592), service wiring in
[#593](https://github.com/HardMax71/amiss/pull/593), and authenticated completion scheduling in
[#594](https://github.com/HardMax71/amiss/pull/594). One scanner lease never polls an entire build.

## The first typed specialist stays outside the engine

The unpublished Rust specialist reads bounded, format-matched Rustdoc JSON and emits one complete
`record-set@1`; it invokes neither Cargo nor Rustdoc. [#595](https://github.com/HardMax71/amiss/pull/595)
normalizes root-crate public free functions, and
[#596](https://github.com/HardMax71/amiss/pull/596) adds inherent and trait declarations with
disjoint stable keys. Public aliases come from the maintained Rustdoc adapter. Numeric Rustdoc IDs
never become identities, and ambiguous specialized inherent functions are refused rather than
disambiguated by parsing rendered Rust syntax.

Real format-matched Rustdoc, Cargo feature boundaries, and host versus `wasm32-unknown-unknown`
target boundaries are pinned by [#597](https://github.com/HardMax71/amiss/pull/597),
[#598](https://github.com/HardMax71/amiss/pull/598), and
[#599](https://github.com/HardMax71/amiss/pull/599). A configuration is complete only for its exact
compiler, format, package, target, triple, features, cfg, and dependency digest. The generic
record-value and record-set projection contract attaches those declarations to visible docs in
[#600](https://github.com/HardMax71/amiss/pull/600); no Rust-specific scanner finding or evaluator
was added.

## Unsupported producers remain explicit

Only GitHub has a planned workflow-artifact acquisition path. Gitea/Forgejo and GitLab remain held
until an operator supplies a real artifact API, credential model, and workflow contract. A second
completed-site producer remains held for the same reason, so no speculative Sphinx/MkDocs assembly
was extracted from the proven mdBook path.

The Rust specialist deliberately stops at functions. The maintained dependencies do not yet expose
the larger public item surface as structured stable kind, path, and declaration rows from already
bounded bytes; Amiss will not parse another crate's rendered text or carry a fork to pretend that
gap is closed. Executable-example receipts and generated-diagram projections also remain demand
gated. Neither summary JUnit nor exact image bytes would by itself prove the semantic claim those
features are often assumed to make.
