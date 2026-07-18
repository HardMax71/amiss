# Documentation drift

Documentation drift is the disagreement that accumulates between a repository's documents
and its tree. The usual shapes: a link to a file that was renamed two months ago, a
hand-written count ("ten workflows") in a tree that has 22, a paragraph that kept
explaining a function long after the function was rewritten under it. Nobody notices
until a reader trusts the page and loses an afternoon.

The [audit behind this tool](evidence.md) went through one repository that took
documentation seriously: golden files, executable CLI examples, a link checker, roughly a
dozen hand-built defenses. It still held seven live drift classes. The architecture page
counted ten workflows against 22 in the tree and named one that never existed. The CLI
reference documented a three-value exit-code contract while the code used four. Railroad
diagrams regenerated on every docs build, faithfully, from a stale copy of the grammar
embedded in the generator script; the freshness of the output proved only that the stale
input still compiled. Executable examples all stayed green, because examples protect the
paths they execute and nothing else.

Checkers that run on demand inherit the failure they exist to catch, since the person who
forgot to update the page also forgot to run the checker. Tools that rewrite prose to
match the code make a different mistake: deciding what the code means is the one judgment
a machine should refuse. So Amiss detects and gates, and leaves repair to someone who can
be held to account, a person or a coding agent reading the finding's own description.

Every run compares two exact snapshots. Under `enforce`, a reference that stops resolving
blocks the change that broke it. A referenced file that changed under an unchanged
paragraph warns, a boundary [Correlation and impact](correlation.md) draws precisely,
because the code moving is a reason to reread the prose and no proof the prose is wrong.
What the tool cannot see, it declares. And repository policy can raise severity but never
lower it, so the gate survives the kind of change it exists to catch.

The full taxonomy of what a scan establishes is in
[Profiles and findings](profiles.md); what Amiss deliberately does not attempt, starting
with reading your prose, is in [What Amiss is not](non-goals.md).
