# The book's contract tables are generated, not written

Dispositions, ceilings, finding meanings, the refusal grammar, and the worked examples all
exist twice: once in the engine and once on a page. Two copies of the same fact drift, and
documentation drift is the thing this repository claims to catch, so the pages do not keep
their own copy.

A contract test regenerates each table from the policy contract and the runtime constants,
compares the meaning sentences against the engine's own strings, checks the grammar against
the refusal grammar, resolves every row of the agent index to a real chapter, and runs every
schema-backed example through the reader that ships. A claim that can be generated is
generated. A claim that cannot links the code that implements it instead.

The tests are `documented_profiles_are_generated_from_the_policy_contract`,
`documented_limits_are_generated_from_runtime_constants`,
`documented_grammar_matches_the_refusal_grammar`,
`documented_finding_examples_cover_the_report_schema`, and
`the_llms_index_names_real_chapters_on_the_published_book`, all in
[`crates/amiss/tests/documentation_contracts.rs`](https://github.com/HardMax71/amiss/blob/main/crates/amiss/tests/documentation_contracts.rs). They cover
[Profiles and findings](../profiles.md), [Limits and refusals](../limits.md),
[Invocation](../invocation.md), and the agent index. Aligned in [#46](https://github.com/HardMax71/amiss/pull/46), with published
examples executed from [#60](https://github.com/HardMax71/amiss/pull/60) and semantic vectors enforced from [#62](https://github.com/HardMax71/amiss/pull/62).
