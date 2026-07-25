# The controller ships as source, not as a crate

The engine is published to crates.io and has no network capability at all. The controller
does have network capability, so publishing it as a convenient dependency would put a
credential-handling HTTP client one `cargo add` away from anyone who wanted the scanner.

The [`controller/`](https://github.com/HardMax71/amiss/tree/main/controller) workspace stays unpublished and source-built. It supplies the
provider-neutral traits, the orchestrator, the bounded ingress gate, the rotating key ring,
the signed-webhook checks, the GitLab OIDC checks, and separate provider adapters. Provider
differences live in small crates rather than in a closed provider enum, so a fourth provider
is a new crate rather than a new arm in every match.

Introduced with the controller foundation in [#98](https://github.com/HardMax71/amiss/pull/98) and folded into a single workspace,
so the scanner's dependency graph stays separate from the controller's, in [#123](https://github.com/HardMax71/amiss/pull/123).
