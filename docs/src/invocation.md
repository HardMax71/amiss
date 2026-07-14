# Invocation

Install from crates.io, or build from source:

```text
cargo install amiss
```

The command line is closed. Every option below is the whole grammar, each may appear at most
once, order does not matter, and anything else is an invalid invocation. There is no `--help`,
which is why the grammar is written out here.

```text
amiss check --repo <path> --object-format <sha1|sha256>
            --base <full-oid> (--candidate <full-oid> | --index)
            [--repository github.com/<owner>/<name>
             --ref refs/heads/<name>
             --default-branch-ref refs/heads/<name>]
            --profile <observe|enforce>
            [--explain-scope] [--format <human|json>]
```

`--base` and `--candidate` take full object IDs, never refs or abbreviations: Amiss evaluates
the exact trees you name and resolves nothing on your behalf. Use `--index` in place of
`--candidate` to evaluate the staged index against a base commit; a
[skip-worktree](https://git-scm.com/docs/git-update-index) entry is still part of that
snapshot, read from the index like any other.

The optional `--repository` triple is what turns a
`https://github.com/<owner>/<name>/blob/...` URL in your prose into a path Amiss will actually
check. Without it those links are foreign URLs and go unchecked, which the report says out
loud. The owner and the name must be given in lowercase, and GitHub reports them with their
original capitals, so a workflow passing `github.repository` has to lowercase it first. Amiss
will not do that for you: the identity you pass is a claim it cannot authenticate, the report
has no field to record what you originally typed, and quietly rewriting an unverifiable claim
is not something a checker gets to do. It refuses instead, and says why.

`--explain-scope` prints the discovery scope rules and exits. `--format json` emits the
canonical report described in [The report](report.md); `human` prints a projection of the same
facts, capped at the first two hundred findings in canonical order.

Exit codes are classes, not details. Exit 0 is a complete run with nothing blocking. Exit 1 is
a complete run with at least one blocking finding. Exit 2 is anything that stopped Amiss from
producing a result it trusts: an unreadable repository, an invalid invocation, a resource
ceiling crossed, a document it could not decode. The report distinguishes the details; the exit
code only ever promises which of the three worlds you are in.
