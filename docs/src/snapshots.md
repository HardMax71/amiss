# Snapshots

A run reads exactly two states of the repository: a base and a candidate. Each is named by a
full commit ID, or the candidate can be the staged index. Nothing else counts as input.
There is no working-directory mode, no branch-name resolution, and no fetching. If a needed
object is not in the local object store, the run refuses; see
[Limits and refusals](limits.md).

In staged-index mode the identity covers the complete logical stage-zero index, including
skip-worktree entries. The engine hashes the sorted
[index projection](../../spec/examples/index-projection.json), hashes a
[synthetic snapshot](../../spec/examples/synthetic-snapshot.json) over that projection, and
then binds the result into the staged
[candidate identity](../../spec/examples/candidate-identity-index.json). A commit-pair run
uses the corresponding [commit candidate identity](../../spec/examples/candidate-identity.json).
These JSON files are digest preimages, not accepted request documents. Their
[identity golden test](../../crates/amiss-scan/tests/identity.rs) validates each one against
its report-schema definition and reproduces the full digest chain through the production
builders.

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
[repository reader](../../crates/amiss-git/src/repo.rs),
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
