# Roadmap

This page tracks the work ahead: what is being done now, and what stays research. It is
not release notes or a promise that anything listed here will ship. The wire contract
froze at `1` in August 2026 and left this page; the record is in
[A settled wire](completed/a-settled-wire.md), and the frozen regime's law lives with
[The report](report.md). Coverage that has
landed is described where it works rather than here. The factual boundary of the current
product is in
[Project status](status.md), the exit evidence for phases already closed is in
[Completed phases](completed-phases.md), and version history is in the
[changelog](https://github.com/HardMax71/amiss/blob/main/CHANGELOG.md).

## Research, not committed work

Value claims shipped as the first evaluated kind: [Claims](claims.md) states the closed
grammar, and everything outside it keeps the unsupported-capability boundary. Typed
snippet, inventory, tree, graph, transcript, narrative, and external claims
remain research. Persistent acceptance records and governed review state reopen the
storage, concurrency, ownership, expiry, and cheapest-bypass problems the stateless
scanner avoids, the same problems that killed the ledger design in
[Provenance](provenance.md).

No claim kind becomes a milestone without design-partner demand, a proof-strength model,
evidence that reviewers find it useful, and experiments covering persistence and concurrent branches.
Until then these are design vocabulary, not advertised capability. Demand has a place to
land: open an issue on the repository naming the claim kind and the repository it would
gate, with one drifted example that reference checking cannot catch. The claim-demand
issue form asks for those three, and optionally what the claim should have
pinned. That register is what
this section reads before anything here becomes work.

The permanent boundaries stay in [What Amiss is not](non-goals.md): no semantic truth
verdicts about prose, no repository-executed hooks, no live-network validation inside the
engine, no automatic prose rewriting, and no repository-controlled weakening of a
required policy.

### Renderer-aware adjacent formats

The [measured MyST, Quarto, and Org yield](ledger.md#myst-quarto-and-org-yield-measured-and-held)
establishes useful repository-reference coverage, but does not make any of these formats
committed work. They remain three separate future tracks:

- A MyST adapter would be explicitly policy-bound because `.md` already means CommonMark/GFM.
  It must model directives, roles, labels, and transclusion context against a pinned renderer
  contract rather than reinterpret every Markdown document.
- A Quarto adapter would preserve the ordinary Markdown subset while modeling Quarto's
  main-document include context and static cross-references. Executed cells, notebook embeds,
  plugins, and renderer-produced declarations remain external evidence; `.qmd` is not a suffix
  alias for Markdown.
- An Org adapter would need lossless physical spans and bounded file-link, include, setup, and
  search syntax. Publishing roots and generated routes must arrive as sealed renderer evidence;
  the engine never evaluates Emacs Lisp or Babel, and oversized documents remain visible
  refusals rather than raising the global ceiling.

Admission still requires design-partner or provider demand and pinned conformance against the
official renderer. All three can use the existing physical byte-span location shape, so none is
reason to reopen the wire solely to name a source construct.

### Editor feedback remains held

The [editor-latency measurement](ledger.md#editor-latency-and-reverse-impact-demand-measured-and-held)
found that the reverse-impact query had not yet shipped, with no observable external use and no
editor-integration request.
Fresh-process startup is about 2.5 ms; complete scans range from 45 ms on Hyperfine to 4.7 seconds
on starship's translation mirrors. Keeping the engine resident would remove the negligible part
and retain the expensive one.

An LSP, worktree overlay, or incremental daemon is therefore research, not committed work. Reopen
it after a released `refs` command has a user who can identify a concrete author interaction and
repository where latency blocks them. Measure a stateless invocation there first. Any convenience
result remains observe-only and cannot satisfy a provider gate; persistent incremental state and
background network access need separate proof before they enter the design.
