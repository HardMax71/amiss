# The external assessment

[The external plan](external-plan.md) names work; the assessment judges what came back.
A producer probes the plan's introduced destinations or asks a forge API about the shaped
ones, and writes its observations into an evidence file. Two producers ship in this
repository: the provider lanes verify shaped destinations through their own APIs, and
`amiss-probe --plan plan.json` probes the unshaped https ones, every URL and redirect hop
vetted and address-pinned before a byte leaves the process. Any other producer works too.
The engine then judges offline:

```sh
amiss external-assess --plan plan.json --evidence evidence.json --format json
```

Evidence carries observations, never verdicts. A probe row reports the final status or
the transport failure, exactly one of the two, the method that saw it, and where
redirects ended. A forge row reports what the API said: the repository's visibility
first, then how the opaque tail resolved against its refs. The file binds the exact plan
by payload digest, and the discipline is strict in both directions: a row naming a
destination the plan did not introduce, repeating one, or binding another plan refuses
the whole run, while destinations the file never mentions simply stay unproven. The
schemas are
[`scanner-external-evidence.schema.json`](https://github.com/HardMax71/amiss/blob/main/spec/scanner-external-evidence.schema.json)
and
[`scanner-external-assessment.schema.json`](https://github.com/HardMax71/amiss/blob/main/spec/scanner-external-assessment.schema.json),
and the assessment example is derived from the plan and evidence examples by the same
code path, checked in CI.

The judgment policy is fixed in the engine and deliberately conservative, because the
web's refusals outnumber its deaths. A 404 or 410 refutes only when a GET confirmed it,
since servers drop HEAD requests they would answer. A 401, 403, 429, or LinkedIn's 999
is a wall, not a grave: unproven. Transport failures, unfollowed redirects, and absent
evidence are unproven too, each with its reason named. On the forge side a missing
repository never refutes, since forges answer 404 for private repositories they will not
name; refutation needs a readable repository whose refs resolved and whose path or
revision then proved absent. Where redirects ended, permanent or not, lands as a
`retarget` suggestion on the row, never a finding. And `reachable` claims exactly what it
says: something answered, not that the content is still right.

Every verdict row echoes the plan's document attribution, and the subject block binds
report, plan, and evidence digests, so the same three inputs always reproduce the same
assessment, digest included, and a lane can replay the whole chain from artifacts alone.
Exit 0 wrote the assessment, refuted rows included. The command itself remains advisory; a
consumer decides what those rows do. Exit 2 means an input could not be trusted.

Provider plans expose that decision as `external_policy`. `off` makes no external API calls;
`advisory`, the default, retains and counts the assessment without changing the engine result;
and `block-confirmed-refutations` changes a passing provider result to block only when the
retained assessment contains at least one `refuted` row. An incomplete assessment, `unproven`
row, authentication or rate-limit wall, private-repository 404, transport failure, missing
evidence, or reachable row never changes the engine result. The blocking mode is an opt-in pilot:
review a lane's retained advisory evidence over time before enabling it. Arbitrary HTTPS remains
the separate advisory experiment described in [Continuous integration](ci.md).

Provider lanes retain the canonical plan, provider evidence, and assessment beside the exact
provider-bound report before the final provider refresh and publication stage. The policy is part
of the controller plan digest. The published assessment digest and artifact locator therefore
name one frozen chain and one frozen decision. A lost provider reply or service restart verifies
and reuses those bytes without another API probe; incomplete verification is retained as
incomplete rather than reconstructed later. If the final refresh finds a changed head or gate,
the staged result is superseded even when the retained assessment had refuted a destination.
Authorization, expiry, and capacity are defined in [Retained provider artifacts](provider-artifacts.md).
