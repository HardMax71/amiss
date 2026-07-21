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
execution constraint.

Every control identity, and the release manifest's, uses one open repository grammar: a
caller-canonical host, a slash-joined owner when the forge supports nested groups, and a
repository name. That admits enterprise and self-hosted instances without making them
impersonate a public host. In the evaluation request, `candidate_ref` is the candidate or
source branch used to recognize same-repository links; `target_ref` is the protected branch to
which the organization floor, trusted time, debt snapshot, and waiver bundle bind. They are
equal for an ordinary branch update but may differ for a pull or merge request.
`default_branch_ref` remains URL-resolution context and does not stand in for the protected
target. The
[organization-floor](https://github.com/HardMax71/amiss/blob/main/spec/organization-floor.schema.json),
[debt-snapshot](https://github.com/HardMax71/amiss/blob/main/spec/debt-snapshot.schema.json), and
[waiver-bundle](https://github.com/HardMax71/amiss/blob/main/spec/waiver-bundle.schema.json) schemas, the
[control parsers](https://github.com/HardMax71/amiss/blob/main/crates/amiss-wire/src/controls.rs), and their
[open-forge contract tests](https://github.com/HardMax71/amiss/blob/main/crates/amiss-wire/tests/controls.rs) pin that grammar and
the exact repository/target-ref binding. The execution constraint additionally pins the action
tree, release manifest, platform, declared required-status name, and bootstrap in its
[dedicated parser](https://github.com/HardMax71/amiss/blob/main/crates/amiss-wire/src/controls/execution_constraint.rs).
A status name is data, not proof of which provider integration published it; source-bound
enforcement remains an adapter responsibility.

Trusted time binds more than a timestamp. Its
[current parser](https://github.com/HardMax71/amiss/blob/main/crates/amiss-wire/src/controls/trusted_time.rs) requires the repository
and protected target ref, a provider namespace, an opaque bounded provider run ID and positive
attempt, and the candidate-identity digest. That candidate identity includes both candidate and
target refs, the selected URL dialect, the repository, and the snapshots, so changing any of
those cannot replay a statement for the same Git trees. The controls request must repeat the
same provider/run tuple, and the
[verification gate](https://github.com/HardMax71/amiss/blob/main/crates/amiss-scan/src/policy.rs) compares it byte-for-byte before
using the time.

These are binding rules, not authentication. The separate controller foundation defines the
required order: an adapter authenticates an untouched provider delivery, a durable ledger
claims its replay identity, the adapter refreshes authoritative state, and only then may a
runner construct the requests for that exact repository, URL dialect, candidate, target and
default-branch refs, commits, and trees. It refreshes again before publication. No concrete
adapter, durable ledger, runner, or publisher implements the surrounding provider path today.
The storage-neutral ledger contract specifies exact binding, a stable evaluation ID, expiring
leases, monotonic fences, atomic publication staging, retry of the frozen value, and atomic
winning completion without embedding SQL or a database. The contract neither authenticates provider
input nor turns controls into provider authority. The request's `forge` value remains only the URL
dialect used by link resolution and is separate from the controller's provider namespace and
instance identity. Debt must reproduce its adoption tree, and a waiver item for another
candidate tree is simply not selected. The commit and staged-index paths share one
[trusted-time, debt, and waiver pipeline](https://github.com/HardMax71/amiss/blob/main/crates/amiss-scan/src/pipeline/external.rs).

Debt and waiver require verified trusted time and a complete Git candidate. An item
carries its accepted fact, and that fact is the sole source of the finding kind and the
key-input preimage; `finding_key` is recomputed from the nested key. The fact can name
only `explicit-target-missing` or `explicit-target-type-mismatch`. Selection needs an
exact current finding key with a candidate fact; a resolved projection or an absent key
is not an exception target. Matching also requires the exact fact digest. When
everything lines up, active unchanged debt records tolerance at `warn`, and an applied
waiver changes only `fail` to `warn`. Invalid, expired, worsened, or overlapping items
suppress nothing, and an overlap makes evaluation incomplete.

The [wrapper tests](https://github.com/HardMax71/amiss/blob/main/crates/amiss-scan/tests/wrapper.rs)
pin binding, trusted-time, expiry, fact-drift, wrong-tree selection, resolved-target, and
overlap behavior. The published [`complete-findings`, `debt-items`, and `waiver-items`
ceilings](limits.md) bound the accepted sets, and the `amiss-scan` `pipeline` benchmark
tracks matching as findings and exception items grow.

One asymmetry remains in the current control contract: the report can carry a finding on a
document whose name is raw bytes, but waiver and debt items spell paths as text. Such a
finding is reportable yet cannot be waived or adopted.

The machine-facing evaluation and controls requests are closed by the
[evaluation-request schema](https://github.com/HardMax71/amiss/blob/main/spec/scanner-evaluation-request.schema.json) and
[controls-request schema](https://github.com/HardMax71/amiss/blob/main/spec/scanner-controls-request.schema.json), with matching
[strict parser tests](https://github.com/HardMax71/amiss/blob/main/crates/amiss-wire/tests/requests.rs). Their unversioned names are
intentional: before 1.0 the shipped schema, parser, examples, and report form one rolling
contract and move together.

In the public command and GitHub composite Action, all five external controls are absent and no
protected target ref is authenticated. The report records `status: "none"` separately for
organization floor, debt snapshot, waiver bundle, execution constraint, and trusted time; its
sandbox assurance is `self-asserted`. There is no aggregate `provider_verified` field. The
exact projection is built in the
[report writer](https://github.com/HardMax71/amiss/blob/main/crates/amiss-scan/src/report.rs).

The sealed bootstrap path can now carry all five controls to the engine. Its report acceptance
binds the requested profile; exact organization-floor, debt-snapshot, and waiver-bundle
presence, digest, and trust source; the execution-constraint digest, trust source, and
recomputed descriptor semantics; the trusted-time digest, provider run, instant, and recomputed
statement semantics; and the candidate identity and honest sandbox projection. The public
[CLI shell](https://github.com/HardMax71/amiss/blob/main/crates/amiss/src/main.rs) still supplies
each value as `None`. A report control row with `status: "verified"` means the engine verified
the supplied value's digest and identity relationships. It does not prove that a provider
authenticated or supplied the value: neither the report nor its enum authenticates its own
source. Until the delivery lane in [Project status](status.md) is complete, the honest reading
of a local or convenience-Action report is: these findings, under this repository policy, with
no outside authority consulted.

The control-plane finding family closes the loop from the other side. When a candidate
weakens its own policy file or drops required coverage, the comparison raises
`policy-weakened` or `coverage-reduced`. With a verified organization floor,
`control-plane-changed` reports a protected control path unless both base and candidate are
present supported blobs with identical path, mode, and raw-content evidence. Loosening the
rules is reported under the rules being loosened.
