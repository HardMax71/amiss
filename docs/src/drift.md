# Documentation drift

Documentation drift is the growing disagreement between what a repository's documents
claim and what its tree holds. A page names a file that was renamed two months ago. A
count says ten workflows where the tree has 22. A paragraph explains a function that was
rewritten under it, word for word as true as the day it was written about code that no
longer exists. None of these announce themselves; each is discovered by a reader who
trusted the page and lost time.

Drift is not a hygiene problem at the margins. The
[audit behind this tool](evidence.md) examined one repository that already ran a dozen
hand-built defenses, golden files, executable examples, a link checker, and still held
seven live drift classes: stale hand-written counts in every place they appeared, a
documented exit-code contract one value short of the code, diagrams regenerated forever
from a stale embedded copy of the grammar. The lesson that shaped Amiss: freshness of a
generation step proves derivation from the step's input, not truth, and examples protect
only the paths they execute.

Two popular answers fail in opposite directions. Tools that only run when someone thinks
to run them inherit the exact failure they exist to catch, since the person who forgot to
update the page also forgot to run the checker. And tools that rewrite prose to match the
code have quietly decided what the code means, which is the one judgment a machine should
refuse; a paragraph rewritten by a guesser is drift with better grammar.

Amiss takes the third position: deterministic detection as a gate, repair left to someone
who can be held to account, whether that is a person or a coding agent reading the
finding's own description. Every change is compared as two exact snapshots. A reference
that stops resolving blocks under `enforce`. A referenced file that changed under an
unchanged paragraph is a warning for a reader, never a machine verdict that the prose is
wrong, a boundary [Correlation and impact](correlation.md) draws precisely. What the tool
cannot see, it declares instead of skipping. And the rules themselves ratchet: repository
policy can raise severity and never lower it, so the gate cannot be quietly loosened by
the change it would have caught.

The full taxonomy of what a scan establishes is in
[Profiles and findings](profiles.md); what Amiss deliberately does not attempt, starting
with reading your prose, is in [What Amiss is not](non-goals.md).
