# The GitHub lane runs one repository end to end

A provider lane is only meaningful if an operator can stand the whole thing up: one repository,
one App installation, one protected branch, and a service that fails loudly when its
configuration is wrong rather than at the first webhook.

The source-built GitHub service completes that lane on GitHub.com or a compatible GHES release.
Strict JSON loads the App key, the rotating webhook secrets, the external controls, the
execution constraint, the bootstrap, and separate private state roots, refusing any field it
does not recognize instead of ignoring it. An unknown key in a trust-boundary configuration is
either a typo or an attempt, and neither should be silently dropped.

The listener speaks plaintext by design and is deployed behind an operator-owned TLS and
connection-limit boundary. That is stated rather than assumed, because a service that pretends
to terminate TLS while sitting behind a proxy that also does invites exactly one confusion.

The App identity is what makes the gate real: the required status is bound to the App's
integration id in the repository ruleset, so no other actor can post the check that satisfies
it. A live run against github.com, including one with the ruleset deliberately disabled to
watch the lane refuse, is recorded in [Retained provider runs](../provider-evidence.md).

Completed in [#107](https://github.com/hardmax71/amiss/pull/107). The service is
[`controller/github-service/`](https://github.com/hardmax71/amiss/tree/main/controller/github-service) and the setup is
[GitHub](../provider-github.md).
