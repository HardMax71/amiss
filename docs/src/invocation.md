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
| `--profile` | `observe` or `enforce` | report only, or let blocking findings gate; see [Profiles and findings](profiles.md) |
| `--explain-scope` | none | adds deterministic scope lines to human output |
| `--format` | `human` or `json` | ten grouped items, or the exact report in [The report](report.md) |

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
[CLI test](https://github.com/HardMax71/amiss/blob/main/crates/amiss/tests/cli.rs). Exit
codes are three classes, not detail: 0 means the run completed and nothing blocks, 1 means
a finding blocks, 2 means nothing trustworthy could be produced. A consumer that closes the
pipe early, `head` among them, ends the printing and not the verdict.
