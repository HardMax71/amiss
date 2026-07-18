# Edge cases and divergences

The edge cases in the suite are where the contract's exact wording gets earned. A sample:

Paths are bytes, so `été.txt` resolves without any Unicode normalization, and two names
that differ only in case are two different files even on a filesystem that would merge
them. A directory link resolves the same way through a commit and through the staged
index, which sounds obvious until you know the index stores no directory entries at all
and containment has to be proved from sorted path prefixes. `guide.md/` with a trailing
slash fails as a type mismatch while `guide.md` resolves right next to it. `%252F` in a
link decodes once, to the literal text `%2F`, and never becomes a second path separator.

Hostile input gets the same rules with more suspicion. A document path carrying ANSI color
codes, a terminal bell, and a forged CI command prints harmlessly in the human output and
survives byte-for-byte in the JSON. A tree entry named with bytes that are not UTF-8 is a
scanned document whose path travels as hex, not a refusal and not a silent drop, while a
name the path grammar itself rejects, a backslash or a dot segment, is an
`UNREPRESENTABLE_PATH` refusal that disclosed those exact bytes. A five-thousand-byte path
is a reported limit crossing carrying both numbers. A tracked file whose object is missing
from the store refuses and names the document, instead of guessing about content it
cannot see.

The forge dialects pin the URL spellings the forges' own browsers emit and nothing
looser. GitLab's legacy pre-separator form still redirects in a browser and is foreign
here, as is `/-/raw/`; a GitLab project literally named `-` could never be told apart
from the separator, and GitLab reserves the name anyway. Gitea's untyped `src/<ref>/`
form, which some tooling still generates, is foreign because the typed `src/branch/`
spelling is what the forge emits. A gitea tag link is out of version scope even when its
segments spell the candidate branch exactly, because no tag is a trusted ref. The
line-anchor grammars do not leak either: `#L10-20` selects lines only under the gitlab
dialect, `#L10-L20` only under github and gitea, so the same fragment can resolve on one
forge and remain unsupported on another. Single-line `#L10` is common to all three.

The parser pin records its known differences instead of hiding them. Measured against the
pinned grammar bundle and against GitHub's own rendering, exactly one difference
affects link extraction: `[link[^1]](#)`, a footnote call inside a link label, which the
pinned Rust parser does not turn into a link. A reference written that way goes unseen.
That is under-reporting, the safer direction to fail, and it is written down in the corpus
notes rather than waiting to be discovered. Two upstream test documents make the parser
panic; the engine catches both as `PARSER_PANIC`, and both live in the corpus as regression
tests. The [GFM](https://github.github.com/gfm/) spec text says `ftp://` should autolink where the pinned bundle and
GitHub's renderer disagree; the bundle wins, and the corpus records why.

One more, for flavor: the object store re-hashes everything with SHA-1 collision detection,
and the suite proves that the public SHAttered and Shambles collision files cannot even be
framed as Git objects without breaking the very property the detector checks. Reachable in
code, unconstructible in practice, and tested as such.
