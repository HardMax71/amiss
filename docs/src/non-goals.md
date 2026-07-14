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

## Against the alternatives

The project record compares the design against every neighboring school, and says where each
one wins.

Swimm wins wherever docs are authored inside its platform: its auto-sync genuinely repairs a
renamed token without a human. Amiss never edits prose, because the best published
comment-updating system reached 16.7% exact match, and a tool that is wrong five times out of
six does not get write access. Swimm sees only documents written in its format; Amiss reads
the repository as it already is.

Fiberplane's drift is the closest mechanism, and for a solo developer with twenty hand-placed
anchors it is the right amount of tool: an authored `@path#Symbol` anchor is never a false
positive. It checks only what someone annotated, though, and unannotated pages are its silent
majority. Amiss starts from zero authoring and reports the whole surface, including the pages
nobody thought to anchor.

The AI-rewrite agents win the pitch, because "we update your docs" sounds better than "we
tell you what moved". They lose everything that makes a gate: no coverage guarantee for the
page the model never visited, nondeterministic output, and a tired reviewer as the only check
on plausible wrong text. The honest relationship is composition: a deterministic finding
queue is a good prompt feed for such an agent, and Amiss is deliberately the deterministic
half.

Executable-docs systems (doctest and its relatives) prove more than Amiss does about the
lines they execute, and nothing about any other line. Regeneration pipelines eliminate drift
on derivable content and say nothing about hand prose; user zero's stale-generator story in
[The evidence base](evidence.md) shows regeneration passing forever on wrong output.
Freshness dates are universal and free and gate on the calendar rather than on change. Each
of these is a fine layer. None of them answers the question Amiss answers, and Amiss does not
answer theirs.
