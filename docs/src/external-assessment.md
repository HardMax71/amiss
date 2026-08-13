# The external assessment

[The external plan](external-plan.md) names work; the assessment judges what came back.
A producer, any producer, probes the plan's introduced destinations or asks a forge API
about the shaped ones, and writes its observations into an evidence file. The engine then
judges offline:

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
revision then proved absent. A permanent redirect lands as a `retarget` suggestion on the
row, never a finding. And `reachable` claims exactly what it says: something answered,
not that the content is still right.

Every verdict row echoes the plan's document attribution, and the subject block binds
report, plan, and evidence digests, so the same three inputs always reproduce the same
assessment, digest included, and a lane can replay the whole chain from artifacts alone.
Exit 0 wrote the assessment, refuted rows included; the artifact is advisory data, and
whether a refuted introduced destination blocks anything is a policy its consumers own.
Exit 2 means an input could not be trusted.
