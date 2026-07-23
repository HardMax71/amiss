# Snapshots

A run reads exactly two states of the repository: a base and a candidate. Each is named by a
full commit ID, or the candidate can be the staged index. Nothing else counts as input.
There is no working-directory mode, no branch-name resolution, and no fetching. If a needed
object is not in the local object store, the run refuses; see
[Limits and refusals](limits.md).

Branch refs describe identity and link scope; they never select either snapshot. The rolling
request and report contracts carry `candidate_ref`, the candidate or source branch whose links
are being evaluated, separately from `target_ref`, the protected branch to which branch-scoped
controls bind. A direct branch update normally uses the same value for both. A
pull or merge request may use a feature branch as the candidate and the protected base branch
as the target.
The default-branch ref is a third fact used for URL resolution and is not inferred to be the
target. The public CLI exposes only its existing candidate `--ref` claim; the complete split is
currently reachable only through the internal request/bootstrap surface.

In staged-index mode the identity covers the complete logical stage-zero index, including
skip-worktree entries. The digest chain has three steps: hash the sorted
[index projection](https://github.com/HardMax71/amiss/blob/main/spec/examples/index-projection.json),
hash a
[synthetic snapshot](https://github.com/HardMax71/amiss/blob/main/spec/examples/synthetic-snapshot.json)
over that projection, then bind the result into the staged
[candidate identity](https://github.com/HardMax71/amiss/blob/main/spec/examples/candidate-identity-index.json).
A commit-pair run uses the corresponding
[commit candidate identity](https://github.com/HardMax71/amiss/blob/main/spec/examples/candidate-identity.json).
Both refs are part of that identity preimage, alongside the repository, selected URL dialect,
base, and candidate, so a trusted-time statement bound to one source/target relationship cannot
be replayed for another.
These JSON files are digest preimages, not accepted request documents, and the
[identity golden test](https://github.com/HardMax71/amiss/blob/main/crates/amiss-scan/tests/identity.rs)
validates each against its report-schema definition and reproduces the full chain through
the production builders.

The provider controller uses a stricter orchestration identity containing the provider instance,
repository and change, URL dialect, candidate, target and default-branch refs, object format, base
and candidate commits, and both tree IDs. Each
[provider lane](provider-controls.md) binds that identity from independently authenticated input,
refreshes the change and protected merge gate through its own credential, and acquires
authenticated SHA-1 commit wants through the same fixed-budget Git protocol-v2 path before
launch. Bootstrap refuses unless the acquired roots reproduce the evaluation identity. The
engine then reads and re-hashes their objects normally; fetching remains outside the engine.
The provider's Check Run, policy-job result, or dedicated review remains merge evidence rather
than a third engine snapshot.

The snapshot request's `repository_handle: 3` is a stable protocol ordinal, not a claim that
the operating system passed file descriptor 3. In the current safe-Rust subprocess path the
bootstrap maps that logical handle to the fixed repository working directory before launch, and
the engine opens only that directory. A future isolation backend may map the same ordinal to a
different pre-opened mechanism without changing the request wire.

Amiss reads [Git](https://git-scm.com)'s storage itself instead of asking the `git` command. Loose objects,
packfiles, deltas, and the index file are parsed by the engine, and the parsers reject
instead of repairing. A tree with entries out of order, an index whose checksum does not
match, a delta chain deeper than the published limit: each one is a typed refusal, never a
best-effort read. Every SHA-1 object is re-hashed as it is read, with collision detection
switched on, so an object that does not hash to its own name simply does not exist as far
as the evaluation is concerned.

The supported repository form is a primary non-bare checkout with a real `.git` directory.
A bare repository or linked worktree whose `.git` entry is a file is refused as unavailable,
and objects available only through Git alternates are not consulted. These are explicit
boundaries of the direct
[repository reader](https://github.com/HardMax71/amiss/blob/main/crates/amiss-git/src/repo.rs),
not empty snapshots or silently missing documents.

File access happens through directory handles opened step by step, never following links.
A symlink, junction, or reparse point at the repository root, at `.git`, at `objects`, or
anywhere along the path to an object is refused outright. The refusal is a different error
from the object being absent, and that difference is deliberate: someone who can plant a
link must not be able to make the scanner read files outside the repository, and must
equally not be able to disguise the attempt as a missing object.

What a refusal looks like in the report's `errors` array, here for a base commit the
store does not hold:

```json
{
  "code": "GIT_OBJECT_MISSING",
  "phase": "git"
}
```

The row also carries `path`, `path_bytes_hex`, `resource`, `configured_limit`, and
`observed_lower_bound` fields, null wherever they do not apply, so every refusal has the
same shape and a consumer never parses two formats. When the refused thing is a name the
path grammar rejects, `path_bytes_hex` holds its exact bytes as lowercase hex, so the
report never swallows what it refused.

Neither snapshot is trusted more than the other. A base commit missing from a shallow clone
is a refusal, not an empty tree, because treating an absent base as empty would make every
document look newly added and flood the report with false `introduced` findings. Comparing
two trees only means something when both trees are exactly the ones you asked for.
