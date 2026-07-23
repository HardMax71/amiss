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

Provider authentication belongs outside both executables. The nested
[`controller/`](https://github.com/HardMax71/amiss/tree/main/controller) workspace keeps the raw
delivery untrusted until a configured verifier accepts it, keeps provider and storage
dependencies out of the scanner, and stops when ownership cannot be proven.
[Controller delivery](controller.md) defines the neutral record, heartbeat, race, and retry
rules.

The source-built provider services are the concrete independent lanes. Their bounded plaintext
listeners must sit behind an operator-controlled TLS terminator that also bounds connection
concurrency and header, body, idle, and slow-body time. GitHub, Gitea, and Forgejo authenticate
the exact webhook body before saving it, acknowledge only durable input, and authenticate the
saved bytes again in the worker. GitLab instead authenticates the policy job's short-lived OIDC
token and keeps the request synchronous so only the exact passing result makes that protected job
succeed. Each endpoint takes a configured in-process permit before reading the body and holds it
through durable admission or synchronous evaluation. That cap does not replace the public
connection and slow-client limits at the TLS edge.

Each adapter refreshes the repository, change, commits, trees, and protected merge rule through a
controller-owned credential. GitHub requires a strict required check bound to its App and writes
the Check Run on the test merge. GitLab requires an enforced merge train and independently owned
pipeline execution policy, binds the job's policy origin and runner, and uses the policy job's
result as evidence. Gitea and Forgejo require one approval restricted to the service's dedicated
reviewer and write the final review through that account. A missing, weakened, bypassable, or
changed rule stays fail closed.

All lanes acquire exact SHA-1 repository and action commits through Git protocol v2, with fixed
pack, object, inflated-byte, resolved-byte, delta-depth, and indexing-thread limits. They invoke
the supervised bootstrap and refresh provider state again before accepting or publishing the
result. Closure, changed head or gate, removed authorization, missing output, timeout, runtime
tampering, or a wrong identity stays fail closed. The pinned action repository must be on the
same provider instance.

The runner independently reopens the acquired repositories and checks the exact commit-tree roots.
It derives the sealed job, matches the bootstrap to the pinned execution constraint, clears the
child environment and standard streams, retains both bounded output handles, and uses ProcessKit's
cross-platform process-tree boundary. Every terminal path hard-kills and drains the group before
output is accepted. Lease loss cancels the same tree. These rules cover ordinary process and
ownership races; they do not promise that a host kernel operation can be interrupted if that
operation itself never returns.

The provider API credential, webhook key ring or OIDC keys, execution constraint, optional
controls, bootstrap, TLS terminator, scratch directory, raw inbox where used, and delivery ledger
are trust roots. Provider and repository administrators who can change the protected merge rule,
integration or policy owners, reviewer-account owners, key issuers, and configured bypass actors
are also inside the boundary. Repository bytes are not. A deployment is only as independent as
its host and those operator-controlled inputs. Self-hosted instances must expose the exact APIs
required by their lane and a certificate chain accepted by the Rust TLS clients; there is no
insecure-TLS mode.

The inbox and ledger use checksummed ordinary files, not SQL or a database. Their roots must be
pre-created private local directories outside the repository and action tree; shared and network
filesystems are unsupported. Checksums detect damage, not a malicious local writer. A webhook
inbox removes raw bytes after controller completion. The ledger retains running and saved work
and keeps GitHub and Gitea-family exact-body completion markers permanently, because those
signatures contain no trusted delivery time. A full store rejects new identities instead of
evicting accepted work.

The resulting Check Run, policy-job result, or dedicated review is provider evidence, but the
engine report remains an unchanged, self-asserted envelope. The controller neither signs it nor
upgrades its sandbox claim.
[Provider-verified controls](provider-controls.md) gives the exact setup, configuration, limits,
storage rules, freshness and retry limits, rotation, and report distinction. No provider update
is atomic with the local ledger; each provider page states its reconciliation limit. The GitHub
convenience Action still invokes the public scanner directly and does not gain this trust
boundary.
