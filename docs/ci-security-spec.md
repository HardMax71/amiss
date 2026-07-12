# CI, policy, governance, security, and operations contract

Date: 2026-07-12.

Status: normative resolution for the discard-state scanner. The governed extension is specified
only far enough to close security and concurrency holes; it is not authorized for implementation
by this document.

The decisions in this file replace the CI, policy, refresh, waiver, and trust defaults proposed in
earlier dossier files where they disagree. `MUST`, `MUST NOT`, `SHOULD`, and `MAY` are normative.

## Decision

Scanner v0 is a read-only, stateless comparison of two immutable repository snapshots. It has no
accept command, baseline, lock, refresh lane, repository writer, privileged comment workflow, or
claim-attestation feature. `enforce` blocks every current deterministic structural
failure unless its exact finding key and policy-free fact are covered by valid active external debt
or waiver; attribution never grants an exception, and raw impact remains advisory. Registering that
profile as a required status
remains gated by
[implementation-readiness.md](./implementation-readiness.md), the separate provider request-wire
and control-epoch/provider-freshness RFCs, X-04, X-05, and X-07.

Governed v1 is a different product stage. It requires stable authored claim IDs, externally
verifiable authority, explicit lifecycle transitions, and final-candidate compare-and-swap. If it
is eventually built, its logical state is per claim; one-file-per-claim storage is the X-06 test
candidate, not a stable format. There is no global hot lockfile and no bot that rewrites
observations after every merge. The distinction matters in
practice: Fiberplane reports that changing its shared lock serialization reduced, but did not
eliminate, substantial simulated merge-conflict rates. That is evidence that shared committed
state is an operational cost, not merely a formatting problem; see
[market-reassessment.md](./market-reassessment.md#lockfile-conflict-is-a-product-cost-not-a-serialization-bug).

The two stages have separate security claims:

| Property | Scanner v0 | Governed v1 gate |
| --- | --- | --- |
| Repository state written | None | Human-authored claim change and affected per-claim records, reviewed in the ordinary PR; ordinary acceptance changes exactly one record |
| Privileged service | None | Required for provider-verified acceptance; separately gated |
| Persistent freshness or attestation | None | Explicit acceptance event only |
| Hard safety input | Current evaluation identity, externally protected policy/execution constraint, verified sandbox, and verified time when expiry is evaluated | Candidate tree, protected policy, claim definition, `previous_record_seal`, `predecessor_acceptance_seal`, and provider receipt |
| Adoption behavior | External debt snapshot or report-only rollout | No implicit debt for protected claims |
| Concurrency | Immutable input SHAs; no shared mutable state | Per-claim `previous_record_seal` and `predecessor_acceptance_seal` compare-and-swap |
| Automatic refresh | Absent | Absent; observations may be proposed but never accepted automatically |

## Non-negotiable invariants

1. **The status belongs to an exact candidate tree.** A result for one tree or commit MUST NOT be
   reused for another.
2. **Final-tree safety and attribution are different outputs.** Base comparison explains whether a
   finding is new. It cannot excuse an absolute invariant on the candidate.
3. **Policy consumes facts; it cannot rewrite them.** Broken, ambiguous, unsupported, waived, and
   debt-tolerated remain visible facts even when they do not block.
4. **Candidate-owned input cannot weaken its own checker.** Repository policy is raise-only in v0.
   Any protected-inventory or control-plane reduction is an unsuppressible failure. Removal of an
   unprotected document/reference remains the explicitly advisory coverage observation defined by
   scanner policy; v0 does not claim universal anti-deletion coverage.
5. **Incomplete never means clean.** Missing objects, unsupported requested capabilities, parser
   failure, resource exhaustion, stale event metadata, and internal failure cannot yield exit `0`.
6. **Automatic work is not review.** Scanning, co-change, initialization, Git authorship, and a
   valid digest do not create an attestation.
7. **Untrusted repository bytes are data.** Neither Markdown nor MDX, configuration, examples,
   plugins, Git attributes, submodules, or generated files execute in the blocking evaluator.
8. **V0 never writes.** It does not modify the checkout, index, refs, object database, cache,
   comments, checks, artifacts, branches, or an external store.
9. **No hidden partial support.** A capability advertised as unsupported produces an explicit
   `unsupported-capability` result when requested. It is never projected as resolved, current,
   verified, or clean.
10. **A green process result has a narrow sentence.** It means only “evaluation completed with no
    effective blocking finding in the disclosed scope.” It never means that documentation is true,
    fresh, complete, or in sync.

## Capability boundary

The exact candidate document set is delegated to
[scanner-v0-spec.md](./scanner-v0-spec.md#candidate-document-set), including its `.md`, `.mdx`,
`.markdown`, and extensionless Markdown names. The first blocking-capable slice is deliberately
small:

- tracked regular-blob documents in that exact built-in set in one Git repository;
- native Markdown links, reference links, and autolinks; raw HTML is opaque;
- same-repository targets in the evaluated co-versioned tree;
- deterministic path resolution; renderer-defined document anchors are retained but unsupported;
- base/candidate fact comparison and advisory impact observations;
- built-in policy plus an optional externally protected organization floor;
- human and deterministic JSON output.

Plain-text and inline-path inference are absent from the stable v0 fact model. The dossier's
research scripts produced non-contract experiment artifacts, but the authorized implementation has
no inference command, option, report, debt/waiver input, or exit behavior.

The following v0 responses are fixed:

| Requested or encountered feature | V0 result |
| --- | --- |
| Governed narrative declaration or acceptance | `unsupported-capability: governed-claim`; incomplete blocking evaluation, exit `2` |
| Any command/option outside the one published `assure check` grammar, including baseline/lock/refresh/migration/inference | `INVALID_INVOCATION`; no state change, exit `2` |
| Repository-authored waiver, skip, executable validator, or other unknown exact-policy field | `UNKNOWN_FIELD` plus `CONFIGURATION_INVALID`; exit `2`; never execute |
| Named shell command, generator, probe, example execution, or repository plugin syntax found in content | Opaque/non-executable content; it creates no capability request or success claim |
| Cross-repository, deployed, or live external selector found in prose | `external-out-of-scope`; no fetch and no passing relation |
| Ordinary external URL found in prose | `external-out-of-scope`; count it, do not fetch it, do not emit a passing relation |
| URI-looking bytes inside raw HTML | No reference occurrence is extracted; the HTML region is counted as opaque and no path resolution is reported |
| Renderer-defined heading anchor on an otherwise extracted native Markdown reference | `unsupported-reference-semantics`; path resolution may still be reported, but no anchor coverage is claimed |
| Same-repository URL pinned outside candidate/default-branch scope | `unsupported-version-scope`; exit `2` when explicitly protected/requested |
| LLM, embedding, translation-lag, OCR, bitmap semantics, or inferred-reference request | No v0 request surface; an extra option/field is `INVALID_INVOCATION` or `UNKNOWN_FIELD`, exit `2` |
| reStructuredText, AsciiDoc, or another unimplemented parser | Outside built-in scope when merely present; if explicitly opted in or requested, `unsupported-document-format` and no plain-text fallback presented as equivalent coverage |
| SARIF upload, PR comment, artifact upload, or acceptance button | Absent; JSON and logs only |

`external-out-of-scope` is valid only for a boundary the product discloses before scanning, such as live
external URL health. `unsupported` means the user or policy requested a capability inside the run
that the engine cannot establish. The latter makes a blocking run incomplete. This distinction
prevents “we did not implement it” from looking like “it passed.”

## Evaluation model

Every result is bound to this tuple:

```text
Evaluation(
  mode,
  event_kind,
  finality,
  repository_identity,
  candidate_ref,
  default_branch_ref,
  base_commit,
  base_tree,
  candidate_commit_or_synthetic_snapshot_input,
  candidate_tree_or_snapshot_digest,
  materialization,
  engine_digest,
  action_and_release_provenance,
  adapter_contract_descriptors,
  built_in_policy_version,
  profile,
  base_repository_policy_digest,
  candidate_repository_policy_digest,
  organization_floor_digest,
  debt_snapshot_digest,
  waiver_bundle_digest,
  execution_constraint_provenance,
  sandbox_provenance,
  evaluation_instant,
  trusted_time_source
)
```

An absent optional floor, debt snapshot, or waiver bundle is represented explicitly as `none`, not
as an empty digest. Base/candidate commits and traversed tree OIDs are type/hash verified before
use; regular/symlink blobs are verified when selected by document/control/target rules, and index
mode verifies its complete blob/symlink surface. Unselected commit-tree leaf blobs, ordinary parent
commits, and gitlinks are not opened.
The two refs are parsing/scope inputs, never substitutes for immutable snapshot identity. Mutable
tags are forbidden. `evaluation_instant` is the
explicit validity input defined in [machine-contracts.md](./machine-contracts.md#evaluation-instant-and-determinism),
not an ambient call to the wall clock.

The time controller binds its statement to
`HJ("assure/scanner-candidate-identity/v1", CandidateIdentityInput)`, where the exact preimage is the
resolved evaluation's repository, refs, mode/event/finality, complete base and candidate snapshots,
materialization, and sparse/index counts defined by the machine contract. It intentionally excludes
only the instant and trusted-time flag. A same-head run with a changed base, synthetic merge,
repository/ref, or materialization therefore has a different identity and cannot reuse the old
statement.

The evaluator computes these sets independently:

1. `BaseFacts`, using the base tree and base repository configuration;
2. `CandidateFacts`, using the candidate tree and candidate repository configuration;
3. `MetaFacts`, by semantically comparing both control planes;
4. `Attribution`, by comparing stable finding keys and fact values;
5. `CandidateSafety`, by applying invariant class, explicit adoption debt, trusted floor, and any
   externally authorized waiver to candidate facts.

The candidate is always scanned completely within the declared scope. Changed-file lists are an
optimization hint only and cannot remove files, references, inventory entries, or control files
from evaluation.

### Attribution

Reference-scope structural attribution has these values; pair-derived observation, document,
control, and analysis findings use `not-applicable`:

| Value | Definition |
| --- | --- |
| `introduced` | Stable finding key absent in base and present in candidate |
| `pre-existing` | The same key and equal policy-free fact digest exist in both |
| `resolved` | Key exists in base and not candidate |
| `unknown` | A fully evaluated base and candidate contain the same key with unequal policy-free fact digests |
| `not-applicable` | The finding is derived from the evaluation pair/control/error rather than a per-side reference fact |

An unknown attribution is never rewritten to `pre-existing`. Unavailable or invalid base data is a
fatal base-first analysis error with empty findings, not an attribution constructor.

Scanner v0 defines no per-kind improvement/worsening order. Its report schema therefore does not
emit attribution `improved` or `worsened`; those core values are reserved for a future finding kind
whose schema publishes an order. Debt mismatch separately emits `debt-worsened` without pretending
the underlying fact has an ordered magnitude.

Stable structural finding keys use the finding kind, document repository path, supported source
construct identity, and normalized target intent. They exclude line number and error wording.
Changing a broken target creates an introduced key unless the old finding is resolved; deleting a
document can resolve a structural finding but may separately violate protected coverage.

### Invariant classes

| Class | Candidate rule | Examples |
| --- | --- | --- |
| Absolute | Candidate MUST satisfy it regardless of attribution | Meta-findings, schema integrity, protected inventory, later governed claims |
| Ratcheted | Scanner-v0 debt tolerates only exact accepted fact equality until expiry; resolution needs no exception | Legacy explicit broken references |
| Advisory | Never changes v0 exit status unless an external floor promotes the exact deterministic kind | Inferred paths, impact observations, rename suggestions |

“Pre-existing” alone does not confer ratcheted status. A finding is adoption debt only when its key
and exact accepted fact digest occur in the externally protected debt snapshot.

## Exact GitHub event semantics

The acquisition wrapper passes provider fields as data arguments, never interpolates them into a
shell program, and validates them as full object IDs for the repository's Git object format. The
evaluator performs no fetch. Object acquisition happens before the sandboxed evaluation through a
commit-pinned checkout/acquisition action.

GitHub documents that `pull_request` checks normally run on the synthetic merge ref and that
`GITHUB_SHA` is its merge commit, while `merge_group` uses the merge-group SHA. GitHub also requires
the distinct `merge_group` trigger for required checks used with a merge queue. See the official
[event semantics](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#pull_request)
and [merge-group webhook guidance](https://docs.github.com/en/webhooks/webhook-events-and-payloads#merge_group).

| Mode | Base | Candidate | Mandatory verification | Safety meaning |
| --- | --- | --- | --- | --- |
| `pull_request` | `github.event.pull_request.base.sha` | `github.sha`, the synthetic PR merge commit | Candidate is a commit with exactly two parents; first equals base and second equals `github.event.pull_request.head.sha`; all trees exist | Exact synthetic base-plus-head tree represented by that SHA |
| `merge_group` | `github.event.merge_group.base_sha` | `github.event.merge_group.head_sha`, equal to `github.sha` | Both are commits; candidate's first parent equals base; event action is `checks_requested`; head ref and repository identity match | Exact combined queue candidate; hard invariants apply even when another queued PR caused the fact |
| Default-branch `push` | `github.event.before` | `github.event.after`, equal to `github.sha` | Ref is the configured default branch; both commits and trees exist; deletion is rejected | Exact before/after ref update, including non-fast-forward updates |
| Explicit commit pair | User-supplied full commit IDs | User-supplied full commit ID | Both resolve to commits; no branch-name fallback | Reproducible local/CI replay |

Provider identity/ref construction is exact. `Evaluation.repository` comes from the authenticated
event repository owner/name after the required ASCII-lowercase validation. For `pull_request`, set
`Evaluation.ref = "refs/heads/" + pull_request.base.ref`; for `merge_group`, copy the already-full
`merge_group.base_ref` and never use its queue `head_ref`; for default-branch push, copy the full
event `ref`. In all three, set `default_branch_ref = "refs/heads/" +
repository.default_branch`. Every constructed value must pass `ref-format-v1`. A default-branch
push additionally requires `ref = default_branch_ref`; PR/merge-group may target another protected
branch, but external floor/debt/waiver controls must bind exactly to that `Evaluation.ref`. Any
missing/mismatching short/full form is `INVALID_EVENT`, not a fallback to a checkout ref or branch
environment variable.

If a parent check fails, an event field is empty, an object is absent in a shallow checkout, the
repository identity differs, or event metadata and checkout disagree, the result is
the applicable closed `GIT_*` or `INVALID_EVENT` analysis code and exit `2`. The checker MUST NOT silently compare a merge base, current
`main`, `HEAD~1`, or whichever object happens to exist.

For a newly created default branch whose `before` value is the all-zero object ID, blocking
base/candidate comparison is unsupported; v0 defines no synthetic/external bootstrap snapshot
input. Recovery is report-only rollout or a provider-recorded administrative bypass until a later
event has a valid base. A deleted default branch has no candidate and is an unsupported event. Both
are non-passing results.
The `before` commit need not be a parent of `after`; force pushes are compared as the two exact
provider snapshots.

The checked status is valid only for the candidate SHA to which the provider attaches it. A
repository that can merge a different tree after the check does not satisfy this contract. A hard
deployment therefore MUST use a merge queue or a protected rule requiring the branch to be current
and the required status to apply to the exact merge candidate. Pull-request checks without that
provider guarantee are useful evidence but cannot be marketed as final-tree enforcement.
That final-tree binding is necessary, not sufficient: the separate control-epoch mechanism must
also invalidate or rerun the candidate when a base-independent control or expiry changes.

### Pull requests versus merge groups

Scanner-v0 JSON and human projections do not identify a PR head or apparent originating PR because
neither is in the evaluator fact model. A future provider wrapper may expose authenticated
acquisition metadata only after the request-wire RFC defines its out-of-band shape; it cannot alter
facts or attribution. Merge-group reports use ordinary attribution relative to the exact
merge-group base; `event_kind = merge-group` records that the combined queue candidate, not an
individual PR, was evaluated. In both modes, candidate safety is unchanged:

- an absolute candidate violation fails;
- every candidate ratcheted violation without exact active external debt or waiver fails;
- a matching, unexpired adoption-debt item remains visible and may be tolerated;
- advisory findings remain advisory.

Thus a queued PR can be innocent of a break and still receive a failed merge-group result. The
correct remedy is to rebuild/rebase the candidate or fix the combined tree, not to merge a tree
known to violate the invariant.

### Default-branch push

The push job is monitoring and defense in depth, not a substitute for a pre-merge check. It scans
the full `after` tree, validates all debt expiries and control-plane transitions from `before`, and
fails on every current absolute violation and every current deterministic structural failure not
covered by exact active external debt or waiver. A red push cannot undo the merge; it must page the
named owner through infrastructure outside scanner v0.

No path filters are allowed on pull-request, merge-group, or default-branch triggers. GitHub notes
that skipped required workflows can remain pending and that path-filter evaluation has limits; see
the [workflow syntax](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax-for-github-actions#onpushpull_requestpull_request_targetpathspaths-ignore).

For any future required lane, the one protected scanner job is unconditional for every supported
event: it has no job-level `if`, no candidate/event-controlled matrix exclusion, no
`continue-on-error`, and no dependency whose skip can skip the scanner. Cancellation, missing
output, neutral conclusion, or skipped execution cannot satisfy the controller. Provider `success`
is published only after the trusted wrapper parses one complete accepted envelope, verifies its
digest/identity, and observes the envelope's required-policy passing exit class; every other path
publishes failure or no success. This is necessary because GitHub documents skipped jobs as
reporting success and accepts successful/skipped/neutral check conclusions for required checks.

## Local snapshot semantics

The only local candidate mode is the staged index; required CI uses commit trees.
Both modes use only the exact primary non-bare repository form and no-follow handle acquisition
defined by the public CLI contract. Repository discovery, bare/linked-worktree `.git` forms,
configured alternates/replacements/grafts, promisor fetch, and administrative symlinks/reparse
points are not implementation choices. The wrapper ignores those extension mechanisms and fails
with the exact repository/index/object code when the primary `.git` directory cannot supply the
requested value.

### Index mode

`check --index --base <commit>` uses only stage-zero index entries and staged additions, deletions,
mode changes, and blobs. It never opens a worktree file for candidate content. It MUST:

- open the primary `.git/index` as one ordinary no-follow file, reject a declared/raw size over
  268,435,456 bytes with `git-index-bytes`, and parse it in-process under `git-index-v1`;
- reject any stage 1, 2, or 3 entry as `GIT_INDEX_UNMERGED`, exit `2`;
- reject intent-to-add as `GIT_INTENT_TO_ADD`; every blob/symlink row must name a readable,
  type-correct, hash-verified blob or use the applicable
  `GIT_OBJECT_MISSING`/`GIT_OBJECT_UNREADABLE`, exit `2`; a `160000` gitlink records its full commit
  OID but never looks it up in the superproject object database;
- read skip-worktree entries from their index blobs;
- ignore unstaged and untracked files;
- pin the initial handle/bytes as the evaluated candidate, then after the run reopen the current
  `.git/index` directory entry through the original `.git` directory handle with no-follow,
  boundedly read and independently parse it, and require raw bytes and logical projection equality.

`git-index-v1` implements versions 2, 3, and 4 of Git's pinned
[`index-format` 2.44.0 grammar](https://git-scm.com/docs/index-format/2.44.0): validate `DIRC`, version/count,
every fixed field and flag, v4 path-prefix compression, entry padding, extensions, and the final
SHA-1/SHA-256 checksum selected by `--object-format`. Unknown lowercase mandatory extensions,
split-index `link`, sparse-directory `sdir`, sparse-directory entry mode, malformed/duplicate paths,
and unsupported mode/flag combinations are `GIT_INDEX_INVALID`. Unknown uppercase optional
extensions and known cache accelerators are size-validated and skipped; they never replace the
entry table. Logical entries are path-byte sorted and prefix-free after parsing; no sparse expansion
or shared-index lookup occurs in v0. The final sample is never a reread of the initial held file
descriptor: Git may atomically rename a new index over that inode. After a valid initial sample,
failure to reopen the current entry, a missing/nonordinary/oversized/malformed final entry, or any
raw/projection inequality produces only `GIT_SNAPSHOT_CHANGED`; byte-identical atomic replacement
is accepted. No secondary final-sample index/resource code, unnamed digest, inode, or mtime
comparison substitutes. Raw optional-extension churn is conservatively a snapshot race even when
the logical projection is unchanged, but raw bytes are not candidate identity.
Index mode deliberately validates the complete materializable blob/symlink surface, not only later
selected documents/targets. Therefore an unrelated indexed blob over `git-object-bytes` is a typed
fatal index limitation; commit-pair mode need not open an outside-scope leaf blob. This asymmetry is
disclosed and covered by X-04 rather than hidden behind lazy implementation choice.

Git's index can contain multiple stages for unresolved conflicts, and such an index does not
represent one candidate tree. See [`git ls-files`](https://git-scm.com/docs/git-ls-files) and the
[Git user manual](https://git-scm.com/docs/user-manual#_the_index).

### Worktree mode is blocked

`--worktree` is not a v0 mode and returns `INVALID_INVOCATION` before filesystem traversal. The
current investigation did not close repository-admin and bare/nested-repository boundaries,
directory/file and skip-worktree conflicts, ignored-rule identity, bounded matcher work,
unreadable/mount/junction/reparse behavior, Windows native-name conversion, alias-versus-hardlink
semantics, or exact race/failure projection. The `gitignore-v1` vectors are research input to a
future worktree RFC only. No v0 implementation may fill those gaps with host Git/config behavior or
an implementation-selected retry.

### Sparse checkouts

Commit-pair CI reads Git trees and blobs, so sparse materialization is irrelevant when required
objects exist. Index mode discloses the number of skip-worktree paths. A relation that
requires filesystem behavior rather than blob bytes is unsupported in sparse local mode. The tool
does not expand the sparse checkout or mutate skip-worktree bits.

## Adoption debt ratchet

Scanner v0 has no committed debt file. A blocking rollout with existing failures has exactly two
safe choices:

1. fix all prospective blocking findings before enabling the required check; or
2. have an administrator create and review a `DebtSnapshot` for one exact adoption tree, then
   deliver it with the externally protected organization floor.

Without either, rollout remains report-only. Automatically treating every base finding as debt is
forbidden because it supplies no owner, expiry, or reviewed boundary.

Each debt item contains:

| Field | Rule |
| --- | --- |
| `finding_key` | One exact stable structural finding; wildcards are forbidden |
| `accepted_fact_digest` | Digest of the exact policy-free candidate fact in the complete adoption report |
| `key_input` | Complete canonical occurrence context that reproduces `finding_key` |
| `owner` | Externally resolved team or escalation identity |
| `reason` | Nonempty bounded text |
| `created_at` | UTC administrative evidence time |
| `expires_at` | Absolute UTC deadline; required |

The enclosing snapshot binds one exact `adoption_tree`, its complete
`adoption_report_payload_digest`, and `organization_floor_digest`. The adoption report's candidate
tree must equal that tree. Every evaluation reopens the adoption tree under the current adapter
contracts and reproduces every embedded multiplicity-one fact before treating current absence as
resolution; a parser/semantic regression is a binding failure, not debt disappearance. Scanner v0
has no numeric or severity-based debt measure.

The ratchet is deterministic. Every applicable debt defect is emitted; there is no hidden
first-error precedence:

- candidate finding absent from the debt snapshot: no debt treatment; its underlying configured
  disposition applies;
- matching key with a different policy-free fact digest: one `debt-worsened`, fail;
- matching key with exact accepted fact-digest equality before expiry: `debt-tolerated`, non-green
  fact, normally warn;
- expired item still present: one `debt-expired`, fail, even if the fact also worsened;
- finding gone: no debt application is emitted; the resolved structural detail and external debt
  inventory remain auditable, and no candidate state update is needed;
- snapshot item changed by repository content: impossible by construction; any attempted repository
  debt declaration is unsupported and fails.

Debt cannot cover analysis errors, unsuppressible meta-findings, protected governed claims, invalid
configuration, or unsupported capabilities. External debt-snapshot replacement is an
administrative control-plane event and must be audited outside the repository.

The debt snapshot's repository/ref/floor digest must equal the current verified evaluation tuple.
Any mismatch is `CONTROL_BINDING_MISMATCH`, incomplete, and exit `2`; unlike candidate-tree-scoped
waiver items, a debt snapshot is never treated as unrelated inactive inventory. A debt or waiver
input without a verified floor is likewise a binding error.

This is an exact-equality-or-resolution adoption exception, not final-tree correctness. Summaries
say “one broken reference tolerated as registered adoption debt,” never “candidate clean.”

## Policy composition

Disposition is the ordered lattice:

```text
record < warn < fail
```

Facts are classified first. Policy then computes an effective disposition without changing the
fact. Composition is:

1. built-in disposition for the exact known finding kind;
2. candidate repository strengthening, using `max`;
3. externally protected organization floor, using `max`;
4. adoption-debt treatment, only for eligible ratcheted facts;
5. externally protected waiver treatment, only when the floor marks that kind waivable;
6. an unsuppressible clamp for meta-findings and analysis integrity.

The exact kind/class/profile defaults, coverage-request variants, trace adjacency rules,
`configured_disposition`/`effective_disposition` boundary, exception applicability, and base-only
resolved projection are normative in
[machine-contracts.md](./machine-contracts.md#classification-and-policy-trace). A resolved finding
is record-only diagnostic output; candidate policy cannot make the absent candidate fact fail.

Repository policy in v0 is **raise-only**. It may add document roots, protected inventory entries,
or increase one deterministic kind enumerated by the scanner-policy schema to `warn` or `fail`.
Raw impact, inference, unsupported, analysis-integrity, and meta-finding dispositions are not
repository-configurable. It cannot:

- lower a built-in or base-repository disposition;
- exclude a built-in document class or protected path;
- add skips, waivers, debt, historical labels, mutable release refs, or arbitrary ignore globs;
- change parser, resolver, engine, or hash versions;
- loosen resource budgets;
- turn an error or unsupported result into a finding with a lower disposition.

Removing a previous repository strengthening is a semantic weakening even when the built-in
default would still pass the current corpus. The checker compares base and candidate policy, so a
candidate cannot erase the fact that it removed protection.

Protected inventory is checked over the union of base and candidate repository-policy paths plus
the floor inventory. A retained, newly added, or just-removed repository inventory path that is
absent, unsupported, or outside candidate document coverage emits unsuppressible
`coverage-reduced`; removing the rule also emits `policy-weakened`. This prevents deleting the
document and its inventory line together from going clean, and makes a newly asserted but already
unsatisfied obligation fail honestly.

Unknown fields, duplicate keys, unknown finding names, and unsupported schema versions are
configuration errors and exit `2`. Unknown future finding kinds from a newer result schema are not
matched by an old policy engine; the engine/schema mismatch exits `2`.

The evaluator follows the fatal validation order in machine-contracts: the verified floor's
resource limits activate before Git acquisition; repository and base snapshot then precede
candidate snapshot, and base policy precedes candidate policy. After any
non-`UNSUPPORTED_CAPABILITY` error, no later stage runs; fatal-incomplete output clears all
document, observation, and finding detail arrays and retains only the bounded error/provenance
projection. Consequently a repair PR receives no candidate diagnostic when its base is unreadable,
malformed, or over limit. Recovery from such a pre-existing bad base is an explicit administrative
bootstrap outside a successful evaluation—report-only investigation or a provider-recorded
one-time bypass, followed by a new valid base—not a candidate-controlled “fix implies green”
exception. This rule also covers base parser, Git-object, and resource failures.

### Externally trusted organization floor

An input is an organization floor only if all of these are true:

1. its bytes do not come from the base tree, candidate tree, worktree, PR artifact, cache, branch,
   tag, issue comment, or candidate-owned workflow input;
2. its expected semantic digest is supplied by an out-of-repository required workflow/ruleset and
   not by the checker action or another artifact;
3. the wrapper verifies the digest before evaluation;
4. the result records the floor digest and trust source;
5. failure to load or verify it exits `2`.

A file checked out from the repository is repository policy even if it is named `org-policy`.
`CODEOWNERS` is also repository-declared ownership, not an organization floor.

The only candidate GitHub deployment for a future blocking lane is an active
organization/enterprise required-workflow ruleset. Its authenticated rule state must select the
external source repository ID, workflow path, ref, and non-null full workflow commit SHA; the
controller must also resolve that path at the commit to an ordinary blob and bind its OID/raw
digest plus any reusable-workflow dependency closure. A required-status rule with an expected app
is not equivalent. GitHub organization rulesets are managed by organization owners; see
[GitHub's organization ruleset documentation](https://docs.github.com/en/organizations/managing-organization-settings/creating-rulesets-for-repositories-in-your-organization)
and the provider's
[organization-ruleset API](https://docs.github.com/en/rest/orgs/rules).

### Unsuppressible meta-findings

Every meta-finding below has built-in disposition `fail`. Repository policy, debt, local flags, and
waivers cannot lower it. An administrator may bypass the provider rule in an emergency, but the
tool result remains failed and the bypass is not rewritten as success.

A report-only deployment may publish these failed facts without making its status a required merge
gate. It does not downgrade them to `warn` or report a successful evaluation.

| Finding | Exact trigger |
| --- | --- |
| `policy-weakened` | Candidate lowers or removes any repository policy protection present in base |
| `coverage-reduced` | A base/candidate repository-inventory union member or floor inventory member is absent, unsupported, or outside candidate coverage |
| `control-plane-changed` | An exact externally protected repository control path changes or disappears |
| `debt-worsened` | A matching debt key has a different current fact digest |
| `debt-expired` | A matching debt item is at or past expiry |
| `waiver-invalid` | A selected waiver has one of the closed time/issuer/kind/owner/key/fact defects |

Those six names are the complete scanner-v0 control-plane set. Candidate attempts to add
repository-owned debt/waiver/config knobs are strict schema/configuration errors, not invented
lowercase findings. Ownership, governed lifecycle, acceptance, scope, and migration names in the
normative core belong to a future governed wire contract and MUST NOT appear in a v0 report.

Not every document deletion is a coverage failure. A non-protected ephemeral document may be
deleted and its inferred observations disappear. Protected coverage is defined by an owned,
externally supplied inventory or an explicit repository include whose removal is itself compared.
This prevents both “zero links means green” and a gameable “one link per page” quota.

The checker cannot defend itself if a candidate can delete the entire workflow before it runs. An
active organization/enterprise ruleset workflow that selects the exact external source repository,
branch, and workflow file—or a provider mechanism proving equivalent source identity—is therefore
necessary for strong bypass resistance. A required status name, an expected-app selector, or a
repository-only workflow is insufficient: those provide producer/visibility constraints without
binding the exact workflow and event lane.

## Waivers

Scanner v0 accepts no candidate-owned waiver syntax and has no adjacent `skip` directive. The only
v0 waiver source is a `WaiverBundle` delivered and digest-protected with the organization floor.
Repositories without that channel have no scanner waiver; they use the provider's visible
administrative bypass and remediate afterward.

A waiver contains:

- one exact stable finding key and one exact candidate tree;
- nonempty reason;
- accountable owner and independent issuer;
- creation evidence supplied by the authenticated external bundle delivery, plus absolute UTC
  `not_before` and `expires_at`;
- residual disposition exactly `warn`; a fail-residual no-op is not called a waiver;
- allowed repository and branch scope;
- floor digest and waiver ID, whose durable audit identity is scoped by the verified bundle digest.

Wildcards, “all findings in this PR,” mutable refs, indefinite live waivers, and waiving analysis
errors or meta-findings are forbidden. A historical document is a scope classification, not an
infinite waiver.

The future blocking CI wrapper supplies and records one trusted UTC `evaluation_instant`. It accepts time
only through the externally controlled statement contract below, never from candidate files,
workflow environment, a provider display field, or the scanner's wall clock. The authorized local
CLI has no waiver input lane: any extra control option/input is `INVALID_INVOCATION`, and waiver
provenance remains exactly `none`. A future wrapper that supplies an expiry-bearing waiver without
a verified trusted instant is incomplete and exits `2`; there is no waiver-validity `unknown` state.

Schema/digest/canonical-order/ID/duplicate-target validation covers the complete bundle before item
selection. The bundle repository/ref and floor digest must exactly equal the current evaluation;
a mismatch is `CONTROL_BINDING_MISMATCH`, incomplete, and exit `2`, not inactive inventory. Only
items for another valid `candidate_tree` are inactive. For a selected item, the evaluator checks
time, authorized issuer, owner/issuer distinction, and exact key/fact body and digest. Each failed
check constructs exactly one blocking `waiver-invalid` with its closed defect rule ID; all
applicable defects are evaluated in defect-code order, then findings are emitted in global canonical
finding-key order, and the waiver has no suppressive effect. A selected item
may therefore produce more than one explicit invalidity fact; there is no hidden first-error
precedence.

Applying a waiver changes only effective disposition. The underlying broken or changed fact plus
bundle-scoped waiver ID, owner, issuer, reason, fact/candidate binding, and times remain in JSON and
human details; the summary retains the exact applied-waiver count. Application is only
`fail -> warn`; an otherwise valid item facing `warn`/`record` is inapplicable and is not counted as
“waived.”

### Trusted-time issuance and replay boundary

The external required-workflow controller issues a strict `TrustedTimeStatement` after authenticating
the current provider repository, destination ref, candidate evaluation identity, run ID, and run
attempt. The report recomputes
`HJ("assure/scanner-trusted-time-statement/v1", TrustedTimeStatement)` and requires the statement
instant to equal `evaluation.evaluation_instant`. The same run ID/attempt and candidate-evaluation
identity digest must appear in the provider-verified sandbox receipt. These are provider
API/runtime facts acquired by the trusted wrapper; candidate output cannot nominate them or upgrade
a statement to `verified`.

Issuance and expiry are whole-second UTC values and obey
`evaluation_instant < valid_until <= evaluation_instant + 600 seconds`. Immediately before it
accepts the report and publishes the required status, the wrapper checks with the controller clock
that `evaluation_instant <= current_time < valid_until`, rechecks every applied exception expiry
against `current_time`, and reacquires the current external expected control/constraint digests and
the authenticated ruleset/workflow-source identity digest.
An exception that expired during evaluation or any pre-publication control rotation rejects the
result and forces a fresh run. There is no grace/skew window. A statement
for another repository/ref, base/candidate/materialization identity, provider run, or attempt; a
future issuance time; an expired statement; or a TTL over ten minutes is a replay/verification
failure and exits 2. A deterministic rerun in the same authenticated attempt may reuse the exact
statement only before `valid_until`; a provider rerun must obtain a new statement. The protected
status remains bound by the provider to the current candidate SHA independently of this time proof.

That ordinary provider binding is insufficient after publication because base/control identity and
expiry are not generally part of the candidate-SHA/status-context key. Required enforcement stays
closed until X-07 and a control-epoch/provider-freshness RFC prove an authenticated merge-time
comparison plus invalidation/rerun on base movement, expiry, revocation, or floor/debt/waiver/
constraint/ruleset/workflow-source rotation. A stale green cannot authorize merge; without that
controller the report is
only a point-in-time evaluation artifact.

## Ownership and reviewer trust

Ownership has four distinct roles:

| Role | Responsibility |
| --- | --- |
| Document owner | Maintains the user-facing claim or reference |
| Evidence owner | Understands the code/schema/behavior selected as evidence |
| Policy owner | May strengthen repository coverage; cannot self-authorize weakening |
| Waiver issuer | Accepts temporary operational risk and owns expiry follow-up |

Scanner v0 reports only the owner/issuer strings carried by authenticated external debt or waiver
items; it does not parse `CODEOWNERS` or expose repository-declared document/evidence ownership.
Future governed records may add typed ownership after Gate C. Neither stage can infer that a person
read or approved anything from Git author, committer, PR author, workflow actor, a
`CODEOWNERS` match, and text written into an acceptance record are not reviewer authentication.

Acceptance trust levels are fixed:

| Level | Evidence |
| --- | --- |
| `self-asserted` | Repository contains a canonical local acceptance event; actor identity and attention are not proven |
| `provider-verified` | Provider API/service proves eligible approval for the exact acceptance candidate that introduced the committed seal and the owner policy in force for that acceptance |
| `service-signed` | A separated service signs a provider-verified receipt over the exact committed acceptance |

V0 observations have machine-extraction provenance; this is never acceptance trust.
`repository-reviewed` is a separate review-context fact saying ordinary provider branch review was
required. It does not prove eligible claim review and cannot upgrade `self-asserted` trust. Governed
acceptance is unsupported in v0.

GitHub obtains `CODEOWNERS` from the base branch and can require a code-owner approval, but one of
several listed owners is sufficient. Protecting `CODEOWNERS` itself is also necessary. See
[GitHub's CODEOWNERS documentation](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/about-code-owners#codeowners-and-branch-protection).
Consequently, a future claim requiring both document-domain and evidence-domain approval cannot be
proven by a single ordinary CODEOWNERS rule. It requires separate provider rules or the governed
service.

For `provider-verified` acceptance, governed v1 MUST establish all of the following at acceptance
time:

- approval applies to the exact acceptance-candidate SHA, committed seal, and claim-definition digest;
- reviewer is not the acceptance author when separation is required;
- reviewer belonged to an eligible owner team at that time;
- required document and evidence owner rules are independently satisfied;
- approval was not dismissed and no later change to the seal, definition, endpoints, accepted
  projections, reviewer rule, or provider protection context invalidated it; an unrelated commit
  alone does not;
- provider repository identity and event delivery are authenticated;
- the receipt binds the exact committed `AcceptanceSeal`, whose event names its
  `predecessor_acceptance_seal`.

If any fact cannot be proven, acceptance trust stays `self-asserted`; review context may separately
be `repository-reviewed`, but trust is never upgraded by inference. A blocking governed narrative
claim requires at least `provider-verified`; a current self-asserted acceptance remains report-only.

## Read-only CI posture

The scanner workflow runs on `pull_request`, `merge_group`, and default-branch `push` with no path
filters. Its job-level token permissions are `contents: read` only; public repositories SHOULD use
`permissions: {}` when checkout can operate without a token. It receives no repository,
environment, organization, cloud, package, signing, or deployment secrets.

The checkout/acquisition action and scanner action are pinned to full commit SHAs. Checkout uses
`persist-credentials: false`. No candidate-local action (`uses: ./...`), package installation,
repository script, docs build, Git hook, filter, generator, or test command runs in this job.

GitHub recommends least-privilege tokens, warns against combining untrusted checkout with
`pull_request_target` or privileged `workflow_run`, and says a full-length commit SHA is the only
immutable Action pin. See the official
[secure-use reference](https://docs.github.com/en/actions/reference/security/secure-use).

V0 therefore prohibits:

- `pull_request_target` for scanner evaluation;
- `workflow_run` consumption of an untrusted PR result in a privileged job;
- issue-comment commands;
- PR comments, labels, review submission, branch pushes, or check-run API writes;
- SARIF or artifact upload from the untrusted evaluator;
- shared writable Actions caches;
- long-lived or trusted self-hosted runners for fork pull requests;
- network calls after exact Git object acquisition.

The Actions platform may need network access to acquire the pinned action and exact Git objects.
That is a separate trusted acquisition phase. The evaluator itself is network-independent and MUST
run only after credentials are removed and acquisition handles are closed. Any Git
acquisition/materialization helper runs before this boundary and its exact output becomes scanner
input.

The blocking sandbox enforces the literal `scanner-v0-zero-capability-v1` descriptor: repository,
object database, and control inputs are read-only; temporary storage is fresh, private, capped at
67,108,864 bytes, and destroyed afterward; network, DNS, inherited sockets, child processes,
repository processes, Git/LFS/filter/helper execution, and shared caches are denied; and no secret,
credential, token, agent socket, or writable alternate object store is mounted. The selected engine
may read only explicit snapshot/control inputs and the private temporary directory and may write
only stdout/stderr and that directory. An attempted denied operation fails the run; it is not
silently ignored.

`scanner-process-env-v1` is an empty evaluator environment. The trusted launcher passes fixed
arguments/handles and clears `PATH`, `HOME`, `TMP*`, locale/timezone, `GIT_*`, provider/CI, proxy,
token, and dynamic-loader variables before exec. UTF-8, UTC, and ordering behavior are compiled
engine rules. The action launcher and acquisition wrapper are outside this boundary, but they MUST
NOT forward ambient variables or runtime files. A need for another environment variable, plugin,
workspace library, configuration file, or sidecar requires a new sandbox and engine contract.

The report embeds descriptor, digest, assurance, enforcement source, and optional verification from
[machine-contracts.md](./machine-contracts.md#sandbox-and-process-environment). A local process has
`self-asserted/local-process/null`. `provider-verified` requires a verification constructed by the
external required workflow, current authenticated provider run ID/attempt, and exact current
execution-constraint, sandbox-descriptor, and candidate-evaluation-identity digests. Its run and
evaluation identities must also match trusted time when time is present. Repository output cannot
assert this proof. Without that exact provider-verified profile, the run cannot be registered or
described as the required stable check.

A plain process is always self-asserted. Provider verification is valid only for the closed
`oci-rootless-sandbox-v1` container or `microvm-sandbox-v1` VM mechanism matching the descriptor;
the controller must attest the actual network namespace/firewall, read-only mounts, process-denial
policy, bounded private storage, and teardown. If a platform cannot enforce one of those mechanisms,
the required run is unsupported and exits `2`; it may not reuse the provider-verified label for an
ordinary hosted-runner process.

The evaluator core writes JSON only to its result stream and diagnostics only to its diagnostic
stream. The local public CLI may parse that complete result in-process and render the non-wire
`human-atom-v1` projection to stdout when `--format human` is validly selected; it never renders a
partial evaluator stream or re-analyzes repository bytes. A trusted wrapper MAY
translate a complete result into a job summary after the evaluator exits. It MUST NOT pass
repository text through workflow commands without escaping, and it MUST NOT use candidate text as
shell source.

## Untrusted-input threat model

The attacker may control every byte and name in both candidate and historically accepted
repository content, including:

- Markdown, MDX, front matter, HTML, JSX, expressions, imports, exports, code fences, and comments;
- configuration, policy-shaped files, declarations, generated files, Git attributes, ignore files,
  and submodule metadata;
- paths containing whitespace, newlines, terminal controls, Unicode confusables, or very long
  segments;
- link targets, fragments, percent escapes, repository URLs, and branch-like text;
- malformed Git objects, large blobs, deep trees, high fan-out, duplicate definitions, and parser
  edge cases;
- event display strings, PR titles, branch names, author names, and commit messages;
- cache entries and artifacts produced by an untrusted PR job.

Trusted components are narrower: the pinned action tree and binary digest, externally protected
execution constraint, runner/kernel sandbox boundary, authenticated sandbox/time controller facts,
provider-supplied event payload after consistency checks, exact Git object IDs, and any externally
digest-protected floor. The base branch is not trusted parser input merely because it merged
earlier.

### MDX

MDX can contain JSX, JavaScript expressions, and ESM imports/exports, including whole programs in
expressions; see the official [MDX syntax description](https://mdxjs.com/docs/what-is-mdx/).
Therefore the evaluator parses source as data and MUST NOT:

- import or evaluate an MDX module;
- invoke the repository's MDX compiler, Next/Fumadocs configuration, remark/rehype plugins, or
  component code;
- evaluate JSX expressions or component properties;
- expand includes, transclusions, imports, or generated fences;
- treat strings inside ESM, JSX expressions, code, or comments as prose references.

V0's MDX adapter recognizes ordinary Markdown constructs outside opaque ESM/JSX/expression
regions. JSX attributes are unsupported unless a later, fuzzed parser adds an explicit literal
`href`/`src` grammar. Every file reports opaque-region counts. A protected claim cannot assert full
link coverage for an opaque region; requesting that capability returns unsupported.

The adapter handles nested braces, strings, templates, comments, and JSX structure with a real
bounded tokenizer/parser, not regular-expression masking. Front matter is explicitly recognized or
masked before section assignment. Parser panic, exception, abort, or invalid span is an analysis
error. A non-returning parser is terminated by the outer watchdog and yields no accepted result.
Compiling with `@mdx-js/mdx` is permitted only as a test oracle over fixtures in an
unprivileged development job, never as the blocking evaluator.

If a Rust implementation uses native grammars, `unsafe_code = "forbid"` in owned crates does not
cover unsafe dependencies or bundled C. `panic = "abort"` is incompatible with classifying a
parser panic in-process. Under `scanner-v0-zero-capability-v1`, child processes are denied, so the
blocking evaluator MUST unwind and catch a panic where sound; a resource-limited parser worker
requires a future sandbox/engine contract and is not a v0 alternative. The real dependency boundary
is fuzzed, not only the wrapper.

### Repository command execution

The public local scanner parses the primary index, loose objects, packs, trees, and commits
in-process under the contracts above. It invokes neither Git nor a shell. The evaluator does not invoke shell, external diff,
text-conversion, clean/smudge filters, credential helpers, hooks, submodule commands, or Git LFS.
Repository configuration cannot select an executable.

If a future request-wire permits a Git subprocess during trusted acquisition, the pre-sandbox wrapper fixes
the executable path, sanitizes `GIT_*` environment overrides, disables optional locks, hooks,
external diff, filters, and file system monitors, and passes object IDs only after type validation.
Any path records use NUL-delimited plumbing. It must still materialize inputs whose identity and
semantics match the separately versioned request contract; host Git behavior is not an implicit v0
fallback. It closes the process before entering the evaluator boundary and does not update refs,
the index, replacement objects, alternates, or object promises. The evaluator itself never starts Git.

Human output escapes control characters and never emits candidate-provided ANSI sequences.
Machine output uses JSON escaping and preserves the raw path as a reversible UTF-8 string only
after encoding validation. Human diagnostics apply `human-atom-v1` to permitted path/identifier
scalars and quote at most 200 Unicode scalar values; they never include source excerpts, raw link
destinations, URL userinfo, or query values.

## Paths and repository object kinds

Candidate resolution operates over the virtual Git tree, not by joining untrusted text to an OS
path. UTF-8 path bytes are compared exactly. The evaluator does not apply Unicode normalization or
case folding, so two Git paths with different bytes remain different even on a case-insensitive
worktree.

URL paths are percent-decoded exactly once. Invalid escapes, decoded NUL, backslash-as-separator,
absolute filesystem syntax, and a dot-segment traversal above repository root produce
`invalid-reference`. Query and fragment components are parsed by their specific grammar; they are
not repeatedly trimmed until a path happens to exist.

| Git entry/input | V0 handling |
| --- | --- |
| Regular blob, mode `100644` or `100755` | Eligible; executable bit is reported and remains part of metadata |
| Symlink blob, mode `120000` | Never follow; document is unsupported; target Resolution is `unsupported/symlink-entry` and the boundary Finding is `unsupported-target-kind` |
| Gitlink/submodule, mode `160000` | Never initialize, fetch, or recurse; target Resolution is `unsupported/gitlink-entry` and the content boundary Finding is `unsupported-target-kind` |
| Tree/directory | Exact tree existence may resolve a directory link; index-page or route semantics require an explicit supported adapter |
| Git LFS pointer | Parse the bounded current/legacy, extension-preserving recognition grammar; path is `resolved/exact-path` with `lfs-pointer-only`, while required content emits an unsupported-target boundary |
| Path outside the `RepoPath` domain (including invalid UTF-8 or literal backslash) | `UNREPRESENTABLE_PATH` with full bounded raw byte hex; the scan cannot claim complete scope and exits `2` |
| Invalid UTF-8 non-document target path | Preserve a hex path identifier in the error; it cannot be selected by a UTF-8 document target |
| Sparse index directory | `GIT_INDEX_INVALID`, exit `2`; v0 does not expand sparse directories or read shared-index backing data |

Git submodules are separate repositories and are not checked out by default, while LFS stores a
small pointer rather than the target bytes. See the official
[Git submodule documentation](https://git-scm.com/docs/gitsubmodules) and
[Git LFS specification](https://github.com/git-lfs/git-lfs/blob/d72db1e533a1d6ee5543e02e9f8ccac97e0fcd34/docs/spec.md).

Same-repository GitHub URLs are parsed as repository references only when owner and repository
match the trusted repository identity. A URL split at the trusted default ref resolves against the
candidate tree only when `ref = default_branch_ref`; on a PR targeting another branch, a
default-only split is `unsupported-version-scope` and is never remapped. A foreign repository is
external. A URL pinned to a
different commit requests historical scope, which is unsupported in v0 rather than silently
falling back to the candidate.

## Resource budgets

Scanner v0 freezes one deterministic engine ceiling set shared identically by `observe` and
`enforce`; profile selection changes finding disposition, never resource accounting or fatality.
Repository content cannot raise these ceilings. A future verified organization floor may lower
them for either profile only with the consequence that hitting a limit still exits `2`; the current
local command has no external floor. Raising a hard ceiling requires a new reviewed engine profile
and benchmark.

| Resource | Scanner-v0 engine ceiling |
| --- | ---: |
| One inflated Git object/delta base | 128 MiB |
| One selected compressed loose object or packed-entry interval | 256 MiB |
| Aggregate selected compressed object storage per evaluation | 2 GiB |
| Entries examined in `objects/pack` | 8,192 |
| Pack/index pairs in the primary object database | 4,096 |
| One pack index | 512 MiB |
| Aggregate pack-index bytes per evaluation | 1 GiB |
| Git delta reconstruction depth | 128 |
| Raw staged index | 256 MiB |
| Git tree entries per snapshot | 1,000,000 |
| Documents per snapshot | 100,000 |
| One raw control JSON input | 16 MiB |
| One externally protected repository control blob | 16 MiB |
| Aggregate protected control bytes per snapshot | 64 MiB |
| Repository-policy entries | 100,000 |
| Adoption-debt items | 100,000 |
| Waiver items | 100,000 |
| Raw path length | 4,096 bytes |
| One document blob | 4 MiB |
| Aggregate document bytes per snapshot | 512 MiB |
| One referenced regular-file target blob | 16 MiB |
| Aggregate referenced target bytes per snapshot | 512 MiB |
| Raw link destination | 16 KiB |
| Parser nesting | 256 |
| Parser nodes per document | 250,000 |
| Parser nodes per snapshot | 5,000,000 |
| Extracted references per document | 4,096 |
| Extracted references per snapshot | 1,000,000 |
| Organization policy/inventory entries | 100,000 |
| Complete findings | 100,000 |
| Typed analysis errors retained | 64 |
| Machine JSON | 64 MiB |
| Human detailed findings | First 200 in deterministic key order, plus exact totals |
| Evaluator managed live allocations | 768 MiB |
| Private temporary storage | 64 MiB (67,108,864 bytes) |

Wall time and host RSS are operational watchdogs, not semantic resources: the trusted wrapper kills
the whole evaluator after 120 seconds and the sandbox caps physical memory at 1 GiB. A kill, OOM,
or non-returning parser yields no accepted envelope; the wrapper fails the required status. Those
events cannot produce a canonical partial error whose presence depends on runner speed. The engine
instead reports deterministic byte/node/count limits and tracks at most 768 MiB of live allocations
through its bounded allocator. E0 may choose only an in-process parser with no callbacks/plugins and
documented termination or cooperative deterministic work checks; the maximum valid 4 MiB fixture
must remain below two seconds on every supported release platform. If that cannot be proved under
X-03/X-05, the parser/format is not eligible—adding an unrestricted worker process is not a fallback.

V0 has these hard zero budgets:

- network requests: `0`;
- repository-controlled processes: `0`;
- include/transclusion/import depth: `0`;
- repository regular expressions: `0`;
- transitive graph traversal: `0`;
- automatic rename/retarget operations: `0`;
- workspace writes: `0`.

Byte, node, reference, and entry caps are checked before allocation or expansion where possible.
The compressed-stream, pack-directory-entry, and aggregate-index caps bound bytes/names consumed
before inflation or index selection; padding-only deflate streams and ignored junk names therefore
cannot defer failure to the wall-clock watchdog.
Scanner-v0 policy inputs use exact document paths and exact tree roots, not globs or regex. A cycle
cannot arise because includes and transitive graph traversal are absent.

The reference budgets count both ordinary extracted link/image/autolink observations and every
reserved `assure:` link-reference definition. A governed definition consumes budget before its
unsupported control-state source is retained; it cannot bypass the 4,096 per-document ceiling by
being classified as a control rather than a reference.

The 16 MiB raw-byte ceiling is checked before JSON parsing for each repository policy, organization
floor, debt snapshot, and waiver bundle. The floor's own hard ceiling is engine-fixed; after the
floor is verified, it may tighten the byte and entry ceilings for subsequently parsed repository
policy/debt/waiver inputs but can never raise them.

`organization-policy-entries` is the sum of the item counts in the floor's
`minimum_dispositions`, `protected_inventory`, `protected_control_paths`,
`waivable_finding_kinds`, `authorized_debt_owners`, `authorized_waiver_issuers`, and
`resource_limits` arrays. The engine rejects the floor before retaining more than 100,000 combined
items even if every individual schema maximum is satisfied. If the floor declares a tighter
`organization-policy-entries` value, its own combined count must fit that value after parsing or the
floor is self-inconsistent and exits `2`.

`repository-policy-entries` is the sum of `document_includes`, `protected_inventory`, and
`finding_dispositions`, checked independently for base and candidate before retaining more than the
effective limit. Array-local schema maxima do not permit their sum to exceed it.

The sandbox mount cap remains 64 MiB. A tighter floor
`private-temporary-storage-bytes` value is the evaluator's usage allowance inside that mount;
crossing it emits the resource error even though the mount has physical capacity left. The
effective retained-error limit is `min(64, floor limit)` and is never below 1; overflow keeps
`limit - 1` ordinary errors plus the sentinel. The effective machine-JSON limit is never below the
64 MiB reserved small-error-envelope minimum. The floor therefore cannot tighten the 64 MiB hard
wire ceiling: the reservation covers the maximum schema-valid embedded release manifest after JSON
escaping plus 64 retained errors. E0 must keep a generated maximal-shape golden below it.
A floor attempting lower reserved minima is schema
invalid rather than a request for silent/no-output failure.

The evaluator reads the Git object header before hashing every referenced regular file, including
code, images, archives, and other non-document blobs. The per-target and aggregate target caps
apply to all of them, not only Markdown anchor targets. Crossing either cap is a bounded
`RESOURCE_LIMIT_EXCEEDED` incomplete run; a large blob cannot force unbounded hashing.

Every externally protected repository control path whose raw digest is compared is independently
bounded by `selected-control-blob-bytes`, and their per-snapshot sum by
`aggregate-selected-control-bytes-per-snapshot`. The object header is checked before hashing. These
limits apply even when the same blob is neither a document nor a reference target; a blob that is in
several classes must satisfy every applicable per-file cap but its bytes are charged once to each
applicable aggregate ledger.

Crossing any deterministic analysis ceiling emits the exact closed resource/output error with the limit and observed lower
bound, marks the result incomplete, and exits `2`. The scanner never stops early and then reports
the scanned prefix as clean. Human presentation may show only the deterministic first 200 findings
after complete analysis. JSON is never silently truncated; inability to emit the complete bounded
result produces a small error envelope and exit `2`.

The 120-second wall ceiling is a safety limit, not a latency objective. Blocking promotion still
requires measured incremental latency and memory on representative repositories.

## Results and fail-closed behavior

The process contract is:

| Exit | Meaning |
| --- | --- |
| `0` | Evaluation is complete, no effective `fail` finding exists, and no requested capability is unsupported |
| `1` | Evaluation is complete and at least one effective `fail` finding exists |
| `2` | Configuration, schema, Git state, parser, sandbox, resource, output, or internal failure prevented a trustworthy complete evaluation |

Every usage or capability error maps to exit `2`; the CLI framework MUST NOT expose another public
exit class. A signal termination, out-of-memory kill, missing result, malformed/partial JSON,
candidate-ID mismatch, wrapper timeout, or unexpected exit code is a failed required check. There
is no `continue-on-error`, fallback success step, or `|| true`.

The wrapper accepts a result only when it can parse one complete schema version and verify the
expected candidate tree, engine digest, floor digest, completeness flag, finding count, and the
payload-only digest defined in [machine-contracts.md](./machine-contracts.md#digest-registry). Text
printed before a crash is never interpreted as a result.

Each summary includes at least:

- base and candidate tree IDs and event/finality mode;
- scanner engine digest, scanner-action/release provenance, adapter descriptors/digests, and
  built-in policy version;
- discovered, scanned, unsupported, excluded-by-built-in-scope, and unlinked document counts;
- explicit local, external-out-of-scope, unsupported-reference, and separate frontmatter,
  opaque-MDX, and opaque-HTML region/byte counts;
- introduced, pre-existing, resolved, debt-tolerated, waived, and unknown-attribution counts;
- blocking, warning, analysis-error, and unsupported-capability counts;
- floor, debt-snapshot, waiver-bundle, execution-constraint, sandbox, and trusted-time provenance.

A zero-link document contributes to discovered, scanned, and unlinked counts. It does not create a
passing relationship. An external or opaque construct contributes to its disclosed category. It
does not become green by omission.

## Action and binary provenance

Every third-party Action, including checkout, is pinned to a reviewed full commit SHA. Mutable tags
such as `@v1`, release branches, and floating container tags are forbidden. Organization settings
SHOULD enforce full-SHA pins where available.

The scanner release is a pinned action tree with platform binaries present at that same commit. A
required workflow validates it as data before executing any target byte. The trusted bootstrap
MUST:

1. choose from a closed OS/architecture table;
2. resolve the reported action commit to its exact reported tree;
3. resolve `manifest_path`, selected artifact `tree_path`, and every manifest-listed runtime path
   as regular non-symlink blobs in that tree;
4. parse that manifest blob and require exact equality with the embedded strict release manifest;
5. verify its semantic manifest digest, every runtime-file mode/checksum, the selected artifact's
   plain SHA-256, and domain-separated engine digest against the exact blobs;
6. refuse symlinks/gitlinks, download, self-update, package installation, sidecar/plugin discovery,
   repository helpers, or fallback to a PATH binary;
7. report action repository/commit/tree, manifest path/body/digest, selected artifact path/platform,
   complete runtime-file closure, binary/engine digests, build source, complete sorted
   dependency-lock path/member-digest preimage, and its set digest;
8. fail before scanning on any mismatch.

The external `ExecutionConstraintDescriptor` pins action repository, object format, commit, tree,
manifest path/digest, expected OS/architecture, trusted bootstrap contract/digest, and expected
provider status name. That name is a context check, not workflow-source identity or merge
authorization. Its identity is
`HJ("assure/scanner-execution-constraint/v1", ExecutionConstraintDescriptor)`. In a required
future `stable-v1` run, the report's verified descriptor MUST equal every corresponding action-provenance
field; the provider MUST confirm that the current destination ref requires the external
workflow/ruleset which owns that exact check context. A matching name emitted by candidate workflow
code is insufficient. Because descriptor v1 has no exact workflow-source fields, it is insufficient
on its own: the provider request-wire/control-epoch RFC must additionally bind the active external
source repository ID, workflow path, ref, non-null full workflow commit SHA, resolved ordinary
workflow blob OID/raw digest, immutable reusable-workflow dependency closure, event/ref
applicability, and merge-time freshness. The
sandbox verification binds this exact descriptor digest. Constraint `none` is limited to
local/disposable experimental runs and cannot produce the required stable status.

The bootstrap derives the actual runner OS/architecture from the protected runtime, never an
environment value or candidate field, and requires equality with the constraint, selected manifest
artifact, action provenance, and sandbox verification. Because OS ABI is outside the manifest, a
producer-selected platform label without this binding is invalid provenance.

The required workflow never invokes the target with `uses: owner/action@sha`; doing so would execute
target metadata before validation. Its separately protected bootstrap is verified against the
constraint, acquires the action tree as data, and only then directly execs the verified native
engine inside the sandbox. Root `action.yml` is a closed JCS-JSON action-metadata shape; general
YAML, aliases/tags/merge keys, pre/post hooks, composite/container execution, and unlisted keys are
rejected. The declared Node launcher is manifest-listed but is not executed in the required path.

The action commit's root `action.yml`, launcher, manifest, and the artifact's complete sorted
`runtime_files` form the reviewed action runtime closure. Metadata and launcher code may only select
a closed platform row, perform the checks above, and exec the resolved artifact. There is no
pre/post hook, mutable container, download, package manager, repository script, runtime plugin, or
unlisted action-tree/workspace file. The engine may consume explicit read-only snapshot/control
inputs and private bounded temp only. OS-owned kernel/system-library ABI is honestly outside the
manifest and remains covered by platform equivalence tests; every non-platform library/data file
must be listed. The exact manifest/lock domains and cross-field bindings are defined in
[machine-contracts.md](./machine-contracts.md#adapter-observation-and-build-provenance). Every other
third-party action still has its own full reviewed SHA pin; scanner provenance does not absorb it.

The release pipeline builds from a protected source commit with locked dependencies, publishes
checksums and an SBOM, and produces signed provenance. GitHub artifact attestations can establish
the build workflow and artifact subject and can be verified by consumers; see the official
[artifact-attestation documentation](https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/use-artifact-attestations).
An attestation complements, but does not replace, the consuming workflow's exact digest pin and
review of the builder identity.

The action source, release builder, dependency updates, and embedded binary manifest receive
security review. A version bump that changes parser or resolution semantics is an engine change,
not a routine cache refresh.

V0 uses no shared cache. In-process memoization dies with the run. A future read cache must be
content-addressed by every input in the evaluation tuple, read-only to untrusted PRs, and treated as
untrusted until its value digest is recomputed. No cache is allowed to change completeness.

## Operations and concurrency

### Scanner v0

Scanner runs are embarrassingly parallel because all inputs are immutable and there is no shared
state. A base branch moving during a run does not alter the result; it creates a new candidate that
must receive a new run.

Workflow concurrency distinguishes the full `CandidateIdentityInput` digest and every expected
floor/debt/waiver/execution-constraint digest, profile, and engine release digest. Cancellation is
permitted only after the external controller proves this complete semantic input tuple equal; no
request wire means no stable cross-process cancellation identity yet. Equal candidate SHAs with
different bases or control epochs are not duplicates; a run for a different merge-group candidate
or default-branch push is never canceled merely because it shares a branch
name. Merge-queue invalidation is left to the provider; `cancel-in-progress: false` is the safe
default for merge-group and push events.

No post-merge refresh job exists. There are no bot commits, branch-protection bypasses, refresh
windows, or lock-only skip markers. The default-branch scanner observes the merged tree without
writing it.

### Governed v1 per-claim state

If the governed stage passes its separate product and schema gates, one global `assure.lock` is
still rejected. The logical state unit is one immutable `ClaimId`:

- authored claim definition is separate from observations and acceptance;
- one canonical record stores the claim's current accepted event, `previous_record_seal`, and
  acceptance event's `predecessor_acceptance_seal`;
- logical state is keyed by SHA-256 `ClaimKey`, never mutable document location;
- a derived reverse index is rebuilt and is not committed state;
- retirement leaves a permanent tombstone preventing ID reuse;
- unrelated claims have independent logical keys;
- two PRs changing the same claim intentionally conflict.

If X-06 selects the per-claim repository-file candidate, physical paths are sharded by ClaimKey and
unrelated claims update separate files; those are conditional layout consequences, not current
invariants. The per-claim physical serialization candidate in
[normative-core-spec.md](./normative-core-spec.md#91-separation-of-contracts) is the selected
Gate B test target, but it is not a released compatibility contract. Implementation remains
evidence-gated by pre-implementation Gate B and
[experiment X-06](./preimpl-experiments.md#x-06-ledger-serializer-and-physical-layout). The
directive implementation remains separately gated. For base record seal `R`, base acceptance seal
`P`, candidate record `C`, and candidate acceptance `A`, structural validity requires:

1. `C.previous_record_seal == R` from the candidate's base tree;
2. `A.predecessor_acceptance_seal == P`, absent only for the claim's first acceptance;
3. `A.claim_definition_digest` equals the complete candidate declaration;
4. every accepted subject/dependency snapshot equals a fresh evaluation of the tree receiving the
   status; a committed self-asserted event does not embed its own enclosing candidate tree;
5. every required endpoint snapshot is present atomically;
6. no engine migration, removal, scope weakening, or tombstone conflict is pending;
7. the canonical record and acceptance event digests verify.

Those conditions derive structurally current acceptance independently of trust. Protected policy
then evaluates a separately supplied provider/service receipt over the exact committed
`AcceptanceSeal` and the acceptance-candidate SHA that introduced it. The current evaluation
candidate is independently bound by the scanner status and must still re-derive structural
currency. Missing authority yields `trust-insufficient-for-blocking`; it
does not make an otherwise canonical self-asserted event structurally invalid.

After another PR merges an acceptance for the same claim, the next merge-group candidate sees a
different current seal. Its stale predecessor emits `acceptance-transition-invalid`; the author
must rebase and review the new evidence. The tool never chooses “last writer wins,” silently
rebases an acceptance, or accepts endpoints piecemeal.

The repository record is the only acceptance CAS chain. A receipt store performs idempotent
insert-if-absent on `(repository, ClaimId, AcceptanceSeal, acceptanceCandidateSHA)` and never advances
`previous_record_seal` or `predecessor_acceptance_seal`. Its signed receipt is a trust overlay, not
a second acceptance event. A committed per-claim record uses Git's ordinary merge conflict plus
the semantic checks above. Multi-claim atomic acceptance is unsupported; it cannot be simulated by
partial success.

Observations may be recomputed at any time but do not modify accepted records. There is no refresh
actor that advances acceptance, changes selectors, retires claims, or resolves CAS conflicts.

## Later service boundary

Provider-verified acceptance requires a service, but that service is not a privileged version of
the v0 parser. Its components and privileges are separated:

1. **Webhook ingress** verifies provider delivery authentication, repository identity, replay
   nonce, and event age. It stores no repository credentials in parser jobs.
2. **Read-only fetcher** obtains exact blobs/trees through a least-privilege installation token,
   pins every object ID, then destroys the token before parsing.
3. **Parser worker** receives a content-addressed bundle, runs without network/secrets in a
   resource sandbox, and emits facts only. It cannot sign, write repository state, or call provider
   APIs.
4. **Authority evaluator** queries provider review/team state for the exact acceptance candidate
   and applies the protected owner policy from that acceptance. It never evaluates repository code.
5. **Receipt signer/store** validates the committed `AcceptanceSeal`, acceptance candidate, fact, and authority
   digests; inserts one idempotent trust receipt; and signs that receipt. It cannot advance claim
   state, fetch, or parse arbitrary repository bytes.
6. **Status publisher** may write only the named check/status for the bound candidate SHA. It has no
   contents-write, PR-comment, branch, release, package, or workflow permission.

No issue-comment command, `pull_request_target` checkout, acceptance button, or workflow artifact
can cross these boundaries until a dedicated threat review and abuse tests pass. The service does
not push claim files or bypass branch protection. Human-authored repository changes still arrive
through normal pull requests.

Executable validators, if later added, run in a separate unprivileged evidence lane with no
service signing credentials. A descriptor names executable digest, complete declared inputs,
environment digest, network/secrets policy, timeout, and resource class. Unless the sandbox proves
undeclared inputs were inaccessible, the result is described as “reproducible from declared
inputs,” not complete provenance.

## Required attack and workflow test matrix

Every row is a release-blocking test for the named stage. Fixtures use disposable repositories and
must assert the JSON fact, exit code, no workspace/index/ref mutation, and deterministic rerun.

### Snapshot, CI, and policy tests

| Case | Required result |
| --- | --- |
| PR synthetic candidate matches event base/head | Complete evaluation bound to candidate SHA |
| PR candidate has base/head first but a third parent | `INVALID_EVENT`, exit `2`; no octopus candidate is accepted as the synthetic pair |
| PR checkout is head-only rather than synthetic merge | `GIT_SNAPSHOT_CHANGED`, exit `2` |
| Missing shallow base object | Exit `2`; no candidate-only success |
| Base and candidate are the same object | Invalid invocation, exit `2`; never label the tree pre-existing |
| Stale event SHA versus checked-out SHA | Exit `2` |
| Merge group contains a violation caused by an earlier queued PR | Apply ordinary base/candidate attribution; fail the candidate violation unless exact active external debt/waiver applies; `event_kind` records merge-group context |
| Merge-group base changes and provider emits new head | Old status is not reused; new candidate reruns |
| Default-branch force push | Compare exact `before` and `after`; no parent assumption |
| Default-branch creation/deletion | Unsupported/bootstrap error, never success |
| Candidate lowers a disposition | `policy-weakened`, unsuppressible exit `1` |
| Candidate adds an unknown exclude/skip/waiver/debt field to the exact policy file | `UNKNOWN_FIELD`/`CONFIGURATION_INVALID`, exit `2`; unrelated files have no control effect |
| Candidate removes protected inventory member or document | `coverage-reduced`, exit `1` |
| Candidate removes a non-protected ephemeral document | Structural findings may resolve; removal disclosed, no false coverage claim |
| Candidate changes an externally protected checker workflow/action path | `control-plane-changed`; v0 compares raw path digests and the external required workflow remains authoritative |
| Candidate breaks policy syntax while removing a finding | Configuration error, exit `2` |
| Unknown policy field or result kind | Exit `2` |
| Pre-existing finding absent from debt snapshot | Fail; attribution alone does not grandfather it |
| Matching unexpired debt | Visible `debt-tolerated`; no clean wording |
| Worsened or expired debt | Exit `1` |
| Candidate attempts to extend debt expiry | Unsuppressible failure; external snapshot unchanged |
| Valid externally protected waiver | Underlying fact remains; residual disposition and provenance shown |
| Expired selected waiver | One `waiver-invalid` expiry fact; other applicable selected-item defects also emit |
| Waiver bundle repository/ref/floor mismatch | `CONTROL_BINDING_MISMATCH`, incomplete, exit `2` |
| Wildcard, duplicate ID/target, or schema-invalid waiver bundle | Configuration/schema error, incomplete, exit `2` |
| Waiver item for a different candidate tree | Inactive inventory; no finding and no suppressive effect |
| No organization floor configured | Output records `none`; repository policy is not mislabeled trusted |
| Floor digest mismatch | Exit `2` before policy evaluation |
| Trusted-time statement has another base/candidate/ref/run/attempt, future issuance, expired validity, or TTL over 600 seconds | Replay/verification failure, exit `2`; no expiry decision |
| Trusted-time and sandbox receipts name different provider run attempts or evaluation identities, or sandbox verification names another descriptor | Control binding failure, exit `2` |
| Required workflow configured with path filters | Workflow-lint fixture fails deployment review |

### Git and filesystem tests

| Case | Required result |
| --- | --- |
| Clean commit/index | Byte-identical structural fact set across both modes |
| Staged content differs from unstaged | Index sees staged only; unstaged bytes are never opened |
| Untracked target | Cannot satisfy the index candidate reference |
| Intent-to-add index entry | Exit `2` |
| Unmerged stages | Exit `2` |
| Index bytes change in place during scan | `GIT_SNAPSHOT_CHANGED`, exit `2` |
| Git atomically renames a different index over the held initial inode | Final path reopen detects it; `GIT_SNAPSHOT_CHANGED`, exit `2` |
| Git atomically renames a byte-identical index over the held initial inode | Accepted as the same candidate bytes/projection |
| Tiny inflated object with over-limit compressed padding/interval | `git-compressed-object-bytes` resource error before watchdog |
| `objects/pack` contains 8,193 ignored/junk entries | `git-pack-directory-entries` resource error before retain/sort |
| Individually valid pack indexes cross the aggregate byte cap | `aggregate-git-pack-index-bytes` resource error in raw-basename order |
| Loose object absent and `objects/pack` absent/empty | `GIT_OBJECT_MISSING`; no ambient alternate/fetch |
| Loose object absent and `objects/pack` is present but nonordinary/unreadable | `GIT_OBJECT_UNREADABLE` |
| One subtree OID appears under two non-ancestor paths | Valid DAG; expand and charge both logical paths |
| One tree OID recurs on its current ancestor chain | `GIT_OBJECT_UNREADABLE`; no infinite traversal |
| Symlink Git object selected | Never follow; document/content use the typed unsupported boundary |
| Submodule path selected as content | `unsupported-target-kind`; no init/fetch |
| LFS pointer selected for path existence | Resolve pointer path and disclose materialization |
| LFS pointer selected for content | Path remains resolved with `lfs-pointer-only`; `unsupported-target-kind` discloses unavailable content |
| Sparse skip-worktree blob available | Read the immutable index/object blob; materialization remains `index` |
| Sparse/promised object absent | Exit `2`; no network fetch |
| Git path outside the `RepoPath` domain | `UNREPRESENTABLE_PATH`, exit `2`; an over-4,096-byte path instead uses the `raw-path-bytes` resource error |
| Newline/control/ANSI path | Correct JSON escaping and inert human output |
| Unicode NFC/NFD and case variants | Byte-distinct deterministic resolution |
| Any `--worktree` request | `INVALID_INVOCATION`, exit `2`; no filesystem traversal |
| Percent-encoded traversal, NUL, backslash, or first-level encoded slash | `invalid-reference`; one decode only |
| `%252F` in a path | Decode once to literal `%2F`; never decode again or reinterpret it as a separator |
| SHA-1 and SHA-256 Git repositories | Same selected-content SHA-256 facts; Git OIDs bind snapshot identity/provenance. SHA-1 object preimages use the pinned collision detector, but future stable authorization additionally authenticates a canonical SHA-256 digest of the complete evaluated object-preimage closure. |

### Parser and denial-of-service tests

| Case | Required result |
| --- | --- |
| MDX import writes a sentinel if executed | Sentinel absent; import remains opaque |
| MDX expression opens network/file or loops if executed | Never evaluated; bounded parse |
| Nested JSX, braces, strings, templates, and comments | Correct opaque boundaries; no false prose extraction |
| Markdown link-looking text inside code/ESM/JSX expression | Not extracted |
| Duplicate definitions and malformed front matter | Deterministic parse or explicit parser error |
| 4 MiB boundary and one byte over | Boundary parses; over-limit exits `2` before full allocation |
| Deep nesting, reference bomb, path explosion, huge tree | Named resource limit, exit `2` |
| Parser panic/segfault/abort | Worker/wrapper fails; no valid result accepted |
| Process timeout or OOM kill after partial stdout | Required check fails; partial JSON rejected |
| More than 100,000 findings | Analysis-limit error, not a truncated pass |
| Human output exceeds 200 details after complete scan | Deterministic truncation with exact total; JSON complete |
| JSON exceeds 64 MiB | Error envelope and exit `2` |
| External and same-repository URLs mixed | Same-repository checked; external counted and never fetched |
| Repository policy names a plugin/command field | `UNKNOWN_FIELD` plus `CONFIGURATION_INVALID`; command sentinel absent |
| Fuzz corpus replay across parser upgrade | No crash/pass-on-error; semantic delta requires review |

### Supply-chain and privilege tests

| Case | Required result |
| --- | --- |
| Mutable Action tag in workflow | Workflow security lint fails |
| Embedded binary checksum mismatch | Launcher fails before execution |
| Manifest body/path or artifact path is absent, symlinked, copied from another tree, or disagrees with embedded provenance | Launcher fails before execution |
| Execution constraint action tree/manifest/status source differs from current provenance or provider rule | Exit `2`; a same-name candidate status is not accepted |
| Dependency-lock digest or release manifest digest disagrees with the protected release | Exit `2` |
| Candidate local action attempts execution | Workflow policy fixture rejects it |
| Candidate attempts cache poisoning | No writable shared cache exists |
| Fork PR tries to read secret or write repository | Token/secret inventory proves unavailable; API write fails |
| `pull_request_target` or privileged `workflow_run` added | Control-plane/security lint fails |
| Evaluator attempts socket, child/Git process, shared-cache access, workspace/object-store write, or temp-cap escape | Sandbox denies it and run fails |
| Evaluator receives PATH/HOME/GIT/provider/proxy/token/loader environment or loads an undeclared sidecar | Environment/runtime-closure fixture fails before a valid report |
| Candidate text contains workflow-command injection | Escaped inert output; no annotation/environment mutation |
| Action provenance belongs to another repository/builder | Verification fails despite a valid signature |

### Governed-v1 tests before that stage can start

| Case | Required result |
| --- | --- |
| Two PRs accept different claims | Independent per-claim records; no shared-state conflict |
| Two PRs accept the same record and acceptance predecessors of one claim | First may merge; second fails `previous_record_seal`/`predecessor_acceptance_seal` CAS after rebase or merge-group rebuild |
| Claim declaration edited without new acceptance | Review required; no implicit co-change attestation |
| Claim removed with its record | `governed-claim-removed`; cannot disappear cleanly |
| Claim ID reused after retirement | `claim-id-reused` |
| Scope changed to historical to avoid invalidation | `scope-weakened` |
| Validator descriptor retargeted while preserving output | `validator-changed`; prior acceptance not reused |
| Parser/selector engine version changes | Dual evaluation or `engine-migration-required`; never automatic green |
| Acceptance includes only one of several required endpoints | `acceptance-transition-invalid` |
| Acceptance records spoofed reviewer text | Acceptance may be structurally current, trust remains self-asserted, and the protected claim fails blocking trust |
| Receipt names a candidate other than the one that introduced its exact acceptance seal | Invalid acceptance authority |
| Unrelated later candidate leaves seal/definition/endpoints/projections/reviewer rule unchanged | Receipt remains valid; current candidate status and structural currency are rechecked separately |
| Service receives the same receipt request twice | Idempotent same receipt; no duplicate trust overlay or claim transition |
| Multi-claim atomic acceptance requested | Permanently unsupported by `accept`; one command accepts exactly one claim, while split/merge use distinct closed lifecycle transactions and inherit no acceptance |

## Deployment gates

Scanner v0 may enter report-only shadow mode after snapshot consistency, parser isolation, resource
limits, and no-write tests pass. That authorization is point-in-time reporting only. No scanner
result may be installed as a merge-authorizing required check until all of the following are true:

- the separate provider request-wire RFC has root schemas, framing goldens, and hostile-input tests;
- exact PR and merge-group candidate acquisition is verified;
- a control-epoch/provider-freshness RFC and X-07 prove authenticated merge-time equality plus
  invalidation/rerun after base movement, expiry, revocation, control rotation, ruleset mutation,
  or workflow-source rotation;
- a required SHA-1 lane authenticates and binds a canonical independent SHA-256 digest over every
  loose-equivalent commit, traversed tree, and selected blob used by evaluation into that epoch, or
  required enforcement accepts only SHA-256 object format;
- the required lane is an active organization/enterprise ruleset workflow that pins the exact
  externally owned source repository ID, workflow path/ref, non-null full commit SHA, resolved
  workflow blob OID/raw digest, and immutable dependency closure, or a provider mechanism with
  equivalent content-addressed source identity and protection; merely requiring a status name,
  mutable branch/path, or expected GitHub App is insufficient;
- the required workflow runs without candidate-selectable filters and includes every required PR
  and `merge_group` event path; its protected job is unconditional, cannot continue on error or be
  skipped through dependencies/matrices, and maps only an accepted passing envelope to provider
  success—never skipped or neutral;
- all Action and binary inputs are immutable and checksum-verified;
- base absence and every abnormal termination fail closed;
- false-positive calibration is acceptable for that exact structural rule;
- existing blockers are fixed or represented in an externally reviewed, expiring debt snapshot;
- owner and emergency provider-bypass procedures are documented;
- push monitoring and merge-queue behavior are exercised.

Only after every prerequisite passes may the deterministic structural classes that individually
passed calibration use the `enforce` profile as merge authorization. Until then, `enforce` remains
a local/report result and cannot be described or registered as a stable required check. An active
ruleset workflow with an immutable source commit establishes exact workflow content better than a
generic status check, but it does not replace the separate control-epoch freshness proof.

Governed v1 remains blocked until stable claim identity, directive conformance, per-claim canonical
encoding, provider authority, tombstones, CAS, service isolation, and the entire governed test
matrix pass. A repository with no accountable owners or no protected provider integration cannot
run a blocking narrative-attestation lane.

The immediate implementation consequence is simple: build the CLI/schema/Git-acquisition scaffold,
complete parser corpus, hostile fixtures, and conformance harness. Only after the corpus passes may
the evidence-producing scanner parser/evaluator be built, and it remains a comparison rather than
a state machine. If it proves valuable, the governed layer can be added without having to
reinterpret an automatic lock update as human review or unwind a privileged refresh bot.
