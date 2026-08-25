# The external plan

A [report](report.md) retains a destination when its resolution delegates evidence to another
layer: an external URL the engine does not judge, or a same-repository exact commit whose required
objects are unavailable locally. The external plan is the pure derivation that turns one written
report into the work another layer may do: which distinct delegated destinations this change
introduced, which it removed, and where each one lives.

```sh
amiss external-plan --report report.json --format json
```

The command opens no repository and touches no network. It reads the report file,
verifies the payload against the digest the envelope records, and refuses anything less
than a complete report. The digest proves the payload is whole and untampered relative
to its own envelope, so corruption and truncation are refused; where the file came from
is the caller's supply chain, as it is for every other input. The derivation is set-wise
per side: a destination counts as introduced when the
candidate references it and the base does not anywhere in the tree, and as removed in
the mirror case. A destination that only moved between documents is neither; it is
counted under `retained_count` and never listed, which keeps the plan proportional to
the change rather than to the corpus.

Each row carries the destination exactly as the report recorded it, after the format's
own decoding, the address an evidence producer would request; its lowercased scheme; and the sorted
documents naming it. Unavailable exact history uses `https`, the only scheme accepted by the
same-repository forge grammar. A destination on a forge host the run can name, github.com,
gitlab.com, codeberg.org, or the report's own declared host under its declared dialect,
also carries a `repository` object: host, dialect, owner, name, then verbatim the path
segment after them as `form` and everything later as one opaque `tail`. The tail stays
unsplit on purpose. Branch names may contain slashes, so separating revision from path
needs the other repository's refs, and naming structure is not claiming the repository
exists; both belong to the verifying layer. The payload binds the report's own `payload_digest` and echoes its
evaluation identities, and the plan envelope carries a digest of its own payload under
the plan schema identity. A producer that probes the introduced list, and any later
judgment over that evidence, can therefore join plan, report, and evidence on one
identity without re-reading the tree. The schema is
[`scanner-external-plan.schema.json`](https://github.com/HardMax71/amiss/blob/main/spec/scanner-external-plan.schema.json),
and its example is derived from the report example by the same code path, checked in CI.

The plan states work; it performs none. Fetching stays outside the engine for the same
reasons [What Amiss is not](non-goals.md) gives for live URLs: a probe's answer varies
with the network's mood, and a guessed pass looks exactly like a real one. What a
producer observes comes back through [the external assessment](external-assessment.md),
and the composition with a checker that does fetch is one pipe, shown in
[Amiss and link checkers](comparison.md).

Exit 0 wrote the plan, human or JSON. Exit 2 means the input could not be trusted:
unreadable, larger than a scanner report can be, not the scanner's strict JSON, not a
report envelope, a payload that fails its recorded digest, an incomplete report, or an
eligible occurrence missing its destination, document, or required scheme. There is no exit 1,
since a plan carries data and no verdict.
