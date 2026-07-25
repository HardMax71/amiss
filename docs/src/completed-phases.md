# Completed phases

Four phases are closed. Each claim they made has its own page here: what the claim is, what it
defends against, how it is held, and where it landed. These are dated exit records rather than
live documentation, so a page states what was true when the phase closed and links the code
that has to stay true for it to hold. Current work is in the [Roadmap](roadmap.md), the
factual boundary of the product is in [Project status](status.md), and version history is in
the [changelog](https://github.com/HardMax71/amiss/blob/main/CHANGELOG.md).

## Validation and hardening

Closed July 2026. The engine was already written; this phase asked whether its claims survive
contact with repositories nobody here wrote.

- [The book's contract tables are generated, not written](completed/generated-contract-tables.md)
- [Embedded code cannot buy unbounded parse time](completed/bounded-embedded-code.md)
- [Ten public repositories were scanned and the counts kept](completed/shadow-scan-ledger.md)
- [A false missing target is a bug, not a statistic](completed/false-missing-is-a-bug.md)
- [Review feedback is grouped, ordered, and bounded](completed/grouped-review-feedback.md)
- [The self-scan runs the event shapes it claims to support](completed/self-scan-event-coverage.md)
- [Mutation and fuzz runs are installed with recorded baselines](completed/mutation-and-fuzz-baselines.md)

## Delivery record

Closed July 2026. A controller that publishes provider verdicts has to survive crashes,
retries, and clock movement without losing a verdict or writing two.

- [The delivery record has four states and no fifth](completed/delivery-record-contract.md)
- [Every accepted delivery carries a replay lifetime](completed/replay-lifetime.md)
- [The record is ordinary files, not a database](completed/file-ledger-implementation.md)
- [Lock growth is fixed and admission does not scan](completed/file-ledger-locks.md)
- [A row is one bounded state file and, briefly, a report](completed/bounded-row-files.md)
- [Cleanup removes only what is safe to forget](completed/record-cleanup.md)

## Provider-verified controls

Closed July 2026. The engine report is self-asserted, so the gate had to become an object the
provider owns and the checked repository cannot forge.

- [One evaluation contract, not one per provider](completed/rolling-evaluation-contract.md)
- [The bootstrap takes canonical documents and nothing else](completed/sealed-bootstrap-inputs.md)
- [The controller ships as source, not as a crate](completed/unpublished-controller-workspace.md)
- [Authenticate first, save the raw bytes, then acknowledge](completed/authenticated-ingress.md)
- [The record and the runner share one lease](completed/shared-lease-contract.md)
- [The runner seals the job it supervises](completed/sealed-runner.md)
- [The GitHub lane runs one repository end to end](completed/github-lane.md)
- [The GitHub source accepts four events and binds them](completed/github-event-source.md)
- [Objects are fetched by exact name under fixed limits](completed/exact-object-acquisition.md)
- [The verdict lands on the commit GitHub actually merges](completed/github-publication.md)
- [The GitLab lane runs as a policy job on the merge train](completed/gitlab-lane.md)
- [The GitLab gate refuses anything but the exact saved pass](completed/gitlab-merge-train-gate.md)
- [The Gitea family lane publishes through a dedicated reviewer](completed/gitea-lane.md)
- [The Gitea family gate is checked, not assumed](completed/gitea-reviewer-gate.md)
- [The lanes are tested through, and against, themselves](completed/lane-test-coverage.md)
- [Provider evidence lives in the provider, not in the report](completed/provider-owned-evidence.md)

## Provider operations

Closed July 2026. A lane that cannot be deployed, watched, or restarted without losing work is
not finished, whatever its verdicts say.

- [Every provider binary can check its configuration offline](completed/offline-configuration-check.md)
- [Liveness and readiness answer different questions](completed/liveness-and-readiness.md)
- [Ten counters, no labels, no cardinality surprise](completed/fixed-metric-set.md)
- [Shutdown finishes the work it already accepted](completed/graceful-drain.md)
- [Hostile provider input is tested without provider accounts](completed/account-free-robustness.md)
