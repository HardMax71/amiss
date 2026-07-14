# amiss-git

Read-only Git object store for Amiss. Parses loose objects, packfiles, deltas, and the index
directly, under grammars that reject malformed input rather than repairing it, published
resource ceilings, SHA-1 collision detection on every read, and a directory-handle chain
that never follows a link.

Part of [Amiss](https://hardmax71.github.io/amiss/).
