# One evaluation contract, not one per provider

Two problems shared one cause. The contract named provider-specific identity types, so
describing GitLab meant growing a second shape and a provider enum sat in the middle of the
trust boundary, where adding a provider means editing everything that matches on it. Separately,
one ref was doing two jobs: the ref used to resolve URLs and the protected branch that controls
apply to are different things, and conflating them means a check can verify one branch while
the merge rule guards another.

The rolling contract separates the source ref from the protected target ref, and a frozen
controller evaluation binds provider, integration, repository, URL dialect, change, refs,
commits, trees, provider gate, check plan, execution limits, and trusted time, none of which
requires knowing which provider is speaking. Providers differ in how those facts are obtained,
not in what the evaluation says.

The change was mostly deletion. Opening the execution-constraint identity took 291 lines added
against 169 removed; rolling the contracts forward removed 10,549 lines across 136 files while
adding 2,499. Forge-shaped variants had accumulated in the wire types, the schemas, the
examples, and the goldens, and most of the work was proving they were redundant rather than
writing something new.

Opened in [#57](https://github.com/hardmax71/amiss/pull/57) and rolled forward in [#58](https://github.com/hardmax71/amiss/pull/58). The types are
[`crates/amiss-wire/src/requests.rs`](https://github.com/hardmax71/amiss/blob/main/crates/amiss-wire/src/requests.rs) and the published
shape is [`spec/scanner-evaluation-request.schema.json`](https://github.com/hardmax71/amiss/blob/main/spec/scanner-evaluation-request.schema.json).
