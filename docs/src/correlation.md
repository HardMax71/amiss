# Correlation and impact

The base-versus-candidate comparison works per occurrence, and the unit it reasons about is
the block: the paragraph, list item, or table cell that contains the reference. An
occurrence in the candidate is matched to its base counterpart when it is the same
reference, spelled the same way, in the same structural place. That matching is called
correlation.

Correlation never guesses. If a page was rewritten so heavily that two identical references
could each descend from either of two originals, the result is an
`observation-correlation-ambiguous` finding with attribution `unknown`, not a coin flip.
Repeats are merged: one finding per distinct fact, carrying a count, so a link repeated in
five places is one row with a multiplicity of five rather than five rows.

For each matched pair, the two snapshots tell one of three stories:

- `subject-changed`: the block holding the reference changed.
- `dependency-changed-subject-unchanged`: the referenced file changed and the block did
  not. This is the finding the tool exists for, and it never blocks: the code moved and the
  prose did not, which is a reason for a person to look, not a machine's verdict that the
  prose is now wrong.
- `dependency-and-subject-cochanged`: both moved together, which is what a maintained page
  looks like, recorded at the lowest level.

The two-sided comparison reduces to a quadrant:

| | dependency unchanged | dependency changed |
| --- | --- | --- |
| **block unchanged** | no finding | `dependency-changed-subject-unchanged` |
| **block changed** | `subject-changed` | `dependency-and-subject-cochanged` |

And the finding the tool exists for, as a change:

```diff
 fn parse(input: &[u8]) -> Ast {
-    tokenize(input).fold(Ast::new(), Ast::push)
+    lex(input).try_fold(Ast::new(), Ast::push).unwrap_or_default()
 }
```

```markdown
The parser tokenizes the input and folds the tokens into the tree.
```

The code block moved and the paragraph did not: `dependency-changed-subject-unchanged`,
a warning in every profile, pointing a reviewer at the paragraph with the line and column
of the reference that ties them together.

Removals get their own kinds. `explicit-reference-removed` means a reference that existed
in the base is gone from the candidate. `document-removed` means the whole file left the
tree. Both are records, not accusations, because deleting stale prose is usually the fix,
and the report treats it as information about the change.

Formatting noise stays out by construction. Amiss does not normalize the content of
referenced files; any byte change in a target is a change, even a formatter's, because
every normalizer is a parser for someone else's language and each one shipped would be a
place for a real change to hide. For the block itself, the compared projection is
structural, so re-wrapping a paragraph without changing its text does not create fake
impact.
