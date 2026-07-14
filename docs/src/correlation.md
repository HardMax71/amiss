# Correlation and impact

The comparison between base and candidate happens per occurrence, and the unit of correlation
is the source block: the paragraph, list item, or table cell that owns the reference. An
occurrence in the candidate is matched to its base counterpart by its extraction key and its
source projection, which is to say: the same reference, spelled the same way, in the same
structural place.

Correlation refuses to guess. When a document is heavily rewritten and two identical
references could each be the descendant of either of two base occurrences, the finding is
`observation-correlation-ambiguous` with attribution `unknown`, not a coin flip. The merge
strategy is one finding per finding key, so a reference repeated in five places is one fact
with a multiplicity, not five copies of the same fact.

Impact is the three-way story the two snapshots tell about each correlated pair:

- `subject-changed`: the block holding the reference changed between base and candidate.
- `dependency-changed-subject-unchanged`: the referenced file's bytes or mode changed and the
  block did not. This is the finding the tool exists for, and it is advisory by law: the code
  moved and the prose did not, which is a reason for a human to look, never a machine verdict
  that the prose is now wrong.
- `dependency-and-subject-cochanged`: both moved together, which is what a well-maintained
  page looks like and is recorded at the lowest severity.

Removals are their own kinds. `explicit-reference-removed` says a reference that existed in
the base is gone from the candidate; `document-removed` says the whole document left the
tree. Both are records of history, not judgments: deleting stale prose is usually the fix,
and the report treats it as information about the change rather than a regression.

Formatting-only changes stay advisory by construction. A change to a target's bytes is a
change even when a formatter made it; Amiss does not normalize target content, because a
normalizer is a parser for someone else's language and every one it shipped would be a place
to hide a real change. The projection it compares for the block itself is structural, so
reflowing a paragraph without changing its text does not manufacture impact.
