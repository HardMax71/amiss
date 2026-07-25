# The controller ships as source, not as a crate

The engine is published to crates.io and has no network capability at all, which is checked
rather than claimed: a separate dependency policy bans HTTP clients, async runtimes, and socket
crates from the engine's graph, with reasons written into the file.

```toml
{ crate = "reqwest", reason = "the engine has no HTTP client" },
{ crate = "tokio", reason = "the engine has no async runtime and no sockets" },
{ crate = "socket2", reason = "the engine opens no sockets" },
```

The controller does have network capability, credentials, and provider tokens. Publishing it as
a convenient dependency would put all of that one `cargo add` away from anyone who wanted the
scanner, and would make the scanner's dependency graph the union of both. So the
[`controller/`](https://github.com/hardmax71/amiss/tree/main/controller) workspace stays unpublished and source-built. An operator who
wants a provider lane builds it from a commit they chose.

Inside, provider differences live in small crates rather than in a closed provider enum:
provider-neutral traits and the orchestrator, a bounded ingress gate, a rotating key ring,
signed-webhook checks, GitLab OIDC checks, and one adapter crate per provider family. A fourth
provider is a new crate, not a new arm in every match statement.

Introduced with the controller foundation in [#98](https://github.com/hardmax71/amiss/pull/98). Folded into a single workspace in
[#123](https://github.com/hardmax71/amiss/pull/123), which kept the two dependency graphs separate while removing the duplication
that came from two independent workspaces: 3,352 lines added against 4,474 removed.
