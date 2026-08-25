# Invocation

Install from [crates.io](https://crates.io), or build from source:

```sh
cargo install amiss
```

Every release also carries the engine and [the external prober](external-assessment.md)
prebuilt for Linux on x86_64 and arm64, both macOS architectures, and Windows x86_64, with a
`SHA256SUMS` file and the sigstore bundle that attests it.
`gh attestation verify <binary> --repo HardMax71/amiss` matches a downloaded binary against the
build that produced it.

The public command line is closed: the grammar below is everything, and anything else
exits 2 as an invalid invocation. The verb comes first; after it the options come in any
order, each at most once. There is no `--help`. A refused invocation prints the violated
contracts and then this whole grammar on stderr, so the binary teaches its own command
line. The one exception is a malformed `--format` selection, which prints a single
`amiss: invalid invocation` line, since the output channel itself was never agreed. The
copy below is checked against the binary's in CI.

<!-- amiss-doc-contract:invocation-grammar:start -->
```text
amiss check --repo <path> --object-format <sha1|sha256>
            --base <full-oid> (--candidate <full-oid> | --index)
            [--repository <host>/<owner>/<name>
             --ref refs/heads/<name>
             --default-branch-ref refs/heads/<name>
             [--forge <github|gitlab|gitea>]]
            --profile <observe|enforce-introduced|enforce>
            [--explain-scope] [--format <human|json|sarif|codequality>]
amiss fix   --repo <path> --object-format <sha1|sha256>
            --base <full-oid> --index
            [--repository <host>/<owner>/<name>
             --ref refs/heads/<name>
             --default-branch-ref refs/heads/<name>
             [--forge <github|gitlab|gitea>]]
            --profile <observe|enforce-introduced|enforce>
amiss claim --repo <path> --path <repo-path> --line <n> --name <name>
amiss adopt --repo <path> --object-format <sha1|sha256>
            --base <full-oid> --candidate <full-oid>
            --repository <host>/<owner>/<name>
            --ref refs/heads/<name>
            --default-branch-ref refs/heads/<name>
            [--forge <github|gitlab|gitea>]
            --floor-digest sha256:<64-hex> --debt-owner <name>
            --debt-reason <text> --created-at <utc-instant>
            --expires-at <utc-instant> --debt-output <path>
amiss external-plan --report <path> [--format <human|json>]
amiss external-assess --plan <path> --evidence <path> [--format <human|json>]
amiss render --report <path> --format <human|sarif|codequality>
amiss refs --report <path>
           (--target <repo-path> | --target-bytes-hex <lower-hex>)
           [--format <human|json>]
amiss --version
```
<!-- amiss-doc-contract:invocation-grammar:end -->

The table gives each flag in one line. The paragraphs after it carry the exact semantics;
trust them when the short form reads ambiguous.

| Flag | Value | Role |
| --- | --- | --- |
| `--repo` | path | the repository checkout to read |
| `--object-format` | `sha1` or `sha256` | the repository's object format |
| `--base` | full commit ID | the state the comparison starts from |
| `--candidate` | full commit ID | the state under review; exclusive with `--index` |
| `--index` | none | checks the staged state against the base instead |
| `--repository` | `<host>/<owner>/<name>`; owner and name lowercase | unverified identity claim for same-repository URLs |
| `--ref` | `refs/heads/<name>` | the candidate branch this tree belongs to; in the adopt form, also the ref the minted debt binds to |
| `--default-branch-ref` | `refs/heads/<name>` | which branch counts as default when resolving URLs |
| `--forge` | `github`, `gitlab`, or `gitea` | URL dialect; an explicit flag beats the host table |
| `--profile` | `observe`, `enforce-introduced`, or `enforce` | report only, block introduced findings while carrying the backlog, or let every blocking finding gate; see [Profiles and findings](profiles.md) |
| `--explain-scope` | none | adds deterministic scope lines to human output |
| `--format` | `human`, `json`, `sarif`, or `codequality` | ten grouped items, the exact report in [The report](report.md), or its SARIF or GitLab Code Quality projection |
| `--path` | repo-relative path | the file the authored claim pins |
| `--line` | positive line number | the line the claim expects, one-based |
| `--name` | ASCII claim name, 1 to 120 bytes | the `amiss:` label; starts with a letter or digit, then letters, digits, `.`, `_`, `-` |
| `--floor-digest` | `sha256:` and 64 hex | the organization floor the minted debt snapshot binds to |
| `--debt-owner` | text | the item owner the floor must authorize |
| `--debt-reason` | text | why the debt is being recorded |
| `--created-at` | UTC instant | the snapshot's and items' creation instant |
| `--expires-at` | UTC instant | when the items expire; must be after `--created-at` |
| `--debt-output` | path | where the minted snapshot is written; must not exist |
| `--report` | path | the report file the plan, render, or refs form reads; foreign to every other form |
| `--plan` | path | the plan file the assessment form judges; foreign to every other form |
| `--evidence` | path | the producer observations the assessment form judges; foreign to every other form |
| `--target` | repo-relative path | the text path whose candidate references `refs` returns |
| `--target-bytes-hex` | lowercase even-length hex | the raw-byte path whose candidate references `refs` returns; exclusive with `--target` |
| `--version` | none | prints this binary's version and engine digest; stands alone, with no `check` and no other flag |

`--base` and `--candidate` take full commit IDs: lowercase hex, forty characters for
sha1, sixty-four for sha256. Branch names, short forms, and two equal IDs are refused. Amiss
evaluates exactly the trees you name and resolves nothing for you. `--index` swaps the
candidate for the staged state, including entries marked
[skip-worktree](https://git-scm.com/docs/git-update-index).

The identity group is a claim, not a login. `--repository github.com/acme/widgets` tells
the resolver which same-repository URLs to read as this repository; nothing verifies you
own it, so the spelling is strict. The host matches your documents' URLs byte for byte and
is never case-folded. Owner and name must be lowercase ASCII, so a workflow passing
`github.repository` lowercases it first. Owner segments may nest, the GitLab group form;
an effective github or gitea dialect refuses a nested owner it could never match. A wrong
spelling is refused, never rewritten. Without the identity group every absolute forge URL
stays external, and a human run with a nonzero external count says so beside its totals
rather than degrading silently.

`--ref` names the candidate branch for URL resolution only: no protected target branch,
no `--target-ref`, and the report's target stays null. No spelling of these flags turns a
CLI run into a provider-authenticated one. A URL naming the declared default branch while
another candidate is under test is recognized and reported as `unsupported-version-scope`,
not resolved. Full lowercase commit IDs must match `--object-format` and resolve only through that
exact commit's locally available objects. A fully walked tree may prove absence; an unavailable
commit, tree, or target retains its exact ID and contained path as unsupported version evidence. A
branch whose spelling is also a full ID is refused as ambiguous. Without the identity group, forge
links stay external URLs and the report says so.

`--forge` names the URL dialect the resolver applies and accepts exactly three values.
`github` covers GitHub and GitHub Enterprise, `gitlab` the `/-/blob/` separator form,
`gitea` the form Gitea, Forgejo, and Codeberg share. Without the flag, github.com,
gitlab.com, and codeberg.org select their own dialects; an identity on any other host is
refused as `INVALID_EVENT` until the flag names its dialect, since accepting it would
silently leave every same-repository link external. An explicit flag beats the table;
that's how a self-hosted instance gets its grammar. Recognizing a dialect authenticates
nothing about how the run was invoked.

`--format json` prints the exact report in [The report](report.md), one line plus a
trailing newline. `sarif` and `codequality` project the same report for code-scanning
uploads and GitLab merge-request widgets. A refused invocation still emits a refusal
envelope under json and sarif, and an empty array under codequality, so a consumer never
parses half a document.

`human` is the default. It prints a status header, one `error` row per retained analysis
error, at most ten grouped Fix and Check items naming only a target and an affected-place
count with an overflow line when more exist, then at most ten Existing items with their
own overflow line, one fixed `note` sentence per error code using the wording from
[Limits and refusals](limits.md), and three totals lines. Existing items are the
pre-existing backlog at warn or fail, and the backlog keeps its own window, so introduced
volume cannot push it off the terminal. The full findings stay in JSON. `--explain-scope` adds six scope lines to that human output, five
fixed and one naming this run's counts, and changes nothing in JSON, behavior pinned by the
[CLI tests](https://github.com/HardMax71/amiss/tree/main/crates/amiss/tests/cli).

`amiss render --report <path> --format <human|sarif|codequality>` reopens one JSON report
and emits an alternate projection without reading the repository or evaluating it again. It
accepts only the active report envelope, supported wire compatibility, matching payload digest,
and a consistent recorded result. A successful projection exits with that recorded 0, 1, or 2;
a closed stdout does not change it. Invalid, unreadable, or oversized report input exits 2 without
output. JSON is not an admitted projection because the report file is already canonical JSON;
requesting it is a grammar refusal and may emit the standard incomplete JSON refusal envelope.

Exit codes are three classes, not detail. 0 means the run completed and nothing blocks. 1
means a finding blocks. 2 means nothing trustworthy could be produced. A consumer that
closes the pipe early, `head` among them, ends the printing and not the verdict.

`amiss fix` repairs what the check proves, over the staged state only. It runs the same
evaluation as `check --index`, takes every finding whose `fix` is not null, and rewrites
exactly those byte spans in the working tree. Nothing is applied on faith. The staged
index is pinned before the evaluation and verified unchanged before any write. A document
is repaired only while its working-tree bytes still equal the staged bytes the fixes were
computed against, and one already holding the repaired bytes counts as already fixed. A
document is refused whole when it is missing from the index, a symlink, unreadable,
escaping the worktree, differing from its staged bytes, carrying overlapping or
out-of-range spans, or failing the write; each refusal row names its reason. Output is one
row per document and a summary line, so `--format`, `--explain-scope`, and `--candidate`
are refused rather than ignored. Exit 0 means every carried fix was applied or already
present. Exit 1 means a document refused, or the staged index moved mid-run with nothing
applied. Exit 2 means the evaluation could not be trusted or the staged index could not
be read; either way nothing was touched. Restage and rerun `amiss check` to see the
repaired state judged.

`amiss claim` authors a [value claim](claims.md) and reads no git at all. Give it a
repo-relative path and a one-based line; it reads that line from the working tree and
prints one ready definition to stdout, nothing else, so the output pastes or pipes
straight into a document. It proves before printing: the candidate definition is run back
through the markdown extractor and the claim grammar, double-quoted first and
single-quoted when that round trip fails. A line neither spelling can carry, an HTML
entity among the causes, is refused rather than printed broken. Exit 0 prints the
definition. Exit 1 refuses the file or the line: unreadable, past the end, not UTF-8, or
numbered beyond the platform. Exit 2 is an invalid invocation, which is also where a
`--name` outside its grammar or a `--path` carrying reserved bytes lands.

`amiss adopt` onboards a repository that already has drift. It runs the evaluation under
enforce, accepting no `--profile`, and mints a [debt snapshot](controls.md) from every
blocking finding of the two debt-eligible kinds, `explicit-target-missing` and
`explicit-target-type-mismatch`. Gate new drift today; work the recorded backlog off on
its own clock. The engine supplies each item's key and accepted fact from the evaluation.
The flags supply what it cannot know: the floor digest the snapshot binds to, the owner
that floor must authorize, the reason, both instants in the wire's own clock grammar with
creation strictly before expiry, and the output path, which must not exist. Adoption
records a committed tree, so `--index` is refused and the identity triple is required.
`--ref` does double duty here: beyond URL resolution, its exact string becomes the
snapshot's ref binding. Spell it as the branch the consuming lanes enforce, since a
snapshot bound to another ref stays out of scope there; the check form is untouched by
that reuse. Consumption is narrower than minting: only the sealed bootstrap that the
[provider lanes](provider-controls.md) operate feeds a snapshot back to the engine, while
the public `check` form and the convenience Action supply every control as absent and
never read one. On a repository without a lane the minted file waits, and the working
ramp for a standing backlog is [`enforce-introduced`](profiles.md). The minted file is
written only after the engine's own reader accepts its
bytes, by exclusive creation. The summary line counts what was recorded, what blocked but
is not debt-eligible, and what was eligible but missing facts. Exit 0 recorded the
snapshot. Exit 1 means the output path already exists or the write failed, any partial
file removed. Exit 2 means nothing trustworthy could be recorded: the evaluation failed,
the report carried no candidate tree, or the minted bytes failed the engine's own reader.

`amiss external-plan` derives [the external plan](external-plan.md) from a report file a
`check --format json` run already wrote. It opens no repository and touches no network:
it verifies the report's own payload digest, refuses an incomplete report, and projects
the delegated-evidence delta the report already carries. `--format` takes `human` or `json` here;
SARIF and Code Quality remain report projections and are refused here. Exit 0 wrote the plan. Exit 2 means the input
could not be trusted: unreadable, larger than a scanner report can be, not the scanner's
strict JSON, not a report envelope, digest mismatch, incomplete, or carrying a malformed
eligible occurrence.

`amiss external-assess` judges [an external plan](external-plan.md) against one
producer's observations, writing [the external assessment](external-assessment.md)
offline under the engine's fixed policy. It verifies the plan's
digest and the evidence's binding to that exact plan; evidence naming a destination the
plan did not introduce, repeating one, or binding another plan refuses the whole run.
`--format` takes `human` or `json`. Exit 0 wrote the assessment, refuted rows included,
since the artifact is advisory data. Exit 2 means an input could not be trusted.

`amiss refs` asks a complete validated report which candidate occurrences refer to one
repository path. It opens no repository and does not reinterpret prose: an occurrence matches
when its normalized repository intent, resolved or unresolved path, resolved target, or known
version-scoped path is the exact target. Correlation alternatives are included, so ambiguous
pairing cannot hide a candidate occurrence. Human output names every document, source position,
construct, resolution, and observation ID. JSON output is an array of the unchanged candidate
`Occurrence` objects already defined by the report schema, not a new report or wire envelope.
`--target-bytes-hex` makes raw non-UTF-8 Git paths queryable on every platform. A valid empty
answer exits 0 even when the source report recorded blocking findings; incomplete, malformed,
unreadable, oversized, or digest-mismatched reports exit 2. Querying writes no state and changes
no recorded verdict.

`amiss --version` is the grammar's ninth form and stands alone: any second token makes it
an ordinary, refused invocation. It opens no repository. It prints two lines and exits 0:

```text
amiss <version>
engine sha256:<64 hex digits>
```

The first line is the binary's version. The second is the `engine_digest`, computed by
hashing the executable's own bytes: the same digest every report stamps and the
[release manifest](https://github.com/HardMax71/amiss/blob/main/spec/scanner-release-manifest.schema.json)
pins per platform. So an installed binary matches a release row, and a report matches
back to the binary that produced it, without running a scan. A binary that cannot read
its own file prints `engine unavailable` and still reports its version. The other shipped
binaries answer `--version` with one line each: `amiss-bootstrap`, `amiss-manifest`,
`amiss-constraint`, `amiss-probe`, and the three provider services.
