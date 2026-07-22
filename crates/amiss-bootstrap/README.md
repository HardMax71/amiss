# amiss-bootstrap

The trusted CI wrapper for Amiss. It validates a pinned action tree as plain data, checks
the release manifest, the runtime closure, and the engine digest, and only then launches the
verified engine with a cleared environment, fixed arguments, and a wall-clock timeout. The
one crate in the workspace allowed to start a process.

The sealed entry is:

```text
amiss-bootstrap exec --action-repository P --repository P --constraint F \
  --evaluation-request F --snapshot-request F --controls-request F --scratch P \
  --report F --result F
```

`--scratch` is a controller-owned absolute directory for the private verified engine copy; the
bootstrap never discovers it through ambient environment variables. `--report` and `--result`
are required. They must be distinct, controller-created empty regular files named `report` and
`result` directly inside `--scratch`. Bootstrap opens those files without replacing them. An
accepted report is written and flushed to `--report` first; the short, versioned `pass` or `block`
record is then written to `--result` as its commit marker. Failed evaluations leave the report
empty and publish only their result record. A report without its result marker is ignored. An
invalid invocation leaves both files untouched, and standard output carries no report bytes.

Part of [Amiss](https://hardmax71.github.io/amiss/).
