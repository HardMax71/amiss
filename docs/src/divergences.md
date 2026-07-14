# Edge cases and divergences

The suite's edge cases are where the contract earns its wording. A sample of the ones that
shaped it:

Paths are bytes, so `été.txt` stages and resolves without normalization, and a path that is
case-distinct from another is a different path even on a filesystem that would fold them. A
directory reference resolves the same through a commit and through the index, which sounds
obvious until an index-only lookup sees no directory entries and has to prove containment
from sorted path prefixes. A reference to `guide.md/` fails as a type mismatch while
`guide.md` resolves beside it. `%252F` in a link decodes once and stays contained instead of
becoming a path separator on the second decode.

Hostile inputs get the same treatment as honest ones, just with more suspicion. A document
path carrying ANSI escapes, a bell, and a forged workflow command renders inert in the human
output and round-trips exactly in the JSON. A tree entry named with bytes no operating
system would accept is an `UNREPRESENTABLE_PATH` refusal rather than a dropped document. A
5,000-byte path is a charged crossing of the raw-path contract, reported with both numbers. A
tracked blob whose object is missing from the store refuses and names the document instead of
guessing at its content.

The parser pin records its divergences instead of hiding them. Against the pinned grammar
bundle and github.com's own rendering, the conformance manifest holds exactly one
extraction-relevant difference: `[link[^1]](#)`, a footnote call inside a link label, which
the pinned Rust parser does not form into a link. A reference written that way would go
unseen, which is under-reporting, the safer direction to fail in, and it is disclosed in the
corpus notes rather than discovered later. Two upstream fixture documents panic the parser;
the contract's `PARSER_PANIC` classification catches both, and both live in the corpus as
regression seeds. GFM's spec text autolinks `ftp://` where the pinned bundle and github.com
do not; the bundle wins, and the corpus records why.

The SHA-1 collision detector rounds out the family: the store rehashes every object with
collision detection, and the public SHAttered and Shambles vectors cannot even be framed as
git objects without breaking the property the detector checks, which the suite proves by
construction rather than by assumption.
