# Invocation

Install from [crates.io](https://crates.io), or build from source:

```sh
cargo install amiss
```

The command line is closed: the grammar below is everything, each option appears at most
once, order does not matter, and anything else is rejected as an invalid invocation. There
is no `--help`, which is why the grammar is written out here.

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

`--base` and `--candidate` take full commit IDs, never branch names or short forms. Amiss
evaluates exactly the trees you name and resolves nothing for you. Use `--index` instead of
`--candidate` to check what is currently staged against a base commit. An entry marked
[skip-worktree](https://git-scm.com/docs/git-update-index) is still part of the staged
state and is read from the index like everything else.

The optional identity group (`--repository`, `--ref`, and `--default-branch-ref`) tells
Amiss which repository and candidate branch this tree belongs to. With a selected forge
dialect, a link like
`https://github.com/<owner>/<name>/blob/main/src/lib.rs` becomes a repository path only when
`--ref` is `refs/heads/main`; a URL for the declared default branch while another candidate
is under test is recognized but remains `unsupported-version-scope`. Without the identity
group, forge links remain external URLs and are skipped, and the report says so. The host
is any spelling without a slash: Amiss never resolves it, and it is matched byte for byte
against the URLs in your documents, so pass the lowercase form your links actually use. The
owner is one or more slash-joined segments, nested only for GitLab group paths, and owner and
name must be lowercase. Forges report them with whatever capitals the owner registered, so
a workflow passing `github.repository` has to lowercase the value first. Amiss will not do
that for you: the identity you pass is a claim it cannot verify, and a checker that silently
rewrites an unverifiable claim has started making things up. It refuses instead, and the
refusal says why.

`--forge` names the URL dialect the resolver applies: `github` for GitHub and GitHub
Enterprise, `gitlab` for GitLab's separator form, `gitea` for Gitea, Forgejo, and
Codeberg. Without the flag, github.com, gitlab.com, and codeberg.org select their own
dialects and any other host selects none, in which case absolute links to that host stay
foreign and the report's `evaluation.forge` is null. An explicit flag always beats the
table, which is how a self-hosted instance gets its grammar. The github and gitea dialects
refuse a nested owner they could never match.

`--explain-scope` does not create a separate early-exit command. The scan still runs, and in
human format the flag adds deterministic scope lines to the normal result. JSON output is
unchanged by the flag. This behavior is pinned by the
[CLI test](../../crates/amiss/tests/cli.rs).
`--format json` emits the exact report described in [The report](report.md); `human` prints
the same facts readably, capped at the first two hundred findings.

Exit codes are three classes, not detail. Exit 0: the run completed and nothing blocks. Exit
1: the run completed and at least one finding blocks. Exit 2: something prevented a
trustworthy result, an unreadable repository, a bad invocation, a crossed limit, an
undecodable document. Details live in the report; the exit code only tells you which of the
three worlds you are in.
