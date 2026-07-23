# Invocation

Install from [crates.io](https://crates.io), or build from source:

```sh
cargo install amiss
```

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
            --profile <observe|enforce>
            [--explain-scope] [--format <human|json>]
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
| `--profile` | `observe` or `enforce` | report only, or let blocking findings gate |
| `--explain-scope` | none | adds deterministic scope lines to human output |
| `--format` | `human` or `json` | ten grouped items, or the exact report |

`--base` and `--candidate` take full commit IDs, never branch names or short forms. Amiss
evaluates exactly the trees you name and resolves nothing for you. Use `--index` instead of
`--candidate` to check what is currently staged against a base commit. An entry marked
[skip-worktree](https://git-scm.com/docs/git-update-index) is still part of the staged
state and is read from the index like everything else.

The optional identity group (`--repository`, `--ref`, and `--default-branch-ref`) tells
Amiss which repository and candidate branch this tree belongs to. With a selected forge
dialect, a link like `https://github.com/<owner>/<name>/blob/main/src/lib.rs` becomes a
repository path only when `--ref` is `refs/heads/main`; a URL for the declared default
branch while another candidate is under test is recognized but remains
`unsupported-version-scope`. Without the identity group, forge links remain external URLs
and are skipped, and the report says so.

This public `--ref` is the candidate or source ref; it is not the protected target branch and
does not authorize external controls. The direct command has no `--target-ref` option and emits
a null report target. The internal evaluation request used by the sealed bootstrap carries
`candidate_ref` and `target_ref` separately, and the branch-scoped control gates match the
latter.
`--default-branch-ref` is still URL-resolution context, not an implicit target. Consequently a
human CLI invocation cannot create a provider-authenticated run merely by spelling provider
identity fields.

The identity's spelling is strict. The host is any spelling without a slash: Amiss never
resolves it, and it is matched byte for byte against the URLs in your documents, so pass
the lowercase form your links actually use. The owner is one or more slash-joined
segments, nested only for GitLab group paths, and owner and name must be lowercase.
Forges report them with whatever capitals the owner registered, so a workflow passing
`github.repository` has to lowercase the value first. Amiss will not do that for you.
The identity you pass is a claim it cannot verify, a checker that silently rewrites an
unverifiable claim has started making things up, and so it refuses, and the refusal says
why.

`--forge` names the URL dialect the resolver applies: `github` for GitHub and GitHub
Enterprise, `gitlab` for GitLab's separator form, `gitea` for Gitea, Forgejo, and
Codeberg. Without the flag, github.com, gitlab.com, and codeberg.org select their own
dialects and any other host selects none, in which case absolute links to that host stay
foreign and the report's `evaluation.forge` is null. An explicit flag always beats the
table, which is how a self-hosted instance gets its grammar. The github and gitea dialects
refuse a nested owner they could never match.
This dialect selection is independent of the provider-controller lane. Recognizing a forge URL
does not authenticate it or prove that this ordinary CLI run came through one of the supported
provider services.

`--explain-scope` does not create a separate early-exit command. The scan still runs, and in
human format the flag adds deterministic scope lines to the normal result. JSON output is
unchanged by the flag. This behavior is pinned by the
[CLI test](https://github.com/HardMax71/amiss/blob/main/crates/amiss/tests/cli.rs).
`--format json` emits the exact report described in [The report](report.md); `human` prints
the same facts as a focused list of at most ten grouped Fix and Check items. Each item names
only its target and affected-place count; the full findings and their kinds remain in JSON.
Under the detail lines, `human` also prints one fixed `note` line per distinct error code,
using the same sentences listed in [Limits and refusals](limits.md), so a failed scan can be
acted on without the book open.

Exit codes are three classes, not detail. Exit 0: the run completed and nothing blocks. Exit
1: the run completed and at least one finding blocks. Exit 2: something prevented a
trustworthy result, an unreadable repository, a bad invocation, a crossed limit, an
undecodable document. Details live in the report; the exit code only tells you which of the
three worlds you are in. A consumer that closes the pipe early, `head` among them, ends
the printing and not the verdict: the exit class reports the run, never the state of
stdout.
