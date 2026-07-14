# Snapshots

A run reads exactly two states of the repository: a base and a candidate. Each is named by a
full commit ID, or the candidate can be the staged index. Nothing else counts as input.
There is no working-directory mode, no branch-name resolution, and no fetching. If a needed
object is not in the local object store, the run refuses; see
[Limits and refusals](limits.md).

Amiss reads Git's storage itself instead of asking the `git` command. Loose objects,
packfiles, deltas, and the index file are parsed by the engine, and the parsers reject
instead of repairing. A tree with entries out of order, an index whose checksum does not
match, a delta chain deeper than the published limit: each one is a typed refusal, never a
best-effort read. Every SHA-1 object is re-hashed as it is read, with collision detection
switched on, so an object that does not hash to its own name simply does not exist as far
as the evaluation is concerned.

File access happens through directory handles opened step by step, never following links.
A symlink, junction, or reparse point at the repository root, at `.git`, at `objects`, or
anywhere along the path to an object is refused outright. The refusal is a different error
from the object being absent, and that difference is deliberate: someone who can plant a
link must not be able to make the scanner read files outside the repository, and must
equally not be able to disguise the attempt as a missing object.

Neither snapshot is trusted more than the other. A base commit missing from a shallow clone
is a refusal, not an empty tree, because treating an absent base as empty would make every
document look newly added and flood the report with false `introduced` findings. Comparing
two trees only means something when both trees are exactly the ones you asked for.
