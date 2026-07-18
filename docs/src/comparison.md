# Amiss and link checkers

Link checkers and Amiss solve different halves of one problem, and the halves compose. A
link checker asks whether destinations are alive, most valuably the external ones: it
fetches URLs, follows redirects, and reports the dead. Amiss asks whether the repository
agrees with its own prose: it resolves every in-repository reference against an exact
Git snapshot, compares two snapshots to see what moved under what, and gates the change
that broke the agreement.

[lychee](https://lychee.cli.rs/) is the strongest of the checkers and the one worth
comparing against honestly. It is fast, async, and reads Markdown, HTML, and
reStructuredText; it checks external URLs, which Amiss never fetches by design; it
checks local file links, and with `--include-fragments` it verifies heading anchors,
which Amiss deliberately declares unsupported rather than validate. If your failure mode
is dead links on a published site, lychee alone is the right tool, and nothing here
argues otherwise.

What a link checker cannot see is change. It examines one state of the world, so it can
say a target is missing but not who removed it, whether it was missing before your pull
request, or that a target still resolves while its content moved out from under the
paragraph citing it. Those questions need two snapshots and exact comparison, and they
are where Amiss lives:

| | Amiss | lychee |
| --- | --- | --- |
| Checks external URLs | never fetches | yes |
| Checks heading anchors | declared unsupported | with `--include-fragments` |
| Compares two snapshots | always | no |
| Attributes a finding to the change | introduced, pre-existing, resolved | no |
| Reports changed content under unchanged prose | yes, as advisory | no |
| Checks the staged index before commit | `--index` | no |
| Policy can loosen the gate | never, loosening is itself a finding | configuration is open |
| Byte-identical reports with digests | yes | no |

The smaller checkers sit on the same side of the line as lychee with less reach:
markdown-link-check and linkinator examine files one state at a time from JavaScript,
and mdbook-linkcheck is scoped to mdBook books. All are one-shot examiners; none
compares, attributes, or ratchets.

The honest pairing for a repository with a published site is both: lychee for the web,
where liveness is the question, and Amiss for the tree, where agreement is the question
and where a gate must not be quietly loosened. For a repository whose documentation
points mostly at itself, which is most repositories, Amiss covers the surface that
actually breaks, and adds the half no checker attempts: telling you when the code moved
and the prose did not, in a report whose every row
[explains itself](profiles.md).
