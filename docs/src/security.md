# Security model

The repository being scanned is treated as the attacker. Its documents, paths, Git objects,
packfiles, index, and policy file all came from whoever wrote the pull request, and the
scanner's whole job is to be a safe, pure function of those hostile bytes.

The engine executes nothing. No plugin system, no configurable commands, no formatter
calls, no `git` subprocess. A policy file that names a command or a plugin is not a feature
request to decline politely: the field is unknown, the configuration is invalid, the run
ends incomplete, and the emitted report cannot be mistaken for a complete pass. Process creation belongs to
the separate `amiss-bootstrap` executable; it is not a capability exposed by the scanner
engine.

The engine has no network acquisition interface and does not fetch missing objects. It never
writes to the repository, which the
[no-write tests](https://github.com/HardMax71/amiss/blob/main/crates/amiss/tests/no_write.rs)
check both by comparing the tree and by scanning a read-only repository. Attempts to make it
read outside the repository run into the never-follow-links rule described in
[Snapshots](snapshots.md).

Parsers are the biggest attack surface and receive fuzz targets and pinned conformance
corpora. Document byte admission is charged before parsing. Parser node and nesting totals,
however, are measured and charged only after the grammar returns; they are output budgets,
not a general CPU deadline inside the parser. The order is explicit in the
[scan pipeline](https://github.com/HardMax71/amiss/blob/main/crates/amiss-scan/src/scan.rs).
One budget does act inside the parse: every candidate close of an MDX code region charges
the accumulated region against the `aggregate-embedded-code-evaluation-bytes-per-snapshot`
ceiling before the lexical scan reads it, which bounds the one measured quadratic case.
The history of that case is in the
[corpus notes](https://github.com/HardMax71/amiss/blob/main/corpus/README.md).

A Markdown parser panic is caught and converted to `PARSER_PANIC` against the document that
caused it instead of aborting the process. The known panic fixtures live in the conformance
corpus and tests pin that classification. This protects the run from that failure mode; it
does not turn the post-parse node limits into a wall-clock guarantee.

Output is part of the surface too. Repository paths end up in terminals and CI logs, so
the human format escapes every byte outside printable ASCII. An ANSI escape sequence, a
carriage return, or a forged `::error::` workflow command embedded in a filename reaches
the log only as harmless `\uXXXX` text. A path that is raw bytes rather than text renders
each such byte as the two-digit escape of its value, never inventing a character the
bytes never encoded. The JSON report keeps fidelity its own way, the exact original
string for a UTF-8 path and a `bytes_hex` object for anything else, because the log needs
safety and the report needs fidelity, and those are different channels with different
rules. The Action separately HTML-escapes repository-controlled targets before placing
them in its Markdown summary and applies GitHub workflow-command escaping to annotation
paths and messages.

Two delivery paths need different trust descriptions. The root
[Action dispatcher](https://github.com/HardMax71/amiss/blob/main/action.yml) makes conventional source release tags usable by delegating to the same version's immutable
[`action/vX.Y.Z` runtime](https://github.com/HardMax71/amiss/blob/main/crates/amiss/action/runtime.yml). That immutable second ref is part of a source-tag or source-commit pin; users that require one complete tree can pin the generated runtime tag or commit directly. The runtime is a GitHub convenience event wrapper. It verifies
the selected engine's digest against the release manifest carried in the same action tree,
then launches the engine directly. That detects an inconsistent tree, but the manifest is
not an independently acquired trust anchor, and this lane does not use bootstrap's
supervisor; it enforces its own wall-clock watchdog, 120 seconds unless the workflow
sets the `watchdog-seconds` input, and a scan that outlives the window ends with no
result.
The manifest's build-source host is supplied explicitly and its repository identity is
forge-neutral, as pinned by the
[release validation tests](https://github.com/HardMax71/amiss/blob/main/crates/amiss-bootstrap/tests/release.rs); that prevents a
format-level `github.com` assumption but does not authenticate the supplied identity.

The separately executable
[`amiss-bootstrap`](https://github.com/HardMax71/amiss/blob/main/crates/amiss-bootstrap/src/main.rs)
implements the stronger local handoff. It bounded-captures a canonical request triplet for
commit-pair materialization; requires a complete repository, URL-dialect, candidate-ref,
target-ref, and default-ref identity; matches the embedded execution constraint and trusted-time
provider/run tuple; verifies that both commit objects were acquired before launch; and validates
the action tree and runtime closure. It then starts only the verified engine, in the supplied
repository, with a cleared environment and one private argument. A magic value, three bounded
lengths, and the exact request bytes travel in evaluation/snapshot/controls order over stdin.
Arbitrary engine arguments are not part of this path.

After the run, bootstrap acceptance rejects an unavailable hybrid and binds the engine, profile,
commits, candidate and protected target refs, and candidate identity recomputed from the report.
It requires exact organization-floor, debt-snapshot, and waiver-bundle presence, digest, and
trust source; binds the provider run and trusted instant; and checks the execution-constraint and
trusted-time digests against their recomputed semantics. Constraint trust source is bound too.
Acceptance also requires the report to retain `self-asserted` sandbox assurance, `local-process`
enforcement, and no sandbox verification. Clearing the environment,
fixing the executable and input, validating runtime closure, and enforcing the 120-second
watchdog are meaningful controls; they are not an OCI sandbox or microVM and must not be
reported as a provider-verified sandbox. The accepted engine envelope is republished unchanged,
so it does not gain an authenticated signature merely by passing through bootstrap.

Provider authentication belongs outside both executables. The separate
[`amiss-controller`](https://github.com/HardMax71/amiss/tree/main/controller) foundation models
an untouched delivery whose headers and body remain untrusted until a registered adapter
authenticates them. It stops and discards output when ownership cannot be proven, and it refreshes
provider state before accepting a result as current. Closure, revocation, and runner failures such
as missing output, timeout, tampered runtime, or an output bound to the wrong identity or tree
remain fail-closed conclusions rather than passes. [Controller delivery](controller.md) defines
the complete flow, durable record, race, and retry rules.

Its concrete provider-neutral runner begins only after acquisition. It independently re-verifies
the exact repository and action commit-tree roots against the run, derives the sealed job from that
same run, and matches the bootstrap bytes to the pinned digest before making a private copy. The
child receives a cleared environment, closed standard streams, and private request and output
paths. The controller creates and retains both output handles, so replacing their path names cannot
change what it reads. The report is bounded by the machine-report ceiling and the fixed result
record is written last. Missing or malformed output, a mismatched exit, excessive output, timeout,
signal, heartbeat loss, and runtime tampering cannot become a pass.

Pinned ProcessKit provides one cross-platform process-tree boundary. On normal exit, timeout, or
cancellation, the runner hard-kills the group and requires ProcessKit to report it empty before
reading output. The runner renews before launch and halfway through each controller-derived
relative lease window, capped at five seconds, and cancels the tree on heartbeat refusal. Its wall
limit is no greater than 120 seconds. A leader that exits cleanly cannot leave its descendants
behind. These rules close ordinary descendant and lease-loss races; they do not claim that a
synchronous host kernel operation can be interrupted if that operation itself stops returning.

The concrete file record requires a pre-created private local directory outside the repository and
action tree. A future service must own that directory and set its operating-system permissions or
access-control list: anyone who can read or change it is inside the trust boundary. Its checksums
detect damage; they do not authenticate a writer. Shared and network filesystems are unsupported.
The root metadata fixes its lease duration, maximum record count, and replay window and keeps a
high-water clock. Admission under one fixed lock prevents concurrent processes from crossing the
record cap; one maintenance lock, one clock lock, and at most 256 row-lock shards keep lock-file
growth fixed. A full root rejects a new identity without evicting work already admitted.

Cleanup runs when the root opens and is available explicitly. Under the exclusive maintenance
lock it saves the high-water clock, validates the root, then removes dead report and atomic-write
files and completed rows whose authenticated replay end has passed. It never removes
running or saved work. GitHub and Gitea-family exact-body deliveries remain permanent because their
signatures authenticate no attempt time; forgetting those rows would reopen replay for a captured
body. A local clock rollback cannot revive a bounded delivery after deletion. Operators must size
the fixed record cap for permanent replay rows rather than treating cleanup as eviction policy.

The controller library now bounds raw ingress and verifies GitHub, GitLab Standard Webhooks, and
Gitea-family HMAC signatures against rotating, redacted anchors. GitHub and Gitea replay identity
comes from the exact signed body rather than their unsigned delivery headers; GitLab binds its
signed ID and timestamp. A verifier proof is tied to the exact controller route, receipt time,
header sequence, and body, and its fields cannot be rewritten by an adapter. Ingress rejects a
GitLab proof under a replay-only route, so its signed timestamp must be checked for freshness.
There is still no concrete provider adapter,
route loader, webhook listener, authenticated payload decoder, API client, credential source,
repository or action-tree acquisition worker, deployable controller, or provider check publisher.
No authenticated integration joins those pieces to the bootstrap runner for any provider or
self-hosted instance. The engine's `forge` field still selects only a link URL dialect.
The JavaScript launcher is pinned manifest data and refuses if invoked directly; the current
composite Action does not invoke bootstrap or the controller.
