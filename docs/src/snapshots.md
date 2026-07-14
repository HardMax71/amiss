# Snapshots

Amiss evaluates exactly two trees: a base and a candidate. Each is named by a full object ID,
or the candidate is the staged index. Nothing else is a snapshot: there is no worktree mode,
no ref resolution, and no remote fetch. If the bytes are not in the object store, the run
refuses; see [Limits and refusals](limits.md).

The scanner reads the repository the way a forensic tool would, not the way a build tool
would. It never shells out to `git`. Loose objects, packfiles, deltas, and the index file are
parsed by the engine itself, under grammars that reject rather than repair: a tree with
entries out of order, an index whose checksum does not match, a delta chain deeper than the
contract allows, each is a typed refusal, never a best-effort read. Every SHA-1 object is
rehashed with collision detection as it is read, so a tree that does not hash to its own name
does not exist as far as the evaluation is concerned.

Filesystem access goes through a directory handle chain that never follows links. A symlink,
junction, or reparse point at the repository root, at `.git`, at `objects`, or anywhere along
an object's path is refused outright rather than followed, and the refusal is distinct from
the object being absent. The distinction matters: an attacker who can point a path somewhere
else must not be able to make the scanner read files outside the repository, and must equally
not be able to disguise that redirection as a missing object.

Base and candidate get no special trust in either direction. The base commit may be absent
from a shallow clone; that is a refusal, not an empty comparison, because treating an absent
base as an empty one would turn the cheapest checkout misconfiguration into a wall of
`introduced` findings. The two-tree comparison only means something when both trees are
exactly what was asked for.
