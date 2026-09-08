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
observed hop used 301 or 308. A forge row reports what the API said: the repository's
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

Plans and assessments share the engine descriptor: its version and digest are the only
accepted fields. An unknown field in that object is rejected even with a matching payload digest.

Evidence is decoded directly into its declared producer and row types. Unknown fields and
positional arrays are rejected, including extra producer fields echoed by an assessment.
The reader verifies that decoding preserves the complete input's canonical identity and
returns that digest to the judge. Whitespace and equivalent JSON escapes preserve the
identity; a change to any accepted evidence field changes it.

Assessment reading uses the complete typed envelope too: extra fields and positional arrays
are rejected throughout its payload. The reader and writer share the same derived validation
and payload hashing path. Invalid verdict relationships are reported before a digest mismatch;
valid examples and their recorded identities stay unchanged.

The judgment policy is fixed in the engine and deliberately conservative, because the
web's refusals outnumber its deaths. A 404 or 410 refutes only when a GET confirmed it,
since servers drop HEAD requests they would answer. A 401, 403, 429, or LinkedIn's 999
is a wall, not a grave: unproven. Transport failures, unfollowed redirects, and absent
evidence are unproven too, each with its reason named. On the forge side a missing
repository never refutes, since forges answer 404 for private repositories they will not
name; refutation needs a readable repository whose refs resolved and whose path or
revision then proved absent. Every redirect destination remains evidence, but only an
all-301/308 chain lands as a `retarget` suggestion on the row, never a finding or an
automatic edit. And `reachable` claims exactly what it says: something answered, not that
the content is still right.

GitHub repository, content and commit presence, and GitLab project visibility and file or
commit presence, use status-only API HEAD requests. These checks use the authenticated
response status without requesting a JSON body; rate limits and unsupported methods are
errors, not evidence that a target is missing. Ref listings still decode GET responses.
GitLab tree presence still requires a GET with a nonempty array of typed entries from the
[repository tree API](https://docs.gitlab.com/api/repositories/#list-all-repository-trees-in-a-project).
The entries retain every requested field and reject unknown fields, invalid object IDs and
invalid modes. An empty tree page remains unproven. This is forge API evidence; the
arbitrary-URL probe's GET confirmation policy is unchanged.

Provider transports check declared and actual response sizes before passing the complete
body to the selected JSON decoder. An oversized or rejected body is an invalid response;
a failed read is an unavailable provider. The transport limit applies whichever typed
reader a route uses.

Gitea and Forgejo content checks keep GET because HEAD support differs between deployments.
They decode complete file or directory responses, retaining the declared metadata and
nullable fields without inventing missing modes or timestamps. Unknown fields and kinds,
malformed object IDs, duplicate keys and positional objects are rejected before content
can prove presence. The returned links are metadata, not URLs the verifier follows.

The Gitea-family adapter also retains complete user profiles, including the compatibility
username and optional Forgejo pronouns. Authenticated-user lookup uses the bounded lossless
reader before checking the dedicated reviewer's identity. Unknown fields, missing profile
keys and unknown visibility values cannot be silently discarded or defaulted.

Commit lookups retain the complete commit, parent, account, file, statistics and signature
metadata through the same reader. Disabled metadata remains explicit null, while parent
lists must be supplied. A nonempty malformed page cannot prove commit presence; an empty
successful page remains unknown. Object IDs stay typed through refresh and relation
consumers. Parsing an API tree identifier does not establish that it names the actual Git
tree; refresh continues to use independently resolved Git objects.

Repository visibility and refresh decode complete repository records, also reused for
pull-request repositories and fork parents. Provider-specific omission and explicit null
remain distinct. Tracker settings and transfer teams are typed, including unit permissions;
unknown nested fields or permission names are rejected. Returned URLs remain metadata,
not alternate fetch destinations.

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
