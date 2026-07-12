# Head-to-head: this design against the alternatives

Date: 2026-07-10. prior-art.md catalogs the landscape mechanism by mechanism; this file compares
the finished design (day-zero inference loop, block identity, refresh lane, attributed failure,
three-verb policy, growth model) against each alternative and says plainly where ours loses.

Status correction (2026-07-11): the design is not finished, and the content-derived governed
identity, automatic refresh, trust-on-edit, and attributed-safety assumptions in this comparison
were rejected by [pre-implementation-review.md](./pre-implementation-review.md). The competitive
map also changed; use [market-reassessment.md](./market-reassessment.md) for current build-vs-extend
conclusions. This file remains a record of the second-pass comparison.

| Alternative | Beats us at | We beat it at |
| --- | --- | --- |
| Swimm | auto-fixing trivial drift, IDE polish, enterprise support | zero authoring, unmodified docs, no platform, no full clone, free prose coverage |
| fiberplane/drift | shipped today, structural binding precision, simplicity | coverage without authoring, typed kinds, attribution, policy, migration |
| AI-rewrite agents | fixing docs instead of pointing, semantic prose updates | determinism, coverage guarantees, cost, privacy, gate-worthiness |
| Execute school | proof strength on executable content | everything not executable, hermetic blocking path |
| Regenerate school | eliminating drift on derivable content | hand prose, generator-input staleness, non-API docs |
| RM suspect links | certification evidence, compliance features | zero authoring, prose scale, modern CI, fatigue design |
| DIY scripts | exists by tonight, exactly fitted, no new trust | month-two survival: migration, attribution, noise law, fatigue |
| Freshness dates | universal, zero tooling, calendar legibility | gating on change not calendar, no nag decay, diffs not dates |
| LLM detectors | semantic contradictions under unchanged names | precision that can gate, cost, determinism |

## Swimm

Swimm is the only funded company that ever aimed at this exact problem, and the comparison is
lopsided in both directions. It wins wherever the docs are written inside its world: auto-sync
genuinely repairs trivial drift (a renamed token propagates into the doc without a human), the IDE
surface puts coupled docs next to code, and an enterprise buyer gets support and onboarding. We
never auto-edit prose, by explicit design (the best published comment updater hits 16.7% exact
match), so the snippet-rename case that Swimm fixes in one click costs us a human `ok` or an edit.
We win everywhere else: Swimm verifies only elements coupled inside documents authored in its
format, so the corpus that already exists in a repository (and every agent-instruction file) is
invisible to it; adoption means writing new docs, not checking old ones. Ours reads the repo as
it is, needs no platform, no full clone, no per-seat fee for the deterministic core. The deeper
difference is category: Swimm is a documentation platform with a checker inside; ours is a checker
that leaves the docs alone.

## fiberplane/drift

The closest mechanism, and the honest comparison is that drift is our channel five shipped early.
It wins on existence (v0.10.1, today), on simplicity (one binary, explicit `@path#Symbol` anchors,
one lockfile, nothing to configure), and on binding precision: an authored anchor is never a false
positive, while our inference carries the whole position-sensitivity and noise apparatus (OP-06,
OP-23). For a solo developer with twenty anchors, drift is the right amount of tool and ours is
overkill. We win on everything that appears at scale: coverage without authoring (drift checks
only what someone annotated, and unannotated docs are its silent majority), typed claim kinds,
suspect-versus-broken, attribution, policy with a floor, identity migration, and an acceptance
design that learned from suspect-link fatigue instead of shipping bare `drift link`.

## The AI-rewrite school (Mintlify agent, DeepDocs, DocuWriter, GitBook agent)

They win the pitch to a manager: "we update your docs" beats "we tell you your docs are stale",
because responding to findings is labor and their product spends model tokens instead. They also
reach semantic prose updates that no deterministic tool can write. They lose everything that makes
a gate: no coverage guarantee (nothing flags the page the model never visited), nondeterminism,
metering, code leaving the repo, and a single tired reviewer as the only check on plausible wrong
text. The honest position is composition, not rivalry: a suspect-claim queue is the best possible
prompt feed for a rewrite agent, and an agent PR that clears findings is the best possible
response to one. Checker-less agents are unsafe; agent-less checkers are laborious; the endgame is
both, and the checker is the part that has to be trustworthy.

## The execute and regenerate schools

Not really rivals, and pretending otherwise would be spin. An executed procedure proves behavior;
our staleness proves only change-since-look, which is strictly weaker evidence wherever execution
is possible. Generated reference eliminates a drift class rather than detecting it, which is
strictly better wherever content is derivable. A team whose docs are ninety percent runbooks gets
more from Runme than from us; a team whose docs are ninety percent API reference gets more from
generation. We wrap both as evidence lanes, we cover the prose around them that neither touches,
and we catch the one failure both share: a generator or harness whose own input went stale (user
zero's railroad diagrams regenerated faithfully from a dead copy for months).

## Requirements-management suspect links (Doorstop, OFT, StrictDoc, Jama, Polarion, Codebeamer)

They win their market permanently: certification evidence, requirement typing, coverage matrices,
and tool qualification are legal requirements in DO-178C and ISO 26262 work, and nothing in our
design produces auditor-grade traceability artifacts. We took their core mechanic and shed their
authoring model, which is exactly why we win outside compliance: nobody hand-writes item files or
bumps revision integers for prose docs. The lesson base flows one way (their two decades of
fatigue failures shaped our acceptance design); the customers barely overlap.

## The do-it-yourself script, and apathy

The real competitor at adoption time is not a product. It is either a script someone writes in an
afternoon (lychee plus grep plus a hash loop plus a Danger rule, or Dosu's published recipe) or
nothing at all, which is what most teams run today. The script wins on day one: it exists, it is
exactly fitted to one repo's conventions, and it demands no trust in a third party. It loses in
month two, and the problems register is literally the list of how: line-based anchors rot,
whole-file hashes make formatting sweeps scream, no attribution means pre-existing debt fails
every PR until someone turns it off, and there is no answer for renames, fan-out, or rubber-stamp
fatigue. Our counter is not features but time-to-value: install in minutes, and be visibly better
than the script on the first run (rename candidates, attribution, grouped fan-out). Apathy wins
until the first agent-caused incident traced to a stale instruction file; that is why the
agent-file wedge leads go-to-market.

## Freshness dates and freshness scores

Date regimes (g3doc, ms.date, Kubernetes sweeps) win on universality: they work on any format with
zero tooling, and calendars are organizationally legible in a way content hashes are not. They
lose on the record: nothing fails, nags decay, and a date certifies attention, not accuracy. Dosu's
0-to-100 score gives managers a trendline our booleans do not; we chose the boolean with diffs
because scores invite threshold politics and cannot be explained in a sentence, and we concede the
dashboard need is real (a derived metric view over claim states, not a gating score). We kept the
one thing dates are right about: external claims expire on time, because nothing else can expire
them.

## LLM inconsistency detectors

They see what we structurally cannot: a sentence whose meaning inverted while every identifier
survived (our EC-F2 blind spot is only partly closed by value checks). They cannot gate: 0.62
precision at the best published filtering means four in ten flags waste a human, and that is
before cost, nondeterminism, and prompt injection. This is why the design quarantines them to an
advisory lane calibrated on CoDocBench before it may speak. Better than us at semantics, unusable
as a gate, and the combination is ours to ship.

## Where this design is worse than everything

Honesty section. It does not exist yet; every alternative above ships today. It adds artifacts
that need explaining (a lockfile, a refresh bot, a policy TOML), where dates and scripts need
none. Its central signal is epistemically weak by construction: stale means changed-since-look,
not wrong, so every finding asks a human question rather than stating a fact, and the value of the
tool rides entirely on how cheap answering that question becomes. Inference precision will never
reach authored-anchor precision, so a noise floor exists that fiberplane/drift structurally does
not have. And the growth model is a standing temptation to rebuild the complexity that four rounds
of critique removed; the discipline to keep day zero the product is a bet on ourselves, not a
property of the design.

## Why the position still holds

Every alternative is strong inside a boundary: Swimm inside its platform, drift inside its
anchors, agents inside a reviewer's patience, execution inside the executable, generation inside
the derivable, RM tools inside compliance, scripts inside one repo, dates inside a calendar. The
unclaimed ground is the docs that already exist, unmodified, in every repository, read
increasingly by machines that cannot smell staleness. Ours is the only design in the field built
for exactly that ground: zero authoring on day zero, honest states, attributed failure, and a
growth path that adds strength only where a team feels the specific pain. It is the least
impressive tool in this file on any single dimension and the only one whose day-one denominator is
every repository on the host.
