# What Amiss is not

Amiss does not read your prose. A hash can prove that selected evidence changed; it cannot
prove that a sentence became false, or that an editor reviewed it. The investigation that
produced this tool spent considerable effort on systems that pretended otherwise, and the
strongest lesson in the dossier is that observation, acceptance, review, and trust are
different facts that rot the moment one is dressed up as another. So Amiss reports structure:
this link resolves or it does not, these bytes changed or they did not, this paragraph moved
or it did not. The judgment stays human.

It is not a link checker in the usual sense. Live-URL checkers fetch the network and rot with
it; Amiss never touches the network and only ever speaks about the repository's own tree at
two exact commits. It is also not a docs linter: it has no opinion on style, headings, or
tone, and no rule engine to express one.

It is not a documentation-coupling system with state. Tools in that family (Fiberplane's
drift, Swimm, and the ledgered design this project itself rejected) remember what they
blessed and react to drift from the memory. The rejected predecessor is described in
[Provenance](provenance.md); the shipped scanner keeps no memory at all, which removes the
migration, conflict, and trust-on-edit failure modes wholesale at the price of answering a
smaller question.

It does not check anchors, heading slugs, site routes, code symbols, or foreign
repositories. Each of those is a real check that belongs to a layer with the right
information: the site generator knows its routes, the language server knows its symbols.
Amiss names them as explicit unsupported boundaries in the report rather than guessing,
because a guessed pass is indistinguishable from a real one until it burns you.

And it does not accept configuration that would let a repository weaken its own check. No
suppression comments, no severity downgrades, no plugin hooks. The absence is the feature.
