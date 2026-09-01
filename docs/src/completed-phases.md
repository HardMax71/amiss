# Completed phases

Eleven phases are closed, one page each. A page is a dated exit record rather than live documentation:
it states what was true when the phase closed, what each claim defends against, and links the code
that has to stay true for the claim to hold. Where a fact has moved on since, the page says so and
points at the live chapter that owns it.

Current work is in the [Roadmap](roadmap.md), the factual boundary of the product is in
[Project status](status.md), and version history is in the
[changelog](https://github.com/HardMax71/amiss/blob/main/CHANGELOG.md).

[Validation and hardening](completed/validation-and-hardening.md) asked whether the engine's claims
survive contact with repositories nobody here wrote. Generated contract tables, a bound on embedded
code, ten public repositories scanned and kept, no false-positive rate, one reviewer projection, the
event shapes the self-scan actually runs, and the first mutation and fuzz baselines.

[Delivery record](completed/delivery-record.md) made a controller that publishes provider verdicts
survive crashes, retries, and clock movement without losing a verdict or writing two. One atomic
claim, a fenced lease shared with the runner, an authenticated replay lifetime, and a durable record
built from ordinary files with fixed lock growth.

[Provider-verified controls](completed/provider-verified-controls.md) turned the gate into an object
the provider owns and the checked repository cannot forge. One evaluation contract, a sealed bootstrap
and runner, exact object acquisition, and three provider lanes with their gates checked rather than
assumed.

[Provider operations](completed/provider-operations.md) made a lane deployable, watchable, and
restartable without losing work. Offline configuration checks, separate liveness and readiness, ten
label-free counters, a graceful drain, and account-free robustness testing.

[Reference coverage](completed/reference-coverage.md) answered the four classes the scan ledger had
measured and named, and refused a fifth. Heading anchors under twelve renderer rules, router
spellings, generated targets read from the repository's own declarations, AsciiDoc and
reStructuredText, and the measurement that killed bare-path inference.

[Live provider evidence](completed/live-provider-evidence.md) replaced fixture belief with provider
verdicts on every lane. A positive and a revoked-control pair per lane, candidate content fixed so
one control moves per flip, github.com and gitlab.com and self-hosted Gitea and Forgejo, and six
live-found defects that every fixture had agreed with.

[A settled wire](completed/a-settled-wire.md) froze the report contract at `1` once its three
conditions held at once. The wire versioned by its own in-payload field, engine releases decoupled,
two minor series quiet under a mechanical tripwire, and the first frozen example retained
permanently to hold every later schema in the major to the additive promise.

[Projection contracts](completed/projection-contracts.md) made exact code, inventory, count, and
record-set relationships policy-owned scanner facts. Stable identities, complete-input semantics,
bounded difference previews, independent resource meters, and local authoring that never promotes
self-asserted data into provider authority.

[Authoritative semantic artifacts](completed/authoritative-semantic-artifacts.md) carried external
producer bytes across the provider trust boundary and beyond provider retention. An immutable
workflow-artifact plan, exact GitHub acquisition, restart-safe audit artifacts, and one isolated
Rustdoc normalizer whose deliberately narrow completeness claim is attached through the generic
projection contract.

[Offline audit sidecars](completed/offline-audit-sidecars.md) separated publication and locale facts
from the scanner report without weakening either. Closed plan, evidence, and assessment contracts,
exact report and product bindings, durable publication replay, page coverage, fallback provenance,
and source lineage—with live deployment and locale intake still stated as operator-gated work.

[Cross-repository relation core](completed/cross-repository-relation-core.md) proved that one
operator-owned relation can bind four exact snapshots, survive supersession and restart, and stage
idempotent provider outcomes without letting either repository select the other. The provider-neutral
core and provider boundaries closed; installation in the live provider binaries did not.
