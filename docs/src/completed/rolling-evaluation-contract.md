# One evaluation contract, not one per provider

A contract that names GitHub's identity types cannot describe GitLab without growing a second
shape, and a provider enum in the middle of a trust boundary is a place where adding a
provider means editing everything.

The rolling evaluation contract separates the source ref used to resolve URLs from the
protected target ref that branch controls apply to, which are different refs and were being
conflated. A frozen controller evaluation binds provider, integration, repository, URL
dialect, change, refs, commits, trees, provider gate, check plan, execution limits, and
trusted time, all without enumerating provider-specific identity types.

The wire types are [`crates/amiss-wire/src/requests.rs`](https://github.com/HardMax71/amiss/blob/main/crates/amiss-wire/src/requests.rs) with the published shape in
[`spec/scanner-evaluation-request.schema.json`](https://github.com/HardMax71/amiss/blob/main/spec/scanner-evaluation-request.schema.json). Opened in [#57](https://github.com/HardMax71/amiss/pull/57) and rolled
forward in [#58](https://github.com/HardMax71/amiss/pull/58).
