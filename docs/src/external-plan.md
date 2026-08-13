# The external plan

A [report](report.md) records every external destination where it was seen and decides
nothing about it, because the engine never fetches one. The external plan is the pure
derivation that turns one written report into the work another layer may do: which
distinct destinations this change introduced, which it removed, and where each one lives.

```sh
amiss external-plan --report report.json --format json
```

The command opens no repository and touches no network. It reads the report file,
verifies the payload against the digest the envelope records, and refuses anything less
than a complete report, so a plan can only exist for bytes the scanner actually stood
behind. The derivation is set-wise per side: a destination counts as introduced when the
candidate references it and the base does not anywhere in the tree, and as removed in
the mirror case. A destination that only moved between documents is neither; it is
counted under `retained_count` and never listed, which keeps the plan proportional to
the change rather than to the corpus.

Each row carries the destination exactly as the report recorded it, after the format's
own decoding, the address a fetcher would request; its lowercased scheme; and the sorted
documents naming it. The payload binds the report's own `payload_digest` and echoes its
evaluation identities, and the plan envelope carries a digest of its own payload under
the plan schema identity. A producer that probes the introduced list, and any later
judgment over that evidence, can therefore join plan, report, and evidence on one
identity without re-reading the tree. The schema is
[`scanner-external-plan.schema.json`](https://github.com/HardMax71/amiss/blob/main/spec/scanner-external-plan.schema.json),
and its example is derived from the report example by the same code path, checked in CI.

The plan states work; it performs none. Fetching stays outside the engine for the same
reasons [What Amiss is not](non-goals.md) gives for live URLs: a probe's answer varies
with the network's mood, and a guessed pass looks exactly like a real one. The
composition with a checker that does fetch is one pipe, shown in
[Amiss and link checkers](comparison.md).

Exit 0 wrote the plan, human or JSON. Exit 2 means the input could not be trusted:
unreadable, not the scanner's strict JSON, not a report envelope, a payload that fails
its recorded digest, or an incomplete report. There is no exit 1, since a plan carries
data and no verdict.
