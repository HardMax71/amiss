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
redirects ended. It marks that destination as a permanent retarget only when every
observed hop used 301 or 308 or was an instant refresh. A redirect stub often answers 200 with
`<meta http-equiv="refresh" content="0; url=...">` in its head, so the probe reads the first
16 KiB of a page that answered with HTML and follows a zero-delay refresh as one more hop. A
refresh with a delay is a page the reader sees first, and it moves nothing. A forge row reports what the API said: the repository's
visibility first, then how the opaque tail resolved against its refs. The file binds the exact plan
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
revision then proved absent. Every redirect destination remains evidence, but only a
permanent chain ending on a page that answered lands as a `retarget` suggestion on the row,
never a finding or an automatic edit. A permanent move onto a page that is gone offers no edit. And `reachable` claims exactly what it says: something answered, not that
the content is still right.

The plan names what a change introduced, so a link that rotted on its own is never probed. A
full audit makes every destination introduced by reading the tree against an empty commit:

```sh
base=$(git commit-tree "$(git hash-object -w -t tree /dev/null)" -m empty)
amiss check --repo . --object-format sha1 --base "$base" \
  --candidate "$(git rev-parse HEAD)" --profile observe --format json > amiss-report.json
amiss external-plan --report amiss-report.json --format json > amiss-plan.json
amiss-probe --plan amiss-plan.json > amiss-evidence.json
amiss external-assess --plan amiss-plan.json --evidence amiss-evidence.json
```

The `-w` matters: git knows the empty tree without storing it, and Amiss reads only the object
store. The commit is dangling and costs nothing. The probe still stops at 64 destinations or
two minutes, whichever comes first, and names what it left unproven on stderr, so a large tree
takes several scheduled runs.

Every verdict row echoes the plan's document attribution, and the subject block binds
report, plan, and evidence digests, so the same three inputs always reproduce the same
assessment, digest included, and a lane can replay the whole chain from artifacts alone.
Exit 0 wrote the assessment, refuted rows included. The command itself remains advisory; a
consumer decides what those rows do. Human output shows up to ten permanent-retarget
suggestions and names any overflow; JSON retains every row. Exit 2 means an input could not
be trusted.

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
