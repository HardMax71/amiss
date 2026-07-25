# The GitHub lane runs one repository end to end

A provider lane is only meaningful if an operator can stand up the whole thing: one
repository, one App installation, one protected branch, and a service that survives its own
configuration being wrong.

The source-built GitHub service completes that lane on GitHub.com or a compatible GHES
release. Strict JSON loads the App key, the rotating webhook secrets, the external controls,
the execution constraint, the bootstrap, and separate private state roots, refusing anything
it does not recognise. The listener speaks plaintext by design and is deployed behind an
operator-owned TLS and connection-limit boundary, which is stated rather than assumed.

The service is [`controller/github-service/`](https://github.com/HardMax71/amiss/tree/main/controller/github-service) and the setup is
[GitHub](../provider-github.md). Completed in [#107](https://github.com/HardMax71/amiss/pull/107). A live run against github.com is
recorded in [Retained provider runs](../provider-evidence.md).
