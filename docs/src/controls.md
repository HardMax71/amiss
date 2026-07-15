# Controls and policy

Two kinds of configuration can shape a run, and they carry opposite amounts of trust.

The repository policy is the one input read from the scanned tree itself, and it is
correspondingly weak. `.amiss/scanner-policy.json` can add directories to scan, list
protected paths whose removal is always a finding, and raise how severely a finding kind is
treated. Raise only: `record` can become `warn` or `fail`, and nothing can go the other
way. So a repository can make its own check stricter and can never loosen it. An unknown
field makes the whole file invalid and the run incomplete, which is what keeps the policy
from growing into a plugin system one field at a time.

External controls come from outside the repository, because anything stored inside it could
be rewritten by the very pull request under review. The contract defines five: an
organization floor (tightens ceilings and dispositions across many repositories), an
adoption debt snapshot (a recorded list of known failures being worked off), a waiver
bundle (time-limited permission to pass despite a named failure), trusted time, and an
execution constraint. Each one is tied to the exact repository and tree it was issued for.
Presenting a control against a different tree is a `CONTROL_BINDING_MISMATCH` refusal, not
a shrug. Waivers are the only sanctioned way to pass with a known failure, they expire, and
every waiver that touches a finding appears as a visible step in that finding's history.
One asymmetry follows from the control formats' own versioning: the report can carry a
finding on a document whose name is raw bytes, but the waiver and debt formats still spell
paths as text, so such a finding is reportable yet cannot be waived or adopted until those
formats revise.

In the shipped v0 command line, all five external controls are `none`, and the report says
so plainly: `provider_verified` is false, and each control's absence is recorded. The
delivery lane for verified controls, a provider-signed request format, is specified but not
built. The report already has the fields so that adding the lane later breaks nothing. Until
then, the honest reading of any local report is: these findings, under this policy, with no
outside authority consulted.

The control-plane finding family closes the loop from the other side. When a candidate
weakens its own policy file, shrinks what gets scanned, or edits the control configuration,
the comparison itself raises `policy-weakened`, `coverage-reduced`, or
`control-plane-changed`. Loosening the rules is reported under the rules being loosened.
