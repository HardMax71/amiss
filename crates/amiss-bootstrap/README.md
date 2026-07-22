# amiss-bootstrap

The trusted CI wrapper for Amiss. It validates a pinned action tree as plain data, checks
the release manifest, the runtime closure, and the engine digest, and only then launches the
verified engine with a cleared environment, fixed arguments, and a wall-clock timeout. The
one crate in the workspace allowed to start a process.

The sealed entry is:

```text
amiss-bootstrap exec --action-repository P --repository P --constraint F \
  --evaluation-request F --snapshot-request F --controls-request F --result F
```

`--result` is required. It must name an absolute path that does not exist. Bootstrap creates
the file without replacing an existing file and writes one short, versioned result only after
the run is settled. A malformed command creates no result. Accepted report bytes are flushed to
standard output before a `pass` or `block` result becomes visible.

Part of [Amiss](https://hardmax71.github.io/amiss/).
