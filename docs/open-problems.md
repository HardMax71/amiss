# Problems register: adversarial pass and resolutions

Date: 2026-07-10. The adversarial pass found eighteen problems; this file records the resolutions
proposed that day. A later pre-implementation review re-opened the central identity, attestation,
refresh, lifecycle, policy, and merge-candidate questions; see
[pre-implementation-review.md](./pre-implementation-review.md). The final OP-01 through OP-30
disposition is in [issue-closure-matrix.md](./issue-closure-matrix.md); many “resolved” mechanisms
below are explicitly rejected historical alternatives. The three unifying moves proposed here
were:

1. The functional check with a refresh lane. The check is a pure function of tree and ledger and
   never writes; only `assure ok` and a post-merge refresh job write the ledger. This dissolves
   the lock-write trilemma and most concurrency conflicts.
2. Content-hash unit identity. A unit's identity is its normalized token content plus document
   path, not headings or ordinals. Trust-on-creation and trust-on-edit stop being special events
   (an edited unit is a new identity with no baseline), position churn cannot lose baselines, and
   orphan collection becomes the audit trail for absolution.
3. Attributed failure. A finding fails only when the pull request's own diff caused it, computed
   by evaluating base and candidate trees against the same ledger and diffing the finding sets;
   environmental staleness reports with a cleanup queue. This makes strict mode livable and fixes
   blame UX.

## Core soundness

OP-01 (was blocker). Lock-write trilemma: shallow clone suffices, edits re-baseline with no
ceremony, and the committed lock is the only state could not all hold, because an edit does not
write the lock.
Resolution (in design.md): the check never writes and evaluates tree against the ledger as of
main; a unit identity absent from the ledger is fresh by construction, so no baseline needs
capturing at edit time at all. The refresh lane (post-merge job on the default branch, or a
schedule) records new identities, retires orphans, and commits when the ledger changed. Shallow
clones are fully sufficient because staleness never consults history. Accepted residue: a
minutes-long window between merge and refresh where a just-edited unit's staleness against very
new target changes goes unobserved.

OP-02 (was blocker). Trust-on-edit absolved silently; a formatting sweep was an implicit
`doorstop clear all` with no audit trail.
Resolution (in design.md): unit hashing is token-based (word tokens plus code spans and link
targets), so reflow, wrapping, and list-marker sweeps create no new identities and cannot absolve
anything. Real edits retire the old identity, and the refresh commit enumerates exactly which
stale baselines were retired by edits, while the editing PR gets an informational finding ("this
edit clears staleness on N targets", diffs attached) so review sees the absolution before it
lands. Typo-level edits still absolve their unit; that is trust-on-edit by intent, now visible.

OP-03 (was blocker; dossier corrected). `docs.yml` never existed, so the broken-versus-noise rule
classified it as noise and "three dead references" was an overclaim.
Resolution (in design.md): a probable-broken tier. A token that parses as a path into an existing
directory whose siblings share its extension reports and never hard-fails; policy may promote it
per path. Day-zero on user zero: two hard failures, one probable-broken report.

OP-04 (was sharp; dossier corrected). Formatter immunity was a v1 property sold as day-zero.
Resolution: a declared projection ladder, printable via `assure projections`. Filetypes with a
tree-sitter grammar get token-stream fingerprints (formatter-immune up to token changes; comments
included, since comments can matter to docs); everything else gets line-normalized raw content and
keeps formatter noise, stated plainly. Launch with the mainstream grammar set (TypeScript,
JavaScript, Python, Go, Rust, Java, C, C++, Ruby, Scala, YAML, JSON, TOML, Bash).

OP-05 (was sharp). Anchor-plus-ordinal identity was the tool's own EC-A4: heading renames and
insertions lost baselines, which read as fresh exactly when pages get reorganized.
Resolution (in design.md): identity is the unit's normalized content plus document path, with an
ordinal only for exact duplicates. Headings and positions are display metadata. Moves within a
file preserve identity for free; file renames migrate by exact hash match across paths; nothing is
ever re-keyed silently. CLI addressing by anchor resolves to current units at command time and is
a view, not identity.

## Extraction

OP-06 (was sharp). Unmodeled reference classes: negative mentions, `path:12-25` citations,
relative-path ambiguity, placeholder paths, and gitignored build artifacts (user zero's
`playground-examples.generated.ts`) misclassified as broken.
Resolution: line and range suffixes are stripped before resolution and kept for display. Relative
tokens resolve doc-relative first, then repo-relative; exactly-one hit binds, two hits prefer the
format's own convention (links are doc-relative), zero falls through to history and the
probable-broken heuristic. Tokens containing `<`, `{`, `*`, or `$` are noise unless they are
explicit globs in an assure line. A non-existing path that matches a gitignore pattern classifies
as generated-reference (informational, never broken), with an optional one-line `[outputs]` glob
list in the TOML for precision. Negative mentions bind on purpose: if the file you are told never
to touch disappears, the advice is stale, and the rare loud case has the skip line. Backslashes
normalize to slashes; bare filenames bind only when unique in the tree.

OP-07 (was debt). `file=` fences mean transclusion in some sites and hand-copies in others.
Resolution: an empty fence body implies transclusion (only the path is checked); a non-empty body
is a snippet claim. Sites with other attribute names map them once in the TOML
(`[fences] include = [...]`).

OP-08 (was debt; refined by OP-20). Section scope off the happy path.
Resolution, specified: the unit is the block (paragraph, list, table, or fence; a paragraph run in
plain text), and sections (heading to next same-or-higher heading) group blocks for reporting and
`assure ok` addressing. Text before the first heading belongs to the document-root scope, which
also owns frontmatter; footnote definitions attribute to the block containing the footnote
reference; HTML blocks are scanned for `href` and `src`. Reference positions inside a unit are
display metadata.

## Policy and governance

OP-09 (was sharp). One invisible skip line defeated org strict mode.
Resolution (in design.md): skip downgrades enforcement by exactly one level, is inert on paths a
rule marks `local_override = false`, and every run prints the skip inventory with locations.

OP-10 (was sharp). Org floor versus repo TOML was unspecified.
Resolution (in design.md): the action and reusable workflow accept a minimum-enforcement input
that clamps repo policy upward and prints the clamp when it engages; rulesets pin it fleet-wide.

OP-11 (was debt). Rule law unspecified.
Resolution (in design.md): gitignore glob semantics; per-key last-match-wins, so a rule setting
only `stale` inherits `broken` from earlier matches or defaults; `exclude = true` admits no other
keys and validation rejects the combination; `[check]` tables and `[[rule]]` entries live in the
one root TOML.

## CI reality

OP-12 (was sharp). Strict mode fought merge queues; concurrent lock updates conflicted.
Resolution (in design.md): attributed failure (unifying move 3) means queued PRs stop bouncing on
staleness other merges introduced, and the refresh-lane model (move 1) means PRs rarely write the
ledger at all, shrinking conflicts to two PRs accepting the same unit, resolved by rerunning
acceptance on the merge result.

OP-13 (was sharp). The key finding lands on files outside the PR diff, where GitHub renders no
inline annotations.
Resolution: the sticky PR comment grouped by root cause is the primary surface ("your change to
`Config.scala` stales docs A and B"), the job summary is the full report, annotations are a bonus
for in-diff files, and attribution (move 3) guarantees the comment talks about this PR's causes
first, with environmental staleness in a separate cleanup section.

OP-14 (was debt). Comment-command lane breaks on forks that disallow maintainer edits; a parser
runs over attacker-controlled input in a privileged workflow; the ledger is hand-editable.
Resolution, documented posture: the comment lane is same-repo branches only and the docs say so;
the parser is fuzzed from day one and the privileged lane runs it with resource limits;
`assure verify-lock` validates internal consistency; authenticity of baselines is git history plus
review culture, stated plainly wherever the audit trail is sold, with signed refresh commits as a
later hardening step. No cryptographic claim is made that the design cannot honor.

## Product honesty

OP-15 (was debt). `assure ok <page>` was bulk absolution in miniature and per-unit addressing
reimported the identity problem.
Resolution: four scopes, all resolved to current identities at command time: `assure ok <path>`
(whole file, logged as file-scope), `assure ok <path>#<anchor>` (one section),
`assure ok --target <code-path>` (root-cause scope: accept everything a reviewed code change
staled, which matches how fan-out is actually reviewed), and `assure ok -i` (interactive
walk-through). File-scope acceptance is distinctly logged, like every bulk gesture.

OP-16 (was debt). Stated-value checks were fragile beyond exact literals.
Resolution, scoped: they bind an exact literal span at acceptance time, record the span, and point
at it when the value moves; number words, ranges, approximations, and locale formats are out of
scope and route to managed regions. Multiple candidate occurrences are disambiguated once, in the
accept flow.

OP-17 (was debt). Real-repo boundaries unlisted.
Resolution, defaults declared: built-in exclusions (`node_modules`, `vendor`, `third_party`,
`dist`, `build`, minified assets, lockfiles including `assure.lock`); the default document set is
the prose extensions (`md`, `mdx`, `markdown`, `rst`, `adoc`, `txt`, `org`), doc-named
extensionless files (README, CONTRIBUTING, CHANGELOG), and agent files (`CLAUDE.md`, `AGENTS.md`,
`.cursorrules`, `llms.txt`); config formats are targets, not documents, unless a rule opts one in.
Translated doc trees are detected at init (locale-coded directories) and suggested as
report-level rules with per-locale grouping; cross-locale lag tracking is a v2 view. References
into submodules resolve only when the submodule is checked out and otherwise report an
informational unresolvable-submodule state, never broken.

OP-18 (was debt; dossier corrected). Internal consistency and the thin day-zero moat.
Resolution: EC-E3 now marks reason-required acceptance as growth-tier; the fast-blocking-job
section now labels its declaration-dependent bullets as growth model; and the moat statement is
owned rather than dodged: the defensible assets are the growth model, the calibration corpus
(user zero's drifts plus CoDocBench), and the quality of extraction defaults, because the day-zero
loop itself is imitable in weeks. Speed of iteration on the defaults is the plan, not a patent.

## Second pass, same day: attacking the resolutions

The fixes above were then given the same adversarial treatment. Twelve findings, open, each with a
direction.

OP-19 (was blocker; resolved). The refresh lane's soundness rests on an unstated invariant: it must never
update an existing ledger entry, only add baselines for new identities and retire orphans. As
written ("records baselines for new unit identities, retires orphaned ones") an implementer could
legally refresh existing fingerprints, which would auto-absolve every stale unit on every merge
and turn the lane into a rubber-stamp machine. Resolution: state the never-update invariant
explicitly in design.md and test it first in the tool.

OP-20 (was blocker; resolved). Section-hash identity destroys signal too broadly and contradicts OP-02's
own mitigation math. With the unit defined as a section, one typo anywhere in a forty-line section
retires every baseline in it, so staleness across all its references is absolved by any edit;
"typo edits are rare per unit" was argued at paragraph scale and adopted at section scale.
Resolution: identity moves to the block (paragraph, list item run, table, fence), sections become
reporting and `ok`-addressing scope only; a retargeted link changes its block's identity, which is
correct (changing a reference is an edit); cross-file migration happens only on unique hash
matches, otherwise fresh with a note; exact-duplicate ordinals are harmless because identical
content has identical reference sets, and that deserves one sentence so nobody "fixes" it.

OP-21 (was blocker; resolved). Broken is not attributed, and that both blocks adoption and contradicts the
cleanup-queue philosophy. The wiring example still says "broken references always fail", so two
pre-existing dead links fail every unrelated PR from the moment of adoption, which is the classic
turn-it-off generator. Resolution: attribute broken exactly like staleness: PR-introduced broken
fails the PR, pre-existing broken fails the default-branch job and sits in the cleanup queue.
Also, attribution needs the base tree, which a depth-1 checkout of the merge commit does not
contain; "shallow clone suffices" needs the depth-2 (or explicit base fetch) asterisk.

OP-22 (was sharp; resolved). The refresh lane assumes it can push to main; protected branches often forbid
that. Resolution: three deployment variants (GitHub App with bypass permission, auto-merged bot PR,
scheduled batch PR), a loop guard (lock-only commits skip CI and do not retrigger refresh), a
concurrency group serializing refresh runs with rebase-retry, and an ordering rule: on the
default-branch job, refresh runs before reporting so a red repository still accumulates baselines.

OP-23 (was sharp; resolved). Probable-broken has a tutorial-shaped noise class: prose that tells the
reader to create a file ("create `app/models.py`") matches the existing-directory heuristic and
gets reported forever. Resolution: position-sensitive confidence. Link-position tokens (markdown
link targets, citation-style references) report; bare prose mentions are counted but hidden
behind `--all`; fenced content never yields probable-broken at all, though path tokens in fences
that do resolve still bind for staleness, because shell examples rot too.

OP-24 (was sharp; resolved). Bare-filename binding is impermanent: `Config.scala` binds while unique, and
an unrelated same-named file appearing later flips the resolution ambiguous and the claim loud.
Resolution: record the resolved full path in the ledger at baseline time; later ambiguity affects
only new, unbaselined mentions.

OP-25 (was sharp; resolved). `exclude = true` bypasses the org floor: a repo can opt whole trees out of
scanning and the minimum-enforcement clamp never sees them. Resolution: the floor input carries a
protected path set on which `exclude` and `ignore` are inert, so everything there at least
reports.

OP-26 (was sharp; resolved). Stated-value bindings keyed to block hashes die on every edit of the
containing block, forcing re-acceptance or risking silent re-binding. Resolution: bind at
(document, check) scope; verification asserts the current literal appears within the claim's
section; the recorded span is display metadata only.

OP-27 (was debt; resolved). The policy defaults never mention the new kinds: `probable_broken` and
`generated_reference` need default verbs (report and ignore-with-tally respectively) and
addressability in rules like every other kind.

OP-28 (was debt; resolved). Heuristic precedence: a tracked path that also matches a gitignore pattern
(force-added files) must classify as tracked; the generated-reference heuristic applies only to
paths absent from the tree.

OP-29 (was debt; resolved). Ledger scale is unquantified: one entry per referenced block on a large
monorepo, churned by the refresh lane; needs a size estimate on user zero and one large OSS repo
before v0, plus the already-specified per-subtree sharding as the escape hatch.

OP-30 (was debt; resolved). Consistency sweep induced by the above: the wiring example's
"broken references always fail" comment, the day-zero section's "broken references block by
default", the shallow-clone phrasing in two places, OP-08's "the unit is the section" wording, and
README's "editing the section clears the flag" all received the
block-identity and attribution updates in this pass, so the dossier agrees with itself again.
