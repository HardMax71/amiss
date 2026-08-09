# Claims

A document can do more than point at a file: it can pin what the file says. A value claim
is a reserved reference definition asserting that one line of one repository file,
terminator aside, is exactly one expected text. The scanner evaluates the claim on every
run, so the page that states a version number or a default stops being a promise and
becomes a checked fact. Authoring one is a single command:
[`amiss claim`](invocation.md) reads the line and prints a definition it has already
proven against the extractor and this grammar.

## The grammar

```markdown
[amiss:release-version]: <amiss:value?path=Cargo.toml&line=L3> "version = \"0.16.0\""
```

The grammar is closed, and every clause of it is load-bearing:

- The label after `amiss:` names the claim. A name starts with an ASCII letter or digit,
  continues in letters, digits, `.`, `_`, or `-`, and holds at most 120 bytes. The name
  heads the finding's rule id, `claim/value/<name>`, so it carries no slash.
- The destination is angle-bracketed and spells `amiss:value` with exactly two
  parameters, `path` then `line`. The path is repository-root relative, taken byte for
  byte with no percent decoding, and must satisfy the repository path grammar. The line
  is `L` followed by a number with a nonzero first digit, at most sixteen digits, within
  the safe integer range.
- The title carries the expected text, decoded by the CommonMark title rules. An empty
  title is lawful and claims an empty line.

A reserved definition that misses any clause is not a lesser claim: it stays an
unsupported capability, and the run ends incomplete with exit 2, exactly as before the
value kind existed. A reference definition is invisible in rendered output, so a claim
adds nothing to the page a reader sees.

## The carriers

Every structured format has an invisible construct that carries the same line. Markdown
and MDX use the reference definition above. reStructuredText uses a comment holding
exactly the one carrier line, and AsciiDoc a line comment:

```text
.. [amiss:release-version]: <amiss:value?path=Cargo.toml&line=L3> "0.16.0"
// [amiss:release-version]: <amiss:value?path=Cargo.toml&line=L3> "0.16.0"
```

The comment carriers take their bytes literally: no entity decoding, the title in double
or single quotes, nothing else on the line, and in reStructuredText nothing else in the
comment, since a comment holding anything more stays opaque exactly as before. The one
recognized spelling is the whole carve-out from comment opacity. `amiss claim` prints
the Markdown line; prefix it with `.. ` or `// ` for the other two, and a broken
claim's fix respells whichever carrier it lives in, marker included.

## Evaluation

The claim's target must be a regular or executable blob in the candidate snapshot. The
line answers without the terminator that ended it, whatever spelling that terminator
used, and the answer must equal the expected text byte for byte. Reading the target
charges the same referenced-target and line-fragment budgets a reference line fragment
charges, through the same cache, so a claim and a link to the same file cost one read.

Claims are evaluated on the candidate side only. A claim speaks in the present tense
about the tree it rides with, so there is no pre-existing exemption to ramp away: its
attribution is `not-applicable`, and the enforce-introduced profile treats a broken
claim exactly as enforce does.

## Outcomes

An attested claim produces no finding. The summary counts it in `governed_claims`, and
`unattested_claims` counts the claims that failed, so the two numbers say how much of
the document's asserted surface held.

A claim that fails produces one of two findings, warn under observe and fail under both
enforce profiles:

- `claim-broken`: the target line exists and says something else. The finding's evidence
  carries the expected and observed digests.
- `claim-target-missing`: nothing can answer, and the evidence names why: the path is
  absent, the target is not a blob, the target is an LFS pointer, or the line is out of
  range.

Claims sharing one name in one document aggregate per outcome kind: every broken
member joins one `claim-broken` finding and every unanswered member joins one
`claim-target-missing` finding, each carrying its contributing source digests, the
way governed boundaries already aggregate.

## The fix a broken claim carries

A `claim-broken` finding standing alone carries a machine-applicable `fix`: the whole
definition respelled with the observed line as its expected words, the byte span of the
definition to replace, and the document that holds it. The engine emits the fix only
when it can prove it: the rewritten definition is parsed back through the real
extractor and must classify to the identical claim with the new expected words, so an
observed line the quoted-title grammar cannot spell (a double quote, a backslash, a
control byte, or bytes outside UTF-8) leaves the field null, and so does a name whose
definitions aggregate, since grouped members share one finding but not one edit. A
`claim-target-missing` finding never carries a fix, because nothing derivable says
where the target went.
