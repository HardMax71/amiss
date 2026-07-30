# Provider-verified controls

Closed July 2026. The engine report is self-asserted, so the gate had to become an object the
provider owns and the checked repository cannot forge. Most of it landed in
[#107](https://github.com/hardmax71/amiss/pull/107); the corrections that live instances forced
came later, in [#131](https://github.com/hardmax71/amiss/pull/131) and
[#132](https://github.com/hardmax71/amiss/pull/132).

## One evaluation contract, not one per provider

Two problems shared one cause. The contract named provider-specific identity types, so describing
GitLab meant growing a second shape and a provider enum sat in the middle of the trust boundary,
where adding a provider means editing everything that matches on it. Separately, one ref was doing
two jobs: the ref used to resolve URLs and the protected branch that controls apply to are
different things, and conflating them means a check can verify one branch while the merge rule
guards another.

The rolling contract separates the source ref from the protected target ref, and a frozen
controller evaluation binds provider, integration, repository, URL dialect, change, refs, commits,
trees, provider gate, check plan, execution limits, and trusted time, none of which requires
knowing which provider is speaking. Providers differ in how those facts are obtained, not in what
the evaluation says.

The change was mostly deletion. Opening the execution-constraint identity took 291 lines added
against 169 removed; rolling the contracts forward removed 10,549 lines across 136 files while
adding 2,499. Forge-shaped variants had accumulated in the wire types, the schemas, the examples,
and the goldens, and most of the work was proving they were redundant rather than writing something
new.

Opened in [#57](https://github.com/hardmax71/amiss/pull/57) and rolled forward in
[#58](https://github.com/hardmax71/amiss/pull/58). The types are
[`crates/amiss-wire/src/requests.rs`](https://github.com/hardmax71/amiss/blob/main/crates/amiss-wire/src/requests.rs).

## The controller ships as source, not as a crate

The engine is published to crates.io and has no network capability at all, which is checked rather
than claimed: a separate dependency policy bans HTTP clients, async runtimes, and socket crates
from the engine's graph, with reasons written into the file.

```toml
{ crate = "reqwest", reason = "the engine has no HTTP client" },
{ crate = "tokio", reason = "the engine has no async runtime and no sockets" },
{ crate = "socket2", reason = "the engine opens no sockets" },
```

The controller does have network capability, credentials, and provider tokens. Publishing it as a
convenient dependency would put all of that one `cargo add` away from anyone who wanted the
scanner, and would make the scanner's dependency graph the union of both. So the
[`controller/`](https://github.com/hardmax71/amiss/tree/main/controller) workspace stays unpublished
and source-built. An operator who wants a provider lane builds it from a commit they chose.

Inside, provider differences live in small crates rather than in a closed provider enum:
provider-neutral traits and the orchestrator, a bounded ingress gate, a rotating key ring,
signed-webhook checks, GitLab OIDC checks, and one adapter crate per provider family. A fourth
provider is a new crate, not a new arm in every match statement.

Introduced with the controller foundation in [#98](https://github.com/hardmax71/amiss/pull/98) and
folded into a single workspace in [#123](https://github.com/hardmax71/amiss/pull/123), which kept
the two dependency graphs separate while removing the duplication two independent workspaces had
caused: 3,352 lines added against 4,474 removed.

## The bootstrap takes canonical documents and nothing else

The bootstrap is the trusted edge. A provider lane acquires objects, then hands them to this
binary, and whatever it accepts is what the engine ends up believing. Every format it tolerates is
a format an attacker may write.

It accepts three canonical documents, checks their required bindings, and passes their exact bytes
to the verified engine in one closed input frame: the evaluation, the snapshot, and the controls.
Bytes in, bytes out, with no reformatting step in between where a difference could hide. The
documents have published schemas rather than being an internal convention, so an operator can
validate what a lane will present before presenting it. The same wire library produces canonical
execution limits and trusted-time statements, so the documents a lane presents come from the code
that validates them rather than from a second implementation that agrees until it does not.

The executable itself is bounded at 33,554,432 bytes:

```rust
pub const BOOTSTRAP_EXECUTABLE_BYTES: u64 = 33_554_432;
```

That bound is load-bearing in a way that only shows up in practice. A fixture binary that linked
one crate too many crossed it during this project's own lane testing and every run refused with
`Unavailable` rather than running an unbounded executable, which is the ceiling doing its job on the
person who set it.

The crate has shipped since [#1](https://github.com/hardmax71/amiss/pull/1); it learned these
documents with the sealed evaluation foundation in
[#98](https://github.com/hardmax71/amiss/pull/98). It is
[`crates/amiss-bootstrap/`](https://github.com/hardmax71/amiss/tree/main/crates/amiss-bootstrap).

## Authenticate first, save the raw bytes, then acknowledge

Two ordering mistakes are easy to make in a webhook receiver, and both are quiet. Parse before
authenticating, and a parser meets hostile input for free. Acknowledge before storing, and a
restart in the wrong millisecond loses a delivery the provider will never send again.

The receiver authenticates before admission and saves the exact raw delivery before acknowledging
it. Raw means the bytes that were signed, not a re-serialized version of them, because a signature
covers bytes and anything else is a different document.

The inbox is ordinary files, like the delivery record it feeds, and carries the properties that
make a queue survivable: it outlives a restart, enforces both row and byte capacity, renews
ownership while the controller works, retries temporary provider failures rather than treating them
as verdicts, and removes the raw bytes only once the delivery ledger has completed. That last
ordering means the bytes outlive every state that might still need them.

The listener is bounded before any of that: a fixed body ceiling, a fixed header count and header
byte budget, and a delivery permit taken before the body is read and held through durable
admission, so the memory a hostile sender can commit is decided by configuration rather than by the
sender.

Completed in [#107](https://github.com/hardmax71/amiss/pull/107). The receiver and inbox are
[`controller/service/src/`](https://github.com/hardmax71/amiss/tree/main/controller/service/src).

## Objects are fetched by exact name under fixed limits

Fetching a branch and trusting what arrives lets the remote choose what gets scanned. Fetching
without limits lets it choose how much memory the controller uses. Both are the same mistake:
letting the answer decide the question.

Acquisition speaks Git protocol v2 with exact authenticated SHA-1 wants for the repository commit
and the pinned action commit, so the remote answers a question rather than proposing an answer. One
deadline covers network receipt and validation together, because a deadline that stops at the
socket lets a slow validator hang after a fast download.

The pack limits are fixed constants, not configuration, and every one of them fails closed:

```rust
pack_bytes: 2_147_483_648,
objects: 2_000_000,
object_bytes: 134_217_728,
inflated_bytes: 4_294_967_296,
resolved_bytes: 4_294_967_296,
delta_depth: 128,
```

`REF_DELTA` is rejected outright, since a delta against an object outside the pack is a request to
resolve something the sender did not send. Pack indexing uses one thread, which trades throughput
for a bounded and reproducible cost profile. The protocol client is
[`controller/git/src/protocol.rs`](https://github.com/hardmax71/amiss/blob/main/controller/git/src/protocol.rs).

## The runner seals the job it supervises

Between acquiring objects and trusting a result there is a process to start. A process is where
inherited environment variables, inherited file descriptors, and leftover child processes turn into
a trust problem, and none of those show up in a happy-path test.

The runner rechecks the acquired repository and action roots rather than trusting that acquisition
put the right things there, derives a sealed job, and checks the pinned bootstrap against its
expected digest. It prepares private inputs, clears inherited environment and streams, and
supervises one cross-platform process tree. The controller owns the output handles, so the child
writes into handles it did not open and cannot redirect.

Reading the result is equally distrustful. Wall-clock and lease limits both apply, whichever ends
first. The output tree is proven empty before the run, so a leftover file cannot be read as this
run's answer. The report is bounded, and an incomplete or malformed result is rejected rather than
parsed optimistically.

The pinned failures are the shape of the guarantee: wrong roots, bootstrap tampering, bad output,
missing output, oversize output, timeout, heartbeat loss, and live descendants. That last one
matters most in practice, because a child that outlives its parent is how a runner leaks work into
the next job.

Sealed in [#106](https://github.com/hardmax71/amiss/pull/106). The runner is
[`controller/src/bootstrap_runner.rs`](https://github.com/hardmax71/amiss/blob/main/controller/src/bootstrap_runner.rs)
and acquisition is
[`controller/src/acquiring_runner.rs`](https://github.com/hardmax71/amiss/blob/main/controller/src/acquiring_runner.rs).

## The GitHub lane runs one repository end to end

A provider lane is only meaningful if an operator can stand the whole thing up: one repository, one
App installation, one protected branch, and a service that fails loudly when its configuration is
wrong rather than at the first webhook.

The source-built GitHub service completes that lane on GitHub.com or a compatible GHES release.
Strict JSON loads the App key, the rotating webhook secrets, the external controls, the execution
constraint, the bootstrap, and separate private state roots, refusing any field it does not
recognize instead of ignoring it. An unknown key in a trust-boundary configuration is either a typo
or an attempt, and neither should be silently dropped.

The listener speaks plaintext by design and is deployed behind an operator-owned TLS and
connection-limit boundary. That is stated rather than assumed, because a service that pretends to
terminate TLS while sitting behind a proxy that also does invites exactly one confusion.

The App identity is what makes the gate real: the required status is bound to the App's integration
id in the repository ruleset, so no other actor can post the check that satisfies it. The service is
[`controller/github-service/`](https://github.com/hardmax71/amiss/tree/main/controller/github-service)
and the setup is [GitHub](../provider-github.md).

## The GitHub source accepts four events and binds them

Accepting every webhook a provider offers widens the attack surface for nothing. Most events cannot
change what a documentation check would conclude, and each one accepted is another payload shape
that has to be parsed safely.

The source accepts signed `opened`, `reopened`, and `synchronize` pull-request events:

```rust
const SUPPORTED_ACTIONS: [&str; 3] = ["opened", "reopened", "synchronize"];
```

`edited` is accepted only when the signed payload says the base branch changed, because that is the
one edit that moves what the check is about. An edited title is not a new evaluation.

Admission then binds the configured repository and target, so a correctly signed event for another
repository is refused rather than evaluated. Signature validity answers "did GitHub send this", not
"is this mine".

After admission the App client refreshes the exact repository, pull request, ref, commit, tree, and
test-merge facts from the API rather than trusting the payload's copy of them, and requires a strict
active status rule whose context is bound to that App. It refreshes again before saving the result,
because the state that decides a verdict is the state at publication, not the state when the webhook
arrived. The source is
[`controller/github/src/lib.rs`](https://github.com/hardmax71/amiss/blob/main/controller/github/src/lib.rs).

## The verdict lands on the commit GitHub actually merges

A check attached to the head commit describes the branch. A merge queue merges something else, the
test-merge commit, and a status on that commit takes precedence over the head. Publishing to the
wrong one produces a green branch and an unchecked merge, which is the failure mode worth the most
care in the whole lane.

Publication attaches `success`, `failure`, or `cancelled` to GitHub's authoritative test-merge
commit. The summary binds the gate, provider run, refs, commits, trees, plan, execution constraint,
report digest, and a stable unavailable reason, so the Check Run says what was evaluated rather than
only how it ended. A reader who distrusts the verdict can reproduce the inputs from the check
itself.

Idempotency is honest about its limit. The evaluation ID reconciles one exact visible retry, so an
ordinary retry updates rather than duplicates. A create that GitHub accepted but whose reply was
lost can still leave a duplicate, because GitHub and the local ledger do not share a transaction,
and no amount of local bookkeeping fixes that. The page says so rather than implying exactly-once.

A final pull-request refresh turns an out-of-order publication into a no-op once its staged head,
base, refs, or gate is no longer current, so slow work cannot write a stale verdict onto a newer
gate. Publication is
[`controller/github/src/live/`](https://github.com/hardmax71/amiss/tree/main/controller/github/src/live).

## The GitLab lane runs as a policy job on the merge train

GitLab has no App identity to own a status, and any project member can edit a job the project
defines. A gate the checked project can edit is not a gate, so the usual shape, a CI job that posts
its own result, does not survive the threat model.

The lane uses a pipeline execution policy owned outside the checked project, which injects the job
into every enforced merge train. The checked project cannot remove it, rewrite it, or skip it. The
service then authenticates the job's short-lived OIDC token and binds its policy project and commit,
job and pipeline, runner, merge request, repository, and exact train-result commit before trusting
any provider state at all. Each of those is a way the job could be someone else's, and the binding
is what makes the token mean this run rather than any run.

The lane requires GitLab 19.3 or newer with Ultimate, because enforced merge trains are what make
the policy job unavoidable, and they are generally available from 19.3. No live run is recorded yet:
as of July 2026 the newest release is 19.2.0, so no supported instance exists to run.

The service is
[`controller/gitlab-service/`](https://github.com/hardmax71/amiss/tree/main/controller/gitlab-service),
the OIDC checks are
[`controller/gitlab/src/oidc.rs`](https://github.com/hardmax71/amiss/blob/main/controller/gitlab/src/oidc.rs),
and the setup is [GitLab](../provider-gitlab.md).

## The GitLab gate refuses anything but the exact saved pass

The policy job is a synchronous endpoint: it asks the service a question and merges on the answer.
Anything other than a proven pass returning success turns the whole lane into decoration.

Refresh requires the configured merge method, exactly two train parents, an active policy job, a
protected target branch with no push or bypass path, and merge-train enforcement for all users. Each
is a way the shape being gated could differ from the shape that was verified. Two train parents in
particular is what makes the train result the thing that merges rather than some other commit that
happens to be nearby.

Then the endpoint refuses everything except the exact saved pass. Success is the exact HTTP `204`
and nothing else. Block, unavailable, duplicate, expired, replayed, and changed state all keep the
policy job failed. There is no "probably fine" state, and no path where a missing answer reads as an
affirmative one. The rules are
[`controller/gitlab/src/live/refresh.rs`](https://github.com/hardmax71/amiss/blob/main/controller/gitlab/src/live/refresh.rs).

## The Gitea family lane publishes through a dedicated reviewer

Gitea and Forgejo have no App identity and no first-class status owner. The only gate available is
an approval from an account nobody else controls, which makes the account itself a trust anchor:
whoever can act as that reviewer can satisfy the gate without Amiss, and the provider page says so
in those words.

The service authenticates the native exact-body HMAC, refreshes the pull request, commits, trees,
effective branch rule, and reviewer identity, then publishes an approval or a request for changes as
that one account. It supports Gitea 1.27 or newer and Forgejo 16 or newer.

Getting it to work against real instances took four corrections that no fixture had caught, because
each fixture was written to the API's documentation rather than its behavior. Both families answer
`/git/commits/{sha}` with the commit's own name in the commit's `tree` field:

```text
$ curl .../git/commits/436a6f35fd89b32d8661c6d7e12ba19960dfd841
  sha        : 436a6f35fd89b32d8661c6d7e12ba19960dfd841
  commit.tree: 436a6f35fd89b32d8661c6d7e12ba19960dfd841
$ git cat-file -p 436a6f35
  tree 5c1c95daa0e57e7a46ad6937d4b1515e0b5ff43f
```

No route on either family states the tree of a commit, so trees now come from fetched Git objects
through the same resolver the GitLab lane already used. Forgejo also sends one signature under two
spellings, `X-Forgejo-Signature` and `X-Gitea-Signature`, and the header reader treated the second
spelling as ambiguity and answered `401` to every real Forgejo delivery. A transient
`mergeable: false`, which Gitea reports for a second or two while it recomputes a merge, was being
read as a terminal verdict. And a control revoked between staging and publication left the lane
publishing nothing at all.

The service is
[`controller/gitea-service/`](https://github.com/hardmax71/amiss/tree/main/controller/gitea-service)
and the setup is [Gitea and Forgejo](../provider-gitea.md).

## The Gitea family gate is checked, not assumed

An approval gates a merge only if the branch rule actually requires that approval and closes every
other way in. Those are separate facts, reported separately, and either one missing makes the
approval decorative.

The gate requires one approval restricted to the dedicated reviewer, closed direct-push and bypass
paths, stale and rejected review blocking, an up-to-date pull request, and administrator
enforcement. The adapter checks the distinct Gitea and Forgejo capability shapes rather than guessing
which forge it is talking to from headers, because the two report overlapping fields with different
meanings and a wrong guess produces a confident wrong answer.

Two facts about this only surfaced against live instances. Reading the rule needs repository
administrator access, not write: below that, `/branch_protections/{rule}` answers `403` and the
branch route leaves `effective_branch_protection_name` empty, so the lane cannot read the rule it is
required to check. The documentation said write access, which cannot work. And the gate is verifiable
in both directions now: with the rule intact the lane approves, with direct push re-enabled it
publishes `unavailable / authorization-revoked`, and restoring the rule returns it to approving the
same content. The checks are
[`controller/gitea/src/live/refresh.rs`](https://github.com/hardmax71/amiss/blob/main/controller/gitea/src/live/refresh.rs).

## The lanes are tested through, and against, themselves

A lane test that only walks the happy path proves the pieces connect. It says nothing about the
cases a gate exists for, and those cases are the product.

End-to-end and focused tests carry a signed delivery through authentication, durable admission,
provider refresh, the runner, the provider gate, completion, and replay suppression. The negative
list is the real coverage: wrong provider, repository, target, runner, policy, reviewer, commit, and
tree; changed bootstrap or merge rule; expiry and replay; missing output and timeout; malformed or
tampered input and state; capacity and restart; lost ownership; ref or gate drift; oversized and
malformed packs; `REF_DELTA`; excessive delta depth; and conflicting provider evidence.

The limit of that coverage is worth recording, because live instances found it. Every double in
these suites answers the way the provider's documentation says it will. Gitea's test double returned
a tree name distinct from its commit name, which real Gitea never does. The Forgejo lane test sent
one signature header, which real Forgejo never does. Both suites passed while neither provider could
have worked, and no amount of adding cases to a double that agrees with the code would have found
it.

What did find it was standing up real instances, which is why
[Retained provider runs](../provider-evidence.md) exists as a separate kind of evidence rather than
as more tests. The fixtures are still the right regression net: they run in seconds, they cover the
negative cases exhaustively, and they catch a change that breaks a lane. They just cannot tell you
the provider was never like that. The suites live under each service, such as
[`controller/github-service/tests/lane/`](https://github.com/hardmax71/amiss/tree/main/controller/github-service/tests/lane).

## Provider evidence lives in the provider, not in the report

A report that says it was verified is a report asserting its own trustworthiness. Anything that can
produce the report can produce the claim, so the field would be worth exactly nothing and would read
as though it were worth something. That is worse than omitting it.

So the evidence is an object the provider owns and the checked repository cannot forge: the App-owned
Check Run, the protected GitLab policy job, or the dedicated Gitea-family review, each paired with
the merge rule that makes it necessary. The engine report stays exactly what it was, self-asserted,
with no provider signature and no `provider_verified` field. Nothing was added to it, and that
decision is the one worth recording: the natural move when shipping provider verification is to
stamp the artifact, and the stamp would have been a lie.

Each provider page states its own commit or tree freshness limit, retry behavior, rotation rules, and
full trust boundary, including which accounts and keys can satisfy the gate without Amiss. A trust
boundary that is not written down is a trust boundary nobody checked.

Live runs for GitHub, Gitea 1.27.0, and Forgejo 16.0.1, in both directions, are in
[Retained provider runs](../provider-evidence.md). The lanes are
[Provider-verified controls](../provider-controls.md).
