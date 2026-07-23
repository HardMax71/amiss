# What Amiss is not

Amiss does not read your prose. A hash can prove that a file changed; it cannot prove
that a sentence became false, or that anyone reviewed it. The investigation behind this
tool spent a long time on designs that pretended otherwise. The strongest lesson in that
record: observation, acceptance, review, and trust are different facts, and dressing one
up as another rots all of them. So Amiss reports structure. This link resolves or does
not, these bytes changed or did not, this paragraph moved or did not. Judgment stays with
people.

It is not a link checker in the usual sense. Live-URL checkers query the network and decay
with it; the scanner engine never touches the network and only ever speaks about one
repository's own files in two exact snapshots: a base commit and either a candidate commit or
the staged index. The GitHub provider controller can acquire the exact repository state before
invoking that engine, but it does not make live URLs an evaluation input. Amiss is not a style
linter either: it has no opinion on headings, tone, or wording, and no rule engine to hold one.

It is not a documentation-coupling system with memory. Tools in that family, [Fiberplane's
drift](https://github.com/fiberplane/drift), [Swimm](https://swimm.io), and the ledger design this project itself rejected, record what they blessed
and react when reality drifts from the record. The rejected design is described in
[Provenance](provenance.md). The shipped scanner remembers nothing, which removes the
migration, merge-conflict, and trust-on-edit failure modes wholesale, at the price of
answering a smaller question.

It resolves the file portion of a relative link with a heading fragment, but it does not
check the heading's slug. A recognized numeric line fragment is narrower: Amiss can select
and compare those exact bytes, but cannot tell whether they still express the idea the prose
intended. It also does not validate site routes, code symbols, live URLs, or other repositories.
Those checks belong to a layer holding the right information: the site
generator knows its routes and the language server knows its symbols. Where a supported
construct reaches one of these boundaries, Amiss records the unsupported or out-of-scope
semantics instead of guessing, because a guessed pass looks exactly like a real one until it
burns you.

And it accepts no configuration that would let a repository weaken its own check. No
suppression comments, no severity downgrades, no hooks. The absence is the point.

## Against the alternatives

The predecessor investigation compared the design with several neighboring approaches and
recorded where each one wins.

Swimm wins wherever docs are authored inside its platform: its auto-sync can repair a renamed
token without a human. Amiss never edits prose because a structural observation does not
authorize a semantic rewrite. Swimm sees documents authored for its platform; Amiss reads
supported document classes already present in the repository.

Fiberplane's drift is the closest mechanism, and for a small set of hand-placed anchors it
can be the right amount of tool: an authored `@path#Symbol` precisely states what should be
checked. It checks only what someone annotated. Amiss starts from zero authoring and discovers
its closed document set automatically, reporting supported references and explicit coverage
boundaries without claiming to understand every sentence or path-like phrase.

The AI-rewrite agents win the pitch, because "we update your docs" sounds better than "we
tell you what moved". They lose everything that makes a gate: no coverage guarantee for the
page the model never visited, nondeterministic output, and a tired reviewer as the only check
on plausible wrong text. The honest relationship is composition: a deterministic finding
queue is a good prompt feed for such an agent, and Amiss is deliberately the deterministic
half.

Executable-docs systems ([doctest](https://docs.python.org/3/library/doctest.html) and its
relatives) prove more than Amiss does about the
lines they execute, and nothing about any other line. Regeneration pipelines eliminate drift
on derivable content and say nothing about hand prose; user zero's stale-generator story in
[The evidence base](evidence.md) shows regeneration passing forever on wrong output.
Freshness dates are universal and free and gate on the calendar rather than on change. Each
of these is a fine layer. None of them answers the question Amiss answers, and Amiss does not
answer theirs.
