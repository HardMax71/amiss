# Invocation

Install from crates.io, or build from source:

```sh
cargo install amiss
```

The command line is closed: the grammar below is everything, each option appears at most
once, order does not matter, and anything else is rejected as an invalid invocation. There
is no `--help`, which is why the grammar is written out here.

```text
amiss check --repo <path> --object-format <sha1|sha256>
            --base <full-oid> (--candidate <full-oid> | --index)
            [--repository github.com/<owner>/<name>
             --ref refs/heads/<name>
             --default-branch-ref refs/heads/<name>]
            --profile <observe|enforce>
            [--explain-scope] [--format <human|json>]
```

`--base` and `--candidate` take full commit IDs, never branch names or short forms. Amiss
evaluates exactly the trees you name and resolves nothing for you. Use `--index` instead of
`--candidate` to check what is currently staged against a base commit. An entry marked
[skip-worktree](https://git-scm.com/docs/git-update-index) is still part of the staged
state and is read from the index like everything else.

The optional `--repository` triple tells Amiss which GitHub repository this tree belongs to, which
turns links like `https://github.com/<owner>/<name>/blob/main/src/lib.rs` in your prose into
paths it can actually check. Without the triple, such links are treated as foreign URLs and
skipped, and the report says so. The owner and name must be lowercase. GitHub reports them
with whatever capitals the owner registered, so a workflow passing `github.repository` has
to lowercase the value first. Amiss will not do that for you: the identity you pass is a
claim it cannot verify, and a checker that silently rewrites an unverifiable claim has
started making things up. It refuses instead, and the refusal says why.

`--explain-scope` prints the scanning scope rules and exits. `--format json` emits the exact
report described in [The report](report.md); `human` prints the same facts readably, capped
at the first two hundred findings.

Exit codes are three classes, not detail. Exit 0: the run completed and nothing blocks. Exit
1: the run completed and at least one finding blocks. Exit 2: something prevented a
trustworthy result, an unreadable repository, a bad invocation, a crossed limit, an
undecodable document. Details live in the report; the exit code only tells you which of the
three worlds you are in.
