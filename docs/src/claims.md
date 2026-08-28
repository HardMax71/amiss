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
or single quotes, nothing after the closing quote, the comment at column zero, and in
reStructuredText nothing but blank lines after the carrier line, since a comment holding
anything more stays opaque exactly as before. The one recognized spelling is the whole
carve-out from comment opacity. `amiss claim` prints the Markdown line; prefix it with
`..` plus one space for reStructuredText or `//` plus one space for AsciiDoc, and a
broken claim's fix respells whichever carrier it lives in, marker included.

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
carrier respelled with the observed line as its expected words, marker included, the
byte span of the carrier to replace, and the document that holds it. The engine emits
the fix only when it can prove it: the rewritten carrier is parsed back through the
real extractor for its own format and must classify to the identical claim with the
new expected words, so an
observed line the quoted-title grammar cannot spell (a double quote, a backslash, a
control byte, or bytes outside UTF-8) leaves the field null, and so does a name whose
definitions aggregate, since grouped members share one finding but not one edit. A
`claim-target-missing` finding never carries a fix, because nothing derivable says
where the target went.

## Policy-owned projections

A projection checks visible example text rather than an invisible expected title. Its relation
lives in `.amiss/scanner-policy.json`:

```json
{
  "document": "docs/api.md",
  "name": "request-shape",
  "projection": "code-text-v1",
  "sink": "previous-code",
  "source": {
    "kind": "blob-lines",
    "path": "examples/request.json",
    "first_line": 1,
    "last_line": 12
  }
}
```

The Markdown or MDX document addresses the visible sink with an invisible definition immediately
after its code block:

````markdown
```json
{"operation":"check"}
```
[amiss:request-shape]: <amiss:projection>
````

Whitespace may separate the code block and marker; prose or another node may not. The name is
unique within the document and joins the policy row to the marker. It is deliberately not encoded
in the destination query, and neither the marker nor the source file owns the relation. That
separation gives deletion safe behavior: a missing marker is drift while the policy survives, and
a removed policy identity is policy weakening.

`code-text-v1` converts CRLF and bare CR endings to LF, then removes exactly one terminal LF to
match the parser's semantic code value. Every other byte, including indentation and trailing
spaces, remains significant. The first selector shown above takes inclusive raw source lines. A
movement-stable selector can instead name the bytes between two complete marker lines:

```json
{
  "kind": "named-region",
  "path": "examples/request.json",
  "start_marker": "// amiss:request:start",
  "end_marker": "// amiss:request:end"
}
```

Each marker is a distinct exact printable-ASCII token of at most 256 bytes, and the complete line
equal to that token must occur once. The scanner interprets no surrounding comment syntax or
embedded occurrence as a boundary. It excludes both complete marker lines and
refuses duplicate, missing, reversed, same-line, or non-UTF-8 regions as typed projection drift.
Edits outside the region do not affect its projected digest. Both selectors use the existing
bounded target cache and line-fragment meter; source bytes are never executed or fetched.

A complete tracked-path inventory uses the same visible sink without reading another source blob:

```json
{
  "document": "docs/examples.md",
  "name": "examples",
  "projection": "sorted-rows-v1",
  "sink": "previous-code",
  "source": {
    "kind": "tree-paths",
    "root": "examples",
    "suffix": ".md",
    "maximum_depth": 2
  }
}
```

The root must exist as a tree in a commit or as a directory implied by the staged index. The source
filters the discovery map Amiss already completed: descendants at or above `maximum_depth`, with an
optional exact suffix, become root-relative UTF-8 rows. Regular, executable, symlink, and gitlink
paths participate; tree entries themselves do not. `sorted-rows-v1` orders those rows by UTF-8
bytes and joins them with LF, without a terminal LF. No glob, repository rewalk, object read,
normalization, suffix stripping, or source-language rule is involved.

A qualifying non-UTF-8 path or path containing a control character refuses the projection instead
of disappearing from or splitting the authoritative set. A path outside the repository grammar
already makes the candidate globally incomplete before projection evaluation.

An attested projection emits nothing. Every nonattested relation emits one `projection-drift`
finding under `claim/projection/<name>`, points at the visible code block when one is uniquely
addressable, and carries only digests and byte counts rather than copying either full value into the
report. No projection fix is emitted: a later rewrite feature must first prove an exact editable
content span,
reparse the whole document, and re-evaluate the relation.
