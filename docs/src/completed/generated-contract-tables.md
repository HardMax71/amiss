# The book's contract tables are generated, not written

Before this closed, the book mixed four maturity levels on the same page: shipped scanner
behavior, the convenience Action, controller components that existed but were not wired to
anything, and research ideas. Several mechanical claims had also drifted from the constants
they described, which is the exact failure this repository sells a tool to catch.

Dispositions, resource ceilings, finding meanings, the refusal grammar, and the worked
examples each exist twice: once in the engine, once on a page. The pages therefore do not keep
their own copy. A contract test regenerates each table from the source of truth and compares:

- `documented_profiles_are_generated_from_the_policy_contract` rebuilds the disposition table
  in [Profiles and findings](../profiles.md) from the policy contract.
- `documented_limits_are_generated_from_runtime_constants` rebuilds every row of the ceiling
  table in [Limits and refusals](../limits.md) from the runtime constants.
- `documented_finding_meanings_are_generated_from_the_engine_text` and its error twin compare
  the meaning sentences on the page against the engine's own strings.
- `documented_grammar_matches_the_refusal_grammar` compares the usage block in
  [Invocation](../invocation.md) against the grammar the binary prints when it refuses.
- `documented_finding_examples_cover_the_report_schema` and
  `all_public_contract_examples_clear_their_schema_and_registered_reader` run every published
  example through the schema and the reader that ships, so an example cannot be aspirational.
- `the_llms_index_names_real_chapters_on_the_published_book` resolves every row of the agent
  index to a chapter file, so the index cannot advertise a page that was renamed away.

The generators are ordinary functions in the same file, `profile_table`, `limits_table`, and
`meanings_list`, so the test failure shows the expected table next to the one on the page
rather than an assertion that something differs.

A claim that can be generated is generated. A claim that cannot links the code that implements
it, which is why so much of the book is link-dense: the link is the check.

Aligned in [#46](https://github.com/hardmax71/amiss/pull/46), which also split the factual [Project status](../status.md) from the
forward-looking [Roadmap](../roadmap.md). Published examples became executable in
[#60](https://github.com/hardmax71/amiss/pull/60), and semantic vectors were enforced in [#62](https://github.com/hardmax71/amiss/pull/62). All of it lives in
[`crates/amiss/tests/documentation_contracts.rs`](https://github.com/hardmax71/amiss/blob/main/crates/amiss/tests/documentation_contracts.rs).
