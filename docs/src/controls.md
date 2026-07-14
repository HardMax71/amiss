# Controls and policy

Two layers may shape an evaluation, and they have opposite trust stories.

The repository policy is the one input read from the tree under evaluation, and it is
correspondingly powerless. `.amiss/scanner-policy.json` may add document include roots, name a
protected inventory of paths whose removal is always a finding, and raise finding
dispositions. Raise only: a disposition may go from `record` to `warn` to `fail` and never
downward, so the policy can make the check stricter for its own repository and cannot weaken
it. An unknown field is an invalid configuration and an incomplete run, which is what stops
the policy from becoming a plugin system by accretion.

External controls arrive from outside the repository, because anything the repository could
write, the pull request author could rewrite. The contract defines five: an organization
floor that may tighten resource ceilings and dispositions across repositories, an adoption
debt snapshot, a waiver bundle, trusted time, and an execution constraint. Each is
identity-bound to the exact evaluation it authorizes: the floor, debt, and waiver documents
name the repository and tree they were issued for, and a control presented against a
different tree is a `CONTROL_BINDING_MISMATCH` refusal, not a shrug. Waivers are the only
sanctioned way to pass with a known failure, they expire, and every application is a visible
policy step in the finding it touched.

In the shipped v0 command line, all five external controls are `none`. The report says so in
so many words, and claims no trust it does not have: `provider_verified` is false, the
control block records the absence of each input, and nothing in the local lane pretends
otherwise. The lane that delivers verified controls, a provider-signed request wire, is
specified but deliberately unbuilt; the report format already carries the fields so that its
arrival is not a breaking change. Until it lands, the honest reading of a local report is:
these findings, under this policy, with no external authority consulted.

The control-plane finding family closes the loop from the other side. When a candidate
weakens its own policy file, shrinks coverage, or touches the control configuration, the
comparison itself raises `policy-weakened`, `coverage-reduced`, or `control-plane-changed`,
so the act of loosening the rules is a finding under the rules being loosened.
