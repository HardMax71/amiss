# Controls and policy

Two kinds of configuration can shape a run, and they carry opposite amounts of trust.

The repository policy is the one input read from the scanned tree itself, and it is
correspondingly weak. `.amiss/scanner-policy.json` can add directories to scan, list
protected paths whose removal is always a finding, declare exact source-to-document
projections, and raise the disposition of
`explicit-target-missing`, `explicit-target-type-mismatch`, and `invalid-reference`. Raise
only: repository policy combines with the built-in profile by maximum, so it can promote an
observe warning to `fail` and can never downgrade or suppress it. An unknown
field makes the whole file invalid and the run incomplete, which is what keeps the policy
from growing into a plugin system one field at a time.

The complete grammar in one example, with `projection_assertions` optional for compatibility
with policies written before projections existed and the file valid only whole:

```json
{
  "schema": "amiss/scanner-policy",
  "document_includes": [
    { "path": "build/docs", "kind": "tree" },
    { "path": "docs", "kind": "tree", "suffix": ".txt", "adapter": "rst" },
    { "path": "notes/ARCHITECTURE", "kind": "document", "adapter": "markdown" }
  ],
  "projection_assertions": [
    {
      "document": "docs/api.md",
      "name": "request-shape",
      "projection": "code-text-v1",
      "sink": "previous-code",
      "source": {
        "kind": "blob-lines",
        "path": "examples/request.json",
        "first_line": 1,
        "last_line": 12
      }
    }
  ],
  "protected_inventory": ["docs/install.md"],
  "finding_dispositions": [
    { "finding_kind": "explicit-target-missing", "disposition": "fail" }
  ]
}
```

The first tree include readmits a subtree the built-in skip list would drop, which is
[Discovery](discovery.md)'s monorepo lever. The second admits only `.txt` descendants of
`docs` and reads them as reStructuredText. The document include reads one extensionless file
under the markdown grammar. The protected path makes its removal a finding, and the disposition
row promotes one kind to `fail`. The
[scanner-policy schema](https://github.com/HardMax71/amiss/blob/main/spec/scanner-policy.schema.json)
closes the grammar, and each array keeps the sort order the schema states. The strictness
also sets the upgrade order: an engine that predates a policy field refuses the whole file
and leaves the run incomplete, so a repository grows its policy only after every engine
reading it has learned the field.

A projection assertion is owned by the policy, under the stable identity `(document, name)`.
The `code-text-v1` sources select either an inclusive one-based line interval from a tracked regular
or executable blob, or bytes between distinct exact `start_marker` and `end_marker` lines. Each
printable-ASCII marker line is at most 256 bytes, occurs exactly once, and is itself excluded. Amiss
does not parse a source language or its comment syntax. Duplicate, missing, reversed, same-line,
or non-UTF-8 regions are typed drift, while edits outside the selected region are irrelevant.

The source's `code-text-v1` projection is compared with the semantic Markdown or MDX code block
immediately before `[amiss:<name>]: <amiss:projection>`. It converts CRLF and bare CR to LF and
removes exactly one final line ending; it does not normalize indentation or any other byte. The
document must be in the scanner's discovered set. Missing, duplicate, or non-adjacent sinks, and
absent, non-blob, LFS, or otherwise invalid sources, produce one `projection-drift` finding. A
marker with no matching policy row remains an unsupported reserved capability and makes the run
incomplete. Removing the marker while the policy row survives therefore cannot disable the
relation, while removing the policy row is `policy-weakened` even when the marker is removed too.

A `record-value` source applies `code-text-v1` to one key in a named `record-set@1` semantic
envelope. Each such envelope carries exactly one `record-set` object, including empty complete
sets, and its `records` are strictly ordered and unique by key. Keys are nonempty, control-free
UTF-8 of at most 4,096 bytes; display values have the same character law and a 65,536-byte cap.
The producer's strings remain inert data: Amiss runs no formatter or template. A row that is
present can attest its value even when the envelope says the set is partial. A missing key means
`source-record-absent` only for a complete set; in a partial set it is
`source-record-unproven`, and a missing named set is `source-record-set-absent`. Evidence derived
from an earlier scanner report is not admitted as a projection source.

For a complete envelope, a `record-set` source applies `sorted-rows-v1` to every display value in
UTF-8 byte order, or applies `decimal-count-v1` to the exact number of records. Duplicate display
values remain distinct records because keys, not values, own identity. A partial set produces
`source-record-set-incomplete` for both projections before any equality, count, extra-row, or
absence conclusion is attempted. Row-difference previews remain byte-sorted and bounded.

The `sorted-rows-v1` projection pairs the same sink with a `tree-paths` source: one existing tree
root, an optional exact suffix, and a positive maximum relative depth. It filters the complete
ordered snapshot map without another Git walk or object read, excludes directory entries, and
projects all other tracked paths relative to the root. A qualifying non-UTF-8 path or path with a
control character is typed drift; an invalid Git path already makes the candidate incomplete. Row
mismatches carry exact counts, a pure-ordering flag, and at most 32 rows and 32 KiB of byte-sorted
preview on each side, with exact omitted counts.

`decimal-count-v1` applies to that same complete `tree-paths` source and emits only the canonical
unsigned ASCII member count. It includes qualifying non-UTF-8 and control-containing path names
because none are rendered. A sign, leading zero, grouping separator, label, or whitespace makes
the visible value noncanonical rather than a second spelling of the same count.

A `document` include names one exact path. A `tree` include names that path and descendants
separated by `/`; `specs` therefore covers `specs/api.md` but not `specs-old/api.md`. Matching
is bytewise, including for paths JSON cannot represent as text. A tree may carry one `suffix`:
2–64 UTF-8 bytes beginning with `.`, with no slash, backslash, or NUL. It selects only non-tree
entries at or below that root whose raw path ends in those exact bytes. There are no globs,
wildcards, regexes, excludes, normalization, or case folding, and built-in classifications still
win. The stable selector identity remains `(path, kind)`, so changing or removing the suffix—or
replacing it with a broader tree—reports policy weakening instead of disguising the old selector
as a new one.

[`amiss policy-include`](invocation.md) prints a validated canonical row for the suffixed-tree form
without touching the policy file. Its optional staged-index preview applies this same matching
implementation and reports exact path identities; it does not invent excludes or merge the row
into existing controls.

Document includes, projection assertions, inventory members, and disposition rows share each
snapshot's [published repository-policy entry ceiling](limits.md), so
the base/candidate classification union can contain twice that many distinct roots. Tree roots
and suffix roots are indexed separately; lookup probes path ancestors and suffix components,
never every policy row. The
[`policy` tests](https://github.com/HardMax71/amiss/blob/main/crates/amiss-scan/tests/suite/policy.rs)
pin the semantic boundaries, and the release
[`eligibility` test](https://github.com/HardMax71/amiss/blob/main/crates/amiss-scan/tests/suite/eligibility.rs)
checks the maximum union without scanning every policy row for every discovered path. The
`amiss-scan` `controls` benchmark tracks tree matching, suffix matching, and policy-set comparison
as the entry count grows.

External controls come from outside the repository, because anything stored inside it could
be rewritten by the very pull request under review. The contract defines five nullable controls: an
organization floor (tightens ceilings and dispositions across many repositories), an
adoption debt snapshot (a recorded list of known failures being worked off, mintable
from a real evaluation by [`amiss adopt`](invocation.md)), a waiver
bundle (time-limited permission to pass despite a named failure), trusted time, and an
execution constraint. The sealed request also carries a bounded set of
[semantic-evidence envelopes](semantic-evidence.md), each paired with an independently planned
context digest, candidate-bound, and interpreted only by a compiled consumer. An ordinary
`amiss check` may instead bind one caller-selected candidate-free template after resolving its
exact candidate; that convenience input remains self-asserted and is not an external control.

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
[open-forge contract tests](https://github.com/HardMax71/amiss/tree/main/crates/amiss-wire/tests/controls) pin that grammar and
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

These are binding rules, not authentication. The controller must authenticate provider input
before constructing requests for the exact run. Its provider-neutral sequence and durable retry
contract are documented in [Controller delivery](controller.md). The concrete
[provider lanes](provider-controls.md) load organization policy and their execution constraint
outside the checked repository, authenticate a signed webhook or policy-job token, refresh
provider-owned change and merge-rule state, acquire the exact trees, derive trusted time, and run
the sealed bootstrap. Their separate pages describe the Check Run, policy-job result, or
dedicated review that carries provider evidence.

The request's `forge` value remains only the URL dialect used by link resolution and is separate
from the controller's provider namespace and instance identity. Debt must reproduce its adoption
tree, and a waiver item for another candidate tree is simply not selected. The commit and
staged-index paths share one
[trusted-time, debt, and waiver pipeline](https://github.com/HardMax71/amiss/blob/main/crates/amiss-scan/src/pipeline/external.rs).

Debt and waiver require verified trusted time and a complete Git candidate. An item
carries its accepted fact, and that fact is the sole source of the finding kind and the
key-input preimage; `finding_key` is recomputed from the nested key. The fact can name
only `explicit-target-missing` or `explicit-target-type-mismatch`. Selection needs an
exact current finding key with a candidate fact; a resolved projection or an absent key
is not an exception target. An exact forge commit is part of the normalized target intent, so an
exception for the same path at another immutable commit does not match. Matching also requires the
exact fact digest. When
everything lines up, active unchanged debt records tolerance at `warn`, and an applied
waiver changes only `fail` to `warn`. Invalid, expired, worsened, or overlapping items
suppress nothing, and an overlap makes evaluation incomplete. Both controls travel only
in the sealed request: `amiss adopt` mints a debt snapshot from the public grammar, but
no public flag supplies one back, so consumption belongs to the provider lanes.

The [wrapper tests](https://github.com/HardMax71/amiss/tree/main/crates/amiss-scan/tests/wrapper)
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
[strict parser tests](https://github.com/HardMax71/amiss/blob/main/crates/amiss-wire/tests/suite/requests.rs). Their unversioned names are
intentional: before 1.0 the shipped schema, parser, examples, and report form one rolling
contract and move together.

The request reader decodes trusted time directly into the closed statement type. An unknown
statement field, wrong schema, or non-object statement rejects the request before evaluation;
the consumers still verify its lifetime, expected digest, and authenticated run bindings.

In the public command and GitHub composite Action, all five external controls are absent and no
protected target ref is authenticated. The Action supplies no semantic evidence; a direct public
`check` may carry one self-asserted template. The report records
`status: "none"` separately for
organization floor, debt snapshot, waiver bundle, execution constraint, and trusted time; its
sandbox assurance is `self-asserted`. There is no aggregate `provider_verified` field. The
exact projection is built in the
[report writer](https://github.com/HardMax71/amiss/blob/main/crates/amiss-scan/src/report.rs).

The sealed bootstrap path can carry all five controls and the bounded semantic-evidence set to the
engine. Its report acceptance
binds the requested profile; exact organization-floor, debt-snapshot, and waiver-bundle
presence, digest, and trust source; the execution-constraint digest, trust source, and
recomputed descriptor semantics; the trusted-time digest, provider run, instant, and recomputed
statement semantics; every semantic envelope's payload and producer/input identities after its
planned context matches; and the
candidate identity and honest sandbox projection. The public
[CLI shell](https://github.com/HardMax71/amiss/blob/main/crates/amiss/src/main.rs) still supplies
each nullable external control as `None`; its optional template is bound only inside the candidate
evaluation and leaves sandbox assurance self-asserted. A report control row with
`status: "verified"` means the engine verified
the supplied value's digest and identity relationships. It does not prove that a provider
authenticated or supplied the value: neither the report nor its enum authenticates its own
source. The honest reading of a local or convenience-Action report remains: these findings, under
this repository policy, with no outside authority consulted. In a provider lane, origin is
evidenced separately by the App-owned Check Run, protected policy job, or dedicated review and
the matching merge rule; copied report bytes do not become an attestation.

The control-plane finding family closes the loop from the other side. When a candidate
weakens its own policy file or drops required coverage, the comparison raises
`policy-weakened` or `coverage-reduced`. With a verified organization floor,
`control-plane-changed` reports a protected control path unless both base and candidate are
present supported blobs with identical path, mode, and raw-content evidence. Loosening the
rules is reported under the rules being loosened.
