# Controls and policy

Two kinds of configuration can shape a run, and they carry opposite amounts of trust.

The repository policy is the one input read from the scanned tree itself, and it is
correspondingly weak. `.amiss/scanner-policy.json` can add directories to scan, list
protected paths whose removal is always a finding, and raise the disposition of
`explicit-target-missing`, `explicit-target-type-mismatch`, and `invalid-reference`. Raise
only: repository policy combines with the built-in profile by maximum, so it can promote an
observe warning to `fail` and can never downgrade or suppress it. An unknown
field makes the whole file invalid and the run incomplete, which is what keeps the policy
from growing into a plugin system one field at a time.

A `document` include names one exact path. A `tree` include names that path and descendants
separated by `/`; `specs` therefore covers `specs/api.md` but not `specs-old/api.md`. Matching
is bytewise, including for paths JSON cannot represent as text. Each snapshot policy can carry
the [published repository-policy entry ceiling](limits.md), so the base/candidate
classification union can contain twice that many distinct roots. The
[`policy` tests](https://github.com/HardMax71/amiss/blob/main/crates/amiss-scan/tests/policy.rs)
pin the semantic boundaries, and the release
[`eligibility` test](https://github.com/HardMax71/amiss/blob/main/crates/amiss-scan/tests/eligibility.rs)
checks the maximum union without scanning every policy row for every discovered path. The
`amiss-scan` `controls` benchmark tracks both tree matching and policy-set comparison as the
entry count grows.

External controls come from outside the repository, because anything stored inside it could
be rewritten by the very pull request under review. The contract defines five: an
organization floor (tightens ceilings and dispositions across many repositories), an
adoption debt snapshot (a recorded list of known failures being worked off), a waiver
bundle (time-limited permission to pass despite a named failure), trusted time, and an
execution constraint. The run identity itself accepts any grammar-valid declared forge
host, including enterprise and self-hosted instances. Bindings and effects are
control-specific: the current floor, debt, waiver, and trusted-time v1 documents remain
GitHub-scoped and match the scanned repository and branch exactly. The
[trusted-time v1 parser](https://github.com/HardMax71/amiss/blob/main/crates/amiss-wire/src/controls/trusted_time.rs)
pins the statement shape and TTL; verification also matches the candidate identity and
authenticated provider run. Debt must reproduce its adoption tree, and a waiver item for
another candidate tree is simply not selected.
The commit and staged-index paths share one
[external-control verification gate](https://github.com/HardMax71/amiss/blob/main/crates/amiss-scan/src/pipeline/external.rs).

Debt and waiver require verified trusted time and a complete Git candidate. Their item
schemas can name only `explicit-target-missing` or `explicit-target-type-mismatch`, and
selection uses an exact current finding key with a candidate fact. A resolved projection
or an absent key is not an exception target. Matching still requires the exact fact digest:
active unchanged debt records tolerance at `warn`, while an applied waiver changes only
`fail` to `warn`. Invalid, expired, worsened, or overlapping items suppress nothing, and
an overlap makes evaluation incomplete. The
[wrapper tests](https://github.com/HardMax71/amiss/blob/main/crates/amiss-scan/tests/wrapper.rs)
pin binding, trusted-time, expiry, fact-drift, wrong-tree selection, resolved-target, and
overlap behavior. The published [`complete-findings`, `debt-items`, and `waiver-items`
ceilings](limits.md) bound the accepted sets; the `amiss-scan` `pipeline` benchmark tracks
matching as findings and exception items grow.

One asymmetry follows from the control formats' own versioning: the report can carry a
finding on a document whose name is raw bytes, but the waiver and debt formats still spell
paths as text, so such a finding is reportable yet cannot be waived or adopted until those
formats revise.

In the public command and composite Action, all five external controls are absent. The
report records `status: "none"` separately for organization floor, debt snapshot, waiver
bundle, execution constraint, and trusted time; its sandbox assurance is `self-asserted`.
There is no aggregate `provider_verified` field. The exact projection is built in the
[report writer](https://github.com/HardMax71/amiss/blob/main/crates/amiss-scan/src/report.rs).

Strict request parsers and the five control semantics exist as internal library surfaces,
but authenticated provider acquisition and public CLI/bootstrap wiring do not. The public
[CLI shell](https://github.com/HardMax71/amiss/blob/main/crates/amiss/src/main.rs) supplies
each value as `None`. Until the delivery lane in [Project status](status.md) is complete, the
honest reading of a local or convenience-Action report is: these findings, under this
repository policy, with no outside authority consulted.

The control-plane finding family closes the loop from the other side. When a candidate
weakens its own policy file, shrinks what gets scanned, or edits the control configuration,
the comparison itself raises `policy-weakened`, `coverage-reduced`, or
`control-plane-changed`. Loosening the rules is reported under the rules being loosened.
