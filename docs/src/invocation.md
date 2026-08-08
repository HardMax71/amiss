# Invocation

Install from [crates.io](https://crates.io), or build from source:

```sh
cargo install amiss
```

Every release also carries the engine prebuilt for Linux on x86_64 and arm64, both macOS
architectures, and Windows x86_64, with a `SHA256SUMS` file and the sigstore bundle that attests it.
`gh attestation verify <binary> --repo HardMax71/amiss` matches a downloaded binary against the
build that produced it.

The command line is closed: the grammar below is everything, each option appears at most
once, order does not matter, and anything else is rejected as an invalid invocation. There
is no `--help`; a rejected human invocation prints this same grammar under its refusal, so
the binary teaches the command even where this book is not installed, and the copy below
is checked against the binary's in CI.

<!-- amiss-doc-contract:invocation-grammar:start -->
```text
amiss check --repo <path> --object-format <sha1|sha256>
            --base <full-oid> (--candidate <full-oid> | --index)
            [--repository <host>/<owner>/<name>
             --ref refs/heads/<name>
             --default-branch-ref refs/heads/<name>
             [--forge <github|gitlab|gitea>]]
            --profile <observe|enforce-introduced|enforce>
            [--explain-scope] [--format <human|json|sarif>]
amiss fix   --repo <path> --object-format <sha1|sha256>
            --base <full-oid> --index
            [--repository <host>/<owner>/<name>
             --ref refs/heads/<name>
             --default-branch-ref refs/heads/<name>
             [--forge <github|gitlab|gitea>]]
            --profile <observe|enforce-introduced|enforce>
amiss adopt --repo <path> --object-format <sha1|sha256>
            --base <full-oid> --candidate <full-oid>
            --repository <host>/<owner>/<name>
            --ref refs/heads/<name>
            --default-branch-ref refs/heads/<name>
            [--forge <github|gitlab|gitea>]
            --floor-digest sha256:<64-hex> --debt-owner <name>
            --debt-reason <text> --created-at <utc-instant>
            --expires-at <utc-instant> --debt-output <path>
amiss --version
```
<!-- amiss-doc-contract:invocation-grammar:end -->

The table gives each flag in one line; the paragraphs after it carry the exact semantics
and are the ones to trust when the short form reads ambiguous.

| Flag | Value | Role |
| --- | --- | --- |
| `--repo` | path | the repository checkout to read |
| `--object-format` | `sha1` or `sha256` | the repository's object format |
| `--base` | full commit ID | the state the comparison starts from |
| `--candidate` | full commit ID | the state under review; exclusive with `--index` |
| `--index` | none | checks the staged state against the base instead |
| `--repository` | `<host>/<owner>/<name>`, lowercase | unverified identity claim for same-repository URLs |
| `--ref` | `refs/heads/<name>` | the candidate branch this tree belongs to |
| `--default-branch-ref` | `refs/heads/<name>` | which branch counts as default when resolving URLs |
| `--forge` | `github`, `gitlab`, or `gitea` | URL dialect; an explicit flag beats the host table |
| `--profile` | `observe`, `enforce-introduced`, or `enforce` | report only, block introduced findings while carrying the backlog, or let every blocking finding gate; see [Profiles and findings](profiles.md) |
| `--explain-scope` | none | adds deterministic scope lines to human output |
| `--format` | `human`, `json`, or `sarif` | ten grouped items, the exact report in [The report](report.md), or its SARIF projection |
| `--version` | none | prints this binary's version and engine digest; stands alone, with no `check` and no other flag |

`--base` and `--candidate` take full commit IDs, never branch names or short forms: Amiss
evaluates exactly the trees you name and resolves nothing for you, and `--index` checks the
staged state instead, including entries marked
[skip-worktree](https://git-scm.com/docs/git-update-index). The identity group is a claim
Amiss cannot verify, so its spelling is strict: the host is matched byte for byte against
the URLs in your documents, owner and name must be lowercase with segments nested only for
GitLab group paths, a workflow passing `github.repository` has to lowercase it first, and a
wrong spelling is refused rather than rewritten. `--ref` names the candidate branch for URL
resolution only; it is not a protected target branch, there is no `--target-ref`, the
report's target stays null, and no spelling of these fields turns a CLI run into a
provider-authenticated one. A URL for the declared default branch while another candidate
is under test is recognized but reported as `unsupported-version-scope`, and without the
identity group forge links stay external URLs and the report says so.

`--forge` names the dialect the resolver applies: `github` covers GitHub and GitHub
Enterprise, `gitlab` the separator form, `gitea` also Forgejo and Codeberg. Without the
flag, github.com, gitlab.com, and codeberg.org select their own dialects and every other
host selects none, leaving that host's links foreign and `evaluation.forge` null. An
explicit flag always beats the table, which is how a self-hosted instance gets its grammar;
the github and gitea dialects refuse a nested owner they could never match, and recognizing
a dialect authenticates nothing about how the run was invoked.

`--format json` emits the exact report described in [The report](report.md); `human` prints
the same facts as at most ten grouped Fix and Check items, each naming only its target and
affected-place count, with one fixed `note` line per error code using the sentences from
[Limits and refusals](limits.md), while the full findings remain in JSON. `--explain-scope`
adds its lines to that human output without changing JSON or creating an early exit,
behavior pinned by the
[CLI tests](https://github.com/HardMax71/amiss/tree/main/crates/amiss/tests/cli). Exit
codes are three classes, not detail: 0 means the run completed and nothing blocks, 1 means
a finding blocks, 2 means nothing trustworthy could be produced. A consumer that closes the
pipe early, `head` among them, ends the printing and not the verdict.

`amiss fix` is the repair form of the same evaluation, over the staged state only. It runs
the check the grammar's first form runs with `--index`, takes every finding whose `fix` is
not null, and rewrites exactly those byte spans in the working tree. It applies nothing on
faith: a document is repaired only while its working-tree bytes still equal the staged
bytes the fixes were computed against, a file already holding the repaired bytes counts as
already fixed rather than drifted, and a symlink, an unreadable file, or overlapping spans
refuse that document whole. Its output is the repair itself, one row per document and a
summary line, so `--format`, `--explain-scope`, and `--candidate` are refused rather than
ignored. Exit 0 means every carried fix was applied or already present, 1 means at least
one document refused, and 2 means the evaluation itself could not be trusted and nothing
was touched. Restage and rerun `amiss check` to see the repaired state judged.

`amiss adopt` is the onboarding form: it runs the same evaluation under enforce and mints
a [debt snapshot](controls.md) from every blocking, debt-eligible finding, so a repository
with legacy drift can start gating new drift the same day while the recorded debt is
worked off. The engine supplies each item's key and accepted fact from the evaluation
itself; the flags supply what it cannot know, the floor digest the snapshot binds to, the
owner the floor must authorize, the reason, both instants in the wire's own clock
grammar, and the output path, which must not already exist. Adoption records a committed
tree, so the staged selector is refused, and the repository identity triple is part of
the form's own grammar because the snapshot binds one. The minted file is written only after the engine's own
reader accepts it, and blocking findings outside the debt-eligible kinds are counted in
the summary line and left to be fixed. Exit 0 means the snapshot was recorded, 1 means
the output path refused, and 2 means nothing trustworthy could be recorded, whether the
evaluation itself failed, the report carried no candidate tree, or the minted bytes were
refused by the engine's own reader.

`amiss --version` is the fourth form of the grammar and the only one that is not a scan. It
carries no other flag, prints two lines to stdout, and exits 0:

```text
amiss <version>
engine sha256:<64 hex digits>
```

The first line names the binary and the version it was built from. The second is the same
`engine_digest` that this binary stamps into every report it writes and that the
[release manifest](https://github.com/HardMax71/amiss/blob/main/spec/scanner-release-manifest.schema.json)
pins for each published platform, so an installed binary can be matched against a release
row, and a report can be matched back to the binary that produced it, without running a
scan. A binary that cannot read its own file prints `engine unavailable` on the second line
and still reports its version. The other shipped binaries answer `--version` too, each with
one line naming itself: `amiss-bootstrap`, `amiss-manifest`, `amiss-constraint`, and the
three provider services.
