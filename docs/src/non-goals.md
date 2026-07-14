# What Amiss is not

Amiss does not read your prose. A hash can prove that a file changed; it cannot prove that
a sentence became false, or that anyone reviewed it. The investigation behind this tool
spent a long time on designs that pretended otherwise, and the strongest lesson in the
record is that observation, acceptance, review, and trust are different facts, and dressing
one up as another rots all of them. So Amiss reports structure: this link resolves or does
not, these bytes changed or did not, this paragraph moved or did not. Judgment stays with
people.

It is not a link checker in the usual sense. Live-URL checkers query the network and decay
with it; Amiss never touches the network and only ever speaks about one repository's own
files at two exact commits. It is not a style linter either: it has no opinion on headings,
tone, or wording, and no rule engine to hold one.

It is not a documentation-coupling system with memory. Tools in that family, Fiberplane's
drift, Swimm, and the ledger design this project itself rejected, record what they blessed
and react when reality drifts from the record. The rejected design is described in
[Provenance](provenance.md). The shipped scanner remembers nothing, which removes the
migration, merge-conflict, and trust-on-edit failure modes wholesale, at the price of
answering a smaller question.

It does not check heading anchors, site routes, code symbols, or other repositories. Each
of those is a real check that belongs to a layer holding the right information: the site
generator knows its routes, the language server knows its symbols. Amiss lists them as
explicit unsupported boundaries in the report instead of guessing, because a guessed pass
looks exactly like a real one until it burns you.

And it accepts no configuration that would let a repository weaken its own check. No
suppression comments, no severity downgrades, no hooks. The absence is the point.
