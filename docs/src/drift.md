# Documentation drift

Documentation drift is the disagreement that accumulates between a repository's documents
and its tree. The usual shapes: a link to a file that was renamed two months ago, a
hand-written count ("ten workflows") in a tree that has 22, a paragraph that kept
explaining a function long after the function was rewritten under it. Nobody notices
until a reader trusts the page and loses an afternoon.

Here is the smallest version of it. A pull request tightens a retry limit and renames the
module, touching nothing under `docs/`:

```diff
--- a/src/retry.rs
+++ b/src/backoff.rs
@@ -1 +1 @@
-pub const MAX_ATTEMPTS: u32 = 3;
+pub const MAX_ATTEMPTS: u32 = 5;
```

The operations page keeps reading:

```markdown
Retries are capped at three attempts; the limit lives in [`src/retry.rs`](../src/retry.rs).
```

The paragraph didn't change, so nothing in review looks at it. Amiss does: the link's
target is gone from the candidate tree, which blocks under `enforce`. Change the constant
in place instead and you get the warn: changed bytes under an unchanged paragraph. Neither
finding says "three" is now a lie. That call belongs to whoever reads the finding.

The [audit behind this tool](evidence.md) went through one repository that took
documentation seriously: golden files, executable CLI examples, a link checker, roughly a
dozen hand-built defenses. It still held seven live drift classes. The architecture page
counted ten workflows against 22 in the tree and named one that never existed. The CLI
reference documented a three-value exit-code contract while the code used four. Railroad
diagrams regenerated on every docs build, faithfully, from a stale copy of the grammar
embedded in the generator script. The fresh output proved one thing: the stale input still
compiled. Executable examples all stayed green, since examples protect the paths they
execute and nothing else.

Checkers that run on demand inherit the failure they exist to catch, since the person who
forgot to update the page also forgot to run the checker. Tools that rewrite prose to
match the code make a different mistake: deciding what the code means is the one judgment
a machine should refuse. So Amiss splits the work. The rewrites the engine can prove ship
as fixes `amiss fix` applies byte for byte: a path off only by case from one tracked
spelling, an anchor off by case or separator style, a claim expecting a line's old text.
Everything that needs judgment goes to someone who can be held to account: a person, or a
coding agent reading the finding's own description.

Every run compares two exact snapshots. Under `enforce`, a reference that stops resolving
blocks the change that broke it. A referenced file that changed under an unchanged
paragraph warns; [Correlation and impact](correlation.md) draws that boundary precisely.
The code moving is a reason to reread the prose, not proof the prose is wrong. What the
tool cannot see, it declares. And repository policy can raise severity but never lower it,
so the gate survives the kind of change it exists to catch.

The full taxonomy of what a scan establishes is in
[Profiles and findings](profiles.md). What Amiss deliberately does not attempt, starting
with reading your prose, is in [What Amiss is not](non-goals.md).
