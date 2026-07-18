# Roadmap

This page contains future work, not release notes or a promise that every candidate will
ship. The factual boundary of the current product is in [Project status](status.md), and
completed changes move to the
[changelog](https://github.com/HardMax71/amiss/blob/main/CHANGELOG.md) instead of being
repeated here.

## Now: validate and harden

Stronger enforcement should follow evidence that the existing scanner is accurate,
predictable, and maintainable outside its own repository.

- Keep the book, active schemas, canonical examples, and released behavior aligned. Prefer
  generated contract blocks and links to implementation artifacts where a claim is
  mechanical.
- Bound parser CPU work while parsing, or narrow the published runtime guarantee to the
  resources the engine can currently measure. Decide whether the convenience Action needs
  an independent wall-clock watchdog.
- Run shadow scans on unrelated repositories and record denominators, false positives,
  clean-page false negatives, reviewer actionability, latency, and maintenance cost.
- Exercise pull-request, push, merge-group, fork, shallow-checkout, and staged-index paths in
  their real hosting environments.
- Accumulate benchmark, fuzz, mutation, security, and compatibility results long enough to
  distinguish a trend from a single green run.

The exit bar for this phase: supported-reference accuracy and reviewer burden have
recorded thresholds; hostile maximum-size inputs meet a truthful published runtime
contract; schema-backed input examples validate against their schemas and typed readers;
canonical report bytes clear the wrapper acceptance path; and every supported CI event
has an end-to-end fixture or recorded run.

## Next: provider-verified controls

This milestone is conditional on the validation phase. Internal request parsers, control
semantics, and bootstrap supervision already exist; their exact maturity is recorded in
[Project status](status.md). The remaining product work is integration and an independent
trust boundary:

- acquire and authenticate provider-created evaluation and control requests;
- implement provider adapters that translate independently authenticated run context into
  the forge-neutral request contract; the GitHub composite Action is currently only a
  convenience adapter, and GitLab, Gitea-family, and other provider-authenticated adapters
  are unsupported;
- feed the authenticated request into the engine instead of constructing an all-absent
  control shell in the CLI;
- include and invoke `amiss-bootstrap` in the protected required-check path;
- define trust anchors, freshness, revocation, and replay behavior;
- cover wrong identity, wrong tree, expiry, replay, missing output, timeout, and tampered
  runtime closure in end-to-end negative tests.

The lane is ready only when the verifier is acquired independently of the action tree it
checks, every authorization binds the exact repository and tree, and a report distinguishes
verified provenance from local self-assertion without relying on repository-controlled
input.

## Reference-coverage candidates

These are candidates, not scheduled milestones. Each enters the roadmap only after observed
demand and a pinned semantic contract exist.

- Heading anchors require a pinned slugging corpus for each supported renderer. Checking
  only the file while guessing the anchor would create false passes.
- reStructuredText or AsciiDoc requires a pinned grammar, conformance corpus, extraction
  goldens, resource accounting, and honest opaque regions.
- Bare-path inference remains advisory research until measured precision is high enough to
  justify the ambiguity and reviewer load it introduces.

## Research, not committed work

Typed snippet, value, inventory, tree, graph, transcript, narrative, and external claims
remain research. Persistent acceptance records and governed review state reopen storage,
concurrency, ownership, expiry, and cheapest-bypass problems that the stateless scanner
avoids.

No claim kind becomes a milestone without design-partner demand, a proof-strength model,
measured review burden, and experiments covering persistence and concurrent branches. Until
then these are design vocabulary, not advertised capability.

The permanent boundaries remain in [What Amiss is not](non-goals.md): no semantic truth
verdicts about prose, no repository-executed hooks, no live-network validation inside the
engine, no automatic prose rewriting, and no repository-controlled weakening of a required
policy.
