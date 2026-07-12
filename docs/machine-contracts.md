# Scanner v0 machine contracts

Date: 2026-07-12.

Status: normative wire contract for scanner-v0 inputs and output. It closes the machine-schema,
digest, duplicate-occurrence, debt-ordering, and trusted-time holes found after the first contract
freeze. It does not authorize a required CI deployment or any governed/provider state.

Normative schemas and shape-valid examples are under [spec/](./spec/). `MUST`, `MUST NOT`,
`SHOULD`, and `MAY` are normative.

## Published schemas

| Contract | Exact location | Role |
| --- | --- | --- |
| Repository policy | [scanner-policy-v1.schema.json](./spec/scanner-policy-v1.schema.json) | Optional raise-only candidate-owned input at exactly `.assure/scanner-policy.json` |
| Organization floor | [organization-floor-v1.schema.json](./spec/organization-floor-v1.schema.json) | Optional externally protected minimum policy and inventory |
| Adoption debt | [debt-snapshot-v1.schema.json](./spec/debt-snapshot-v1.schema.json) | Optional externally protected exact legacy-fact inventory |
| Waivers | [waiver-bundle-v1.schema.json](./spec/waiver-bundle-v1.schema.json) | Optional externally protected exact finding/fact/candidate exceptions |
| Scanner result | [scanner-report-v1.schema.json](./spec/scanner-report-v1.schema.json) | One deterministic envelope containing the complete evaluated payload or a bounded incomplete result |
| Logical index digest preimage | [scanner report `IndexProjectionInput` fragment](./spec/scanner-report-v1.schema.json#/$defs/IndexProjectionInput) | Validation root `urn:assure:schema:scanner-report:v1#/$defs/IndexProjectionInput` |
| Synthetic index snapshot preimage | [scanner report `SyntheticSnapshotInput` fragment](./spec/scanner-report-v1.schema.json#/$defs/SyntheticSnapshotInput) | Validation root `urn:assure:schema:scanner-report:v1#/$defs/SyntheticSnapshotInput` |
| Candidate identity digest preimage | [scanner report `CandidateIdentityInput` fragment](./spec/scanner-report-v1.schema.json#/$defs/CandidateIdentityInput) | Validation root `urn:assure:schema:scanner-report:v1#/$defs/CandidateIdentityInput` |

Every schema uses JSON Schema Draft 2020-12; every complete/domain object shape is closed with
`additionalProperties: false`, while conditional `properties` overlays only refine a separately
referenced already-closed shape. Validity-relevant enums are closed, and the schemas carry
additional semantic rules below. JSON Schema validation
alone is insufficient because it cannot generally prove duplicate-key rejection, canonical byte
order, digest equality, cross-field totals, Git object existence, or external trust.
The three advertised fragment URIs are first-class validation roots even though their definitions
are not reachable from every envelope instance. A producer validates the referenced fragment with
the report schema as its resource base; it must not wrap a preimage in a fake report. Shape-valid
examples for all three are published under `spec/examples/` and exercised by the smoke checker.

`experiments/validate-machine-contracts.mjs` is deliberately a local smoke checker for root and
advertised-fragment schemas, shape-valid examples, canonical report bytes, selected digests, and
finite seed/ref/ignore/LFS vectors. `experiments/validate-gitignore-vectors.mjs` is a development differential check
against the installed Git, not the production matcher. Neither is a duplicate-key-preserving parser
or product conformance evidence. Gate A's paper-design entry is sufficient to start only disposable
CLI/schema/Git-acquisition scaffolding and a conformance harness. Parser integration and evaluator
implementation remain blocked until the complete checked-in parser-profile corpus (including exact
node/depth accounting goldens) exists. Stable evidence remains blocked until a strict parser, a
conforming Draft 2020-12 validator, every cross-field rule below, the exact pinned-oracle
differential suite, and negative raw-byte fixtures are implemented independently of those smoke
checks.

## Strict JSON and canonical bytes

The four control inputs are UTF-8 JSON with no BOM. Insignificant whitespace and object-member
source order are accepted; semantic identity is computed from parsed JCS, so formatting-only edits
do not change a policy digest. The parser rejects:

- duplicate object keys before constructing a map;
- invalid UTF-8 or lone surrogate escapes;
- an unknown field, schema major, or validity-relevant enum;
- `-0`, a fraction, exponent notation, or an integer outside the safe-integer range;
- a set-shaped array that is unsorted or has a duplicate canonical key;
- a `RepoPath` that violates the core path grammar;
- trailing non-whitespace bytes after the one JSON value.

The parsed value is serialized with the restricted RFC 8785/JCS contract in
[normative-core-spec.md](./normative-core-spec.md#81-digest-primitives). A supplied file whose bytes
are semantically valid may therefore have a digest independent of indentation. The scanner report
is generated as exactly `JCS(envelope) || LF`; a noncanonical report is rejected by the CI wrapper.
Repository formatters SHOULD emit two-space indented control files for review, as the examples do.
The scanner-report example is also a readable parsed-value example, not a valid emitted byte
fixture. The separate
[`scanner-report-v1.canonical.json`](./spec/examples/scanner-report-v1.canonical.json) is the exact
one-line `JCS(envelope) || LF` golden, and the smoke checker proves it is the canonicalization of the
indented value. E0 must additionally prove that feeding the indented example bytes to the CI wrapper
is rejected as noncanonical output.

Set-shaped arrays use the `x-assure-order` key declared in their schema. Keys are compared by the
UTF-8 bytes of their canonical string value; composite keys compare their listed components in
order. The implementation MUST validate order rather than silently sort an input. Report arrays
are produced in the order defined below. The three unavailable `reasons` arrays use the explicit
`enum-declaration-order` annotation instead of a member field.

`RepoPath` is the scanner's deliberately narrower representable Git-path domain: the raw path is
1–4,096 bytes of valid UTF-8; is relative and slash-separated; has no empty, `.` or `..` component;
and contains neither NUL nor a literal backslash. Bytes and case are preserved with no Unicode
normalization. Git objects on some hosts can contain names outside this domain. Acquisition checks
the raw byte length before decoding: more than 4,096 bytes emits Git-phase
`RESOURCE_LIMIT_EXCEEDED` for `raw-path-bytes` with null path/byte-hex; a within-limit raw path that
fails UTF-8 or any other `RepoPath` rule emits `UNREPRESENTABLE_PATH` with null path and the complete
lowercase raw byte hex. Thus every Git tree/index name has either a representable value or one
closed failure, including a valid-UTF-8 POSIX name containing `\\`. The JSON Schema character limit
is only a prefilter; this byte law is authoritative.

## Digest registry

The primitive definitions are `HJ(domain, value)` and `HB(domain, bytes)` from the core. Version 1
uses these exact additional domains:

| Value | Primitive and domain |
| --- | --- |
| Repository scanner policy | `HJ("assure/scanner-policy/v1", parsed policy)` |
| Organization floor | `HJ("assure/organization-floor/v1", parsed floor)` |
| Debt snapshot | `HJ("assure/debt-snapshot/v1", parsed snapshot)` |
| Waiver bundle | `HJ("assure/waiver-bundle/v1", parsed bundle)` |
| Scanner engine binary | `HB("assure/scanner-engine/v1", exact executable bytes)` |
| Trusted action bootstrap binary | `HB("assure/scanner-action-bootstrap/v1", exact bootstrap bytes)` |
| Adapter contract descriptor | `HJ("assure/scanner-adapter-contract/v1", strict descriptor)` |
| One dependency-lock member | `HB("assure/raw-evidence/v1", exact lockfile bytes)` |
| Dependency-lock set | `HJ("assure/scanner-dependency-lock/v1", DependencyLockInput)` |
| Release manifest | `HJ("assure/scanner-release-manifest/v1", ReleaseManifest)` |
| Execution constraint | `HJ("assure/scanner-execution-constraint/v1", ExecutionConstraintDescriptor)` |
| Sandbox profile | `HJ("assure/scanner-sandbox-profile/v1", SandboxDescriptor)` |
| Sandbox verification | `HJ("assure/scanner-sandbox-verification/v1", SandboxVerification)` |
| Candidate evaluation identity | `HJ("assure/scanner-candidate-identity/v1", CandidateIdentityInput)` |
| Trusted-time statement | `HJ("assure/scanner-trusted-time-statement/v1", TrustedTimeStatement)` |
| Logical Git index | `HJ("assure/scanner-index-projection/v1", IndexProjectionInput)` |
| Synthetic staged-index snapshot | `HJ("assure/scanner-snapshot/v1", SyntheticSnapshotInput)` |
| Raw selected blob | `HB("assure/raw-evidence/v1", exact Git blob bytes)` |
| Protected repository control evidence | `HJ("assure/scanner-protected-control-evidence/v1", {path, git_mode, raw_digest})` |
| Raw link destination | `HB("assure/scanner-raw-destination/v1", exact destination bytes)` |
| Raw link query | `HB("assure/scanner-link-query/v1", exact query bytes)` |
| Raw link fragment | `HB("assure/scanner-link-fragment/v1", exact fragment bytes)` |
| Unavailable snapshot request | `HB("assure/scanner-snapshot-request/v1", exact bounded request bytes)` |
| Unavailable evaluation request | `HB("assure/scanner-evaluation-request/v1", exact bounded request bytes)` |
| Unavailable controls request | `HB("assure/scanner-controls-request/v1", exact bounded request bytes)` |
| Source block projection | `HB("assure/scanner-source-projection/v1", newline-normalized source bytes)` |
| Reserved governed-definition source | `HB("assure/scanner-governed-definition-source/v1", exact complete definition-node source bytes)` |
| Target projection | `HJ("assure/scanner-target-projection/v1", {git_mode, raw_digest})` |
| Observation ID | `HJ("assure/observation-id/v1", ObservationIdInput)` |
| Finding key | `HJ("assure/scanner-finding-key/v1", FindingKeyInput)` |
| Finding fact | `HJ("assure/scanner-fact/v1", FindingFactInput)` |
| Control state | `HJ("assure/scanner-control-state/v1", ControlStateInput)` |
| Scanner result payload | `HJ("assure/scanner-report-payload/v1", payload)` |

An envelope has exactly `schema`, `payload`, and `payload_digest`. `payload_digest` hashes only the
payload; it is not inside the hashed value. This removes the former self-reference. The wrapper
recomputes it and rejects any mismatch.

The out-of-band expected floor/debt/waiver digest uses the semantic HJ digest above, not a raw-file
checksum. A wrapper may additionally pin a raw delivery checksum, but that is acquisition metadata
and not a policy identity.

The three request-digest domains reserve diagnostic framing for a future wrapper-to-engine API;
they do not define that API. In scanner v0's in-process public CLI, every unavailable evaluation,
snapshot, or controls `request_digest` is exactly null because no request byte stream exists. A
future stable wrapper must first publish root schemas/framing goldens; only then may it hash a
complete bounded stream from byte zero through EOF. Such a future digest is non-null exactly when
EOF was obtained within the 16 MiB cap and otherwise null; prefix digests, shell quoting,
environment variables, and display strings are forbidden. Request digests are diagnostic
identities, never accepted configuration. An unavailable evaluation or controls value is legal only
in an incomplete exit-2 envelope with a matching typed error.

Unavailable snapshot/evaluation/controls values retain **all** applicable `reasons`, not the first
one encountered. Each array is duplicate-free and sorted by its schema enum declaration order.
Validation order therefore cannot choose the payload; matching AnalysisErrors are independently
sorted by their total key. A request-unreadable/not-parsed reason may coexist with only defects
established safely before the unreadable boundary—producers never speculate about unseen bytes.

The reason/error bridge is closed. The following rows name the required anchor error. All anchor
errors have null resource/limits and null byte-hex except where a row says otherwise. An identical
anchor tuple needed by both base and candidate is emitted once and anchors both; errors are a set,
not a role-tagged log.

| Value | Reason | Required anchor phase/code/path |
| --- | --- | --- |
| Evaluation | `invalid-invocation` | `invocation` / `INVALID_INVOCATION` / null |
| Evaluation | `unsupported-provider` | `invocation` / `UNSUPPORTED_PROVIDER_HOST` / null |
| Evaluation | `invalid-event` | `invocation` / `INVALID_EVENT` / null |
| Evaluation | `invalid-profile` | `invocation` / `INVALID_PROFILE` / null |
| Evaluation | `request-unreadable` | `invocation` / `REQUEST_UNREADABLE` / null |
| Snapshot | `not-supplied` | `invocation` / `INVALID_INVOCATION` / null |
| Snapshot | `repository-unavailable` | `git` / `GIT_REPOSITORY_UNAVAILABLE` / null |
| Snapshot | `missing-object` | `git` / `GIT_OBJECT_MISSING` / null |
| Snapshot | `wrong-object-kind` | `git` / `GIT_OBJECT_WRONG_KIND` / null |
| Snapshot | `unreadable-object` | `git` / `GIT_OBJECT_UNREADABLE` / null |
| Snapshot | `index-invalid` | `git` / `GIT_INDEX_INVALID` / null |
| Snapshot | `index-unmerged` | `git` / `GIT_INDEX_UNMERGED` / null |
| Snapshot | `intent-to-add` | `git` / `GIT_INTENT_TO_ADD` / affected path |
| Snapshot | `snapshot-changed` | `git` / `GIT_SNAPSHOT_CHANGED` / null |
| Snapshot | `unrepresentable-path` | `git` / `UNREPRESENTABLE_PATH` / null, with full byte hex |
| Controls | `invalid-profile` | `invocation` / `INVALID_PROFILE` / null |
| Controls | `invalid-repository-policy` | `configuration` / `CONFIGURATION_INVALID` / `.assure/scanner-policy.json` |
| Controls | `invalid-external-control` | `configuration` / `CONFIGURATION_INVALID` / null |
| Controls | `control-binding-mismatch` | `configuration` / `CONTROL_BINDING_MISMATCH` / null |

Snapshot `resource-limit` instead requires at least one `git` / `RESOURCE_LIMIT_EXCEEDED` error
whose resource is in the closed Git-acquisition partition and whose numeric fields obey the limit
law. Snapshot `not-evaluated` and controls `not-parsed` are derivative and add no anchor: they are
legal only when an already-emitted evaluation, snapshot, configuration, sandbox, or internal error
made that stage unreachable, and no more specific safely established reason exists. Conversely,
every anchor failure that prevents construction adds its mapped reason to each affected unavailable
value.

Cardinality is deterministic. Context-free anchors occur once after full-tuple deduplication.
`GIT_INTENT_TO_ADD` occurs once per affected representable path, and
`UNREPRESENTABLE_PATH` occurs once per distinct offending raw path. A resource error occurs once per
distinct `(resource, configured_limit, observed_lower_bound, path)` tuple established before the
failed acquisition stops. The unavailable value carries its reason once regardless of how many
path/resource errors anchor it. Safely established lexical/parser defect errors MUST accompany the
aggregate configuration anchor under the mandatory code law below, but they do not replace it.

No stable provider-wrapper request API is published in v0: root request schemas, handle ordering,
and cross-language wire goldens do not yet exist. The disposable local implementation invokes the
evaluator in-process from the exact public CLI and may use these byte-stream rules only internally.
A `stable-v1` required wrapper is blocked until a separate request-wire RFC publishes all three
root schemas/framing laws and adversarial fixtures. The action/execution/sandbox contracts below
are required target properties, not permission to invent that missing interop surface.

## Adapter, observation, and build provenance

A complete report carries exactly three adapter descriptors, ordered
`markdown-v1`, `mdx-v1`, `plain-advisory-v1`, with no repeated `adapter_id`. The wrapper checks that
each outer ID equals its descriptor ID and that `contract_digest` recomputes from the complete
descriptor. The compatibility matrix is closed:

| Adapter | Grammar | Frontmatter | Source projection | Structural address |
| --- | --- | --- | --- | --- |
| `markdown-v1` | `commonmark-gfm-v1` | `frontmatter-v1` | `source-projection-v1` | `markdown-ast-node-path` |
| `mdx-v1` | `mdx-source-v1` | `frontmatter-v1` | `source-projection-v1` | `mdx-ast-node-path` |
| `plain-advisory-v1` | `plain-zero-lexer-v1` | `none` | `none` | `none` |

Parser name and version are validity inputs, not labels. Any other combination, missing built-in
adapter, repeated ID, or digest mismatch is a malformed report. Changing one produces a different
descriptor digest and engine release.

V0 has no runtime adapter installation or promotion path. Renderer routes, MDX literal attributes,
and fence metadata are not extracted observations and produce no adapter-candidate finding.
Supporting any of them requires a new engine/report contract.

Every occurrence embeds its strict `ObservationIdInput`: adapter ID and contract digest, document
`RepoPath`, exact source-construct enum, structural address, source-projection digest, and complete
canonical extracted target intent. `StructuralAddress.node_path` is the zero-based child-index path
from the post-frontmatter adapter root to the exact extracted link/image/autolink syntax node, not
to its selected block owner. Child indices are the oracle `children` array positions. One supported
syntax node emits exactly one occurrence, so v0 requires `construct_index = 0` and
`duplicate_index = 0`; those fields are reserved for a future address version and never disambiguate
v0 output. The complete parser corpus publishes each address. These values may churn and never
become acceptance or exception identity. The occurrence's repeated adapter, document, construct,
projection, and intent fields MUST equal the embedded input, and `observation_id` MUST recompute.

For `github-action` provenance, the report embeds the strict release manifest. Action object format
controls the action commit/tree OID length. The wrapper resolves `action_commit_oid` in
`action_repository`, requires a commit of that format, and requires `action_tree_oid` to be that
commit's tree. It then resolves both `manifest_path` and the selected artifact's `tree_path` in that
exact tree. Each MUST be a regular blob, never a symlink, gitlink, submodule path, generated
worktree file, or same-named file from another revision. `manifest_path` has Git mode `100644`; the
selected artifact has mode `100755` on every platform (Windows ignores the execute bit but the tree
identity does not).

The manifest blob is parsed with the strict JSON rules above. Its parsed value MUST equal the
embedded `release_manifest`, and `release_manifest_digest` MUST recompute over that complete value.
The manifest lists between one and six artifacts, sorted by platform, with no repeated platform;
it need not publish all six supported platforms. Exactly one artifact MUST match both
`selected_platform` and `selected_artifact_name`; its
`tree_path` names the exact executable blob. The wrapper hashes those blob bytes twice: plain
SHA-256 MUST equal the artifact's `binary_sha256`, and the domain-separated engine digest MUST
equal both its `engine_digest` and `engine.engine_digest`. A copied binary with the right checksum
at an unlisted path, a manifest copied from another tree, or an embedded value that was not parsed
from `manifest_path` is malformed provenance.

The release manifest's build-source repository, object format, and commit identify the namespace
of the build. `DependencyLockInput` names every build lockfile by canonical repository path and its
raw-evidence digest, sorted by path with no duplicate path. Its set digest is `HJ` over that complete
object; the manifest and repeated action-provenance digest MUST agree. The protected release
pipeline resolves every listed path as a regular blob in the exact build-source commit, recomputes
each member and set digest, and binds the resulting manifest to its signed build provenance. No
implementation-selected concatenation, unnamed lockfile, directory archive, newline conversion, or
Unicode normalization is permitted. The consuming wrapper checks the embedded preimage, digest,
and pinned release constraint but does not pretend that this alone proves a reproducible rebuild.

The pinned action tree also contains the action-side runtime closure. Its root `action.yml` has mode
`100644` and is **JCS JSON plus LF**, using JSON's YAML-compatible subset rather than a general YAML
parser. It has exactly `name`, `description`, and `runs`; `runs` has exactly
`{"main": <RepoPath>, "using": "node20"}`. Duplicate keys, YAML anchors/aliases/tags/merge keys,
`pre`, `post`, composite steps, containers, and any other field are impossible. `runs.main` resolves
as a regular mode-`100644` blob and is exactly one selected `runtime_files` row with role
`launcher`. `manifest_path` and every runtime row likewise resolve as regular non-symlink blobs in
that tree. Runtime paths are never discovered by importing the launcher.

Runtime paths are unique and sorted. Plain SHA-256 of each blob equals `file_sha256`; its mode
equals `git_mode`; exactly one row has role `executable`, path equal to `tree_path`, mode `100755`,
and checksum equal to `binary_sha256`. The executable's domain-separated digest additionally equals
`engine_digest`. `runtime_contract` is exactly `manifest-closed-v1` and
`environment_contract` is exactly `scanner-process-env-v1`.

The manifest's `engine_version` is the reviewed release label. For GitHub-action provenance it
MUST equal `Engine.engine_version`; the artifact and report engine digests still identify the bytes.
For a local experimental run, where no release manifest exists, `engine_version` is explicitly
self-reported display metadata and confers no compatibility or trust claim.

The action metadata/launcher may only select the closed platform row, verify the manifest and all
runtime rows, and invoke the selected artifact by its resolved action-tree path. It has no
`pre`/`post` hook, mutable container image, download, package install, PATH fallback,
repository-local helper, plugin discovery, or unlisted sidecar/data search. After the sandbox
boundary only the selected engine and manifest-listed runtime files are available. It may read
explicit snapshot/control inputs and private bounded temporary storage and write stdout/stderr; it
MUST NOT load other action-tree/workspace code, configuration, dynamic-loader overrides, or
undeclared runtime data. `selected_platform` binds only the executable target OS and instruction-set
architecture named by its six-value enum; it does not bind kernel version, minimum OS, libc/system
ABI, physical host architecture, or the absence of emulation. Those remain X-05 compatibility
evidence, not cryptographic provenance. Any non-system library shipped by the action must be a
runtime row; a system-ABI launch failure yields missing accepted output and a failed wrapper.
Cross-host goldens remain required. This provenance covers the scanner action only. Checkout and
every other third-party action retain their own immutable pin and review requirement.

### External execution constraint

`ExecutionConstraintDescriptor` is an externally protected allow-list entry for one scanner action
tree, one release-manifest path/digest, one trusted bootstrap binary/contract, and one required
provider status name. `required_status_name` is the expected emitted context, not proof of the
workflow that emitted it and not merge authorization. Its
`descriptor_digest` MUST recompute with the execution-constraint domain above. `verified` means the
trusted wrapper acquired the descriptor from the named noncandidate source and verified that the
source applies to the current repository/ref; a candidate-authored copy or matching digest in
repository content cannot upgrade trust.

The report shape reserves `stable-v1`, but no such required deployment is currently authorized.
For a future `stable-v1` required run, execution constraint status MUST be `verified`, action provenance
MUST be `github-action`, and descriptor repository, object format, commit, tree, manifest path, and
manifest digest and selected platform MUST equal the corresponding action-provenance fields exactly. The provider check
context emitted for the current candidate MUST equal `required_status_name`, and the wrapper MUST
verify through the provider that the current ref actually requires the external workflow/ruleset
which owns that context. A name collision from a candidate workflow is not verification. A local or
disposable experimental run may use constraint status `none`, but it cannot be registered or
described as the required stable check. `SandboxVerification.execution_constraint_digest` binds the
enforced sandbox to this exact verified descriptor.

That v1 descriptor does not carry exact workflow-source identity and is therefore deliberately
insufficient to authorize a stable check. Before `stable-v1` can be used, the separate provider
request-wire and control-epoch/provider-freshness RFCs MUST bind an active organization/enterprise
ruleset workflow's source repository ID, workflow path, ref, non-null full workflow commit SHA,
resolved ordinary workflow blob OID/raw digest, and immutable reusable-workflow dependency closure
(or a provider-equivalent exact content identity), its applicability to the current
repository/ref/event, and merge-time freshness.
For a SHA-1 repository that epoch also MUST authenticate and bind a canonical independent SHA-256
digest over every loose-equivalent object preimage used for snapshot construction/evaluation:
commits, every traversed tree, and every selected document/control/target blob. Binding only the top
commit/tree is insufficient for a colliding child blob. Without the complete closure binding,
`stable-v1` accepts only SHA-256 object format. The pinned collision detector is defense in depth,
not a substitute for this authorization identity.
The RFC must add those authenticated fields to a versioned root request/descriptor or an equally
auditable provider receipt. Selecting an expected app or matching only the status context is not an
equivalent proof.

The required workflow MUST NOT invoke the target with `uses: owner/action@sha`, because that lets
the Actions loader parse/execute target metadata before provenance validation. The externally
protected workflow obtains the pinned action tree strictly as data, then runs the separately
protected `assure-action-bootstrap-v1` binary whose bytes recompute to `bootstrap_digest`. That
bootstrap parses the restricted metadata/manifest, verifies every path, mode, runtime checksum,
engine version/digest, and constraint binding. It derives the target platform from the selected
executable artifact row and executable header, without repository input, and requires that value to
equal both descriptor `selected_platform` and sandbox verification `platform`. This is the process
target, not an assertion about physical architecture under Rosetta, WoW, or another emulator. The
provider-owned runner must be able to execute that target; otherwise the required run fails without
an accepted envelope. The bootstrap then establishes the sandbox and directly execs the selected
native artifact with the empty environment. It never executes or imports `runs.main`; that launcher
exists only for non-required experimental convenience. Provider verification attests the current
bootstrap/constraint/run tuple. Changing the bootstrap requires a new externally reviewed
constraint digest.

### Sandbox and process environment

`scanner-v0-zero-capability-v1` is one closed descriptor, not a menu of aspirational flags. The
descriptor digest is `HJ` over the complete `SandboxDescriptor`. `network`, `child_processes`,
`repository_processes`, and `shared_cache` are denied; the repository, object database, and supplied
control bundle are read-only; secrets and credentials are absent; and temporary storage is a fresh
private directory capped at exactly 67,108,864 bytes and destroyed after the run. The outer
sandbox also binds a 1,073,741,824-byte physical-memory cap and 120,000-millisecond watchdog; those
operational caps kill/reject rather than create speed-dependent semantic facts. Denial covers
IPv4/IPv6, Unix/domain sockets, DNS, inherited listening sockets, process creation, shelling out,
Git/LFS/filter/helper execution, writable cache mounts, and writes through alternate repository or
object-store paths. The trusted acquisition wrapper may materialize objects before this boundary,
but none of its network handles, tokens, credential files, agents, or writable mounts cross it.

`scanner-process-env-v1` means the evaluator starts with an empty process-environment mapping. The
disposable CLI supplies repository/output/private-temporary handles and profile through an
in-process implementation boundary with no public framing promise. A future provider wrapper must
pass those values through the separately specified request-wire/invocation contract. Neither form
uses `PATH`, `HOME`, `TMP*`, locale, timezone,
`GIT_*`, `CI`/provider variables, proxy variables, token variables, or dynamic-loader variables.
UTF-8 handling, UTC parsing, and locale-independent ordering are engine rules, not host-environment
choices. The launcher itself is outside the evaluator boundary and MUST clear inherited variables
before exec; the selected engine MUST NOT consult ambient environment or discover configuration.
An implementation that needs another variable or runtime file requires a new sandbox profile and
engine contract rather than silently widening this literal.

`provider-verified` sandbox assurance requires a non-null strict `SandboxVerification` created by
the external required-workflow controller after it enforces this descriptor. Its repository-external
workflow identity and current `provider_run_id`/`provider_run_attempt` are authenticated through the
provider, and its `execution_constraint_digest` MUST equal the current verified execution
constraint, its `sandbox_descriptor_digest` MUST equal the enclosing provenance's recomputed
`descriptor_digest`, and its `evaluation_identity_digest` MUST equal the current
`CandidateIdentityInput` digest. `SandboxVerification` has the digest constructor in the registry
even though the report embeds the complete value rather than a second redundant digest field. The
run IDs and evaluation-identity digest MUST also equal the verified trusted-time statement when one
is present. `self-asserted` requires `local-process` and null verification; `provider-verified`
requires `external-required-workflow` and non-null verification. Repository JSON, environment
variables, or a scanner-emitted assertion can never create provider assurance. A mismatch is
incomplete execution, exit 2, not a policy finding.

`isolation = process` is self-asserted only: it cannot prove socket/process/mount denial.
Provider-verified isolation requires `container` with
`mechanism = oci-rootless-sandbox-v1`, or `virtual-machine` with
`mechanism = microvm-sandbox-v1`. The verifier attests read-only mounts, no network namespace or
device/socket handles, a seccomp/job policy denying process creation after initial exec, bounded
private storage, and destroyed instance state. A container/VM label without those measured
enforcements remains self-asserted. X-07 must demonstrate the mechanism on every platform before
stable required use; an unsupported platform exits 2 rather than falling back to a plain process.

`built_in_policy_version` names the exact table later in this document. It has no separate opaque
digest: the normative table is versioned, and the executable bytes that implement it are already
bound by `engine_digest`. Any table change requires a new version literal and compatible report
schema; editing prose under `scanner-policy-defaults-v1` is not a compatible change.

## Synthetic staged-index snapshot input

`IndexProjectionInput` is the canonical complete stage-zero path/mode/object surface for `--index`.
Rows are unique, UTF-8-path-byte sorted, and prefix-free; directories are derived and deletions are
represented by absence. Every row records the index object format/OID and skip-worktree bit. Thus
the projection digest already pins every candidate blob/symlink byte sequence through its Git blob
OID and every gitlink commit without depending on later policy, document discovery, or reference
resolution.

`SyntheticSnapshotInput` is exactly
`{schema, kind, identity_scope, base_object_format, base_commit_oid, index_projection_digest}`.
Its `schema` is `assure/scanner-snapshot/v1`, `kind` is `index`, and `identity_scope` is
`complete-logical-index`. It deliberately contains no selected-entry list or raw SHA-256 digest:
selected document/control/target raw digests are later evidence facts, not snapshot identity. This
removes a dependency cycle in which policy and parsing would otherwise be needed to construct the
candidate snapshot that must exist before those stages run. `SyntheticSnapshot.entry_count` is the
exact `IndexProjectionInput.entries` length, and `snapshot_digest` is HJ over this complete small
preimage.

Before every scan, the wrapper pins one valid initial `.git/index` handle/byte string and reads the
complete supported stage-zero index—including modes, object IDs, and ordinary skip-worktree
flags—into the strict sorted `IndexProjectionInput`. Split-index backing and sparse-directory forms
are rejected as `GIT_INDEX_INVALID`; v0 neither reads a shared index nor expands a sparse directory.
After every otherwise complete or boundary-incomplete scan, it reopens the **current** `.git/index`
directory entry relative to the original `.git` handle with no-follow, boundedly reads and parses it
independently, and compares raw bytes and projection. Rereading the original fd is insufficient
because atomic replacement leaves it pinned to the old inode. Once the initial sample was valid,
any missing/nonordinary/oversized/malformed final entry or inequality is solely
`GIT_SNAPSHOT_CHANGED`; a byte-identical replacement is accepted. The stable initial projection
enters the snapshot input. Raw equality is race detection only and is not hashed into candidate
identity. Unmerged stages, intent-to-add, unreadable/missing objects,
nonrepresentable paths, a prefix conflict, or concurrent raw/logical index change makes the
candidate snapshot unavailable.
Whenever an index candidate is unavailable—including when base failure prevented its evaluation—
both `skip_worktree_paths` and `index_only_materialized_paths` are exactly zero; they are failure
sentinels, not claimed index counts. Once the projection is complete, all selected evidence bytes
are read by the pinned object OIDs rather than worktree paths. The wrapper never opens worktree
files for index candidate content.

Index projection rows are unique, path-byte sorted, and prefix-free directly after supported index
parsing. `blob` pairs only with `100644`/`100755` and an existing blob OID; `symlink` pairs only
with `120000` and an existing blob OID; `gitlink` pairs only with `160000` and a full commit OID.
Every row's object format equals the declared repository/base format and its OID length matches.
Stage-zero entries with any other mode/kind/object combination are `GIT_INDEX_INVALID`; object-kind
or readability failures use the exact closed `GIT_OBJECT_*` code.

Worktree acquisition is deliberately outside this contract. The checked-in `gitignore-v1` vectors
are research evidence only; they do not define an executable v0 mode. `--worktree` is rejected as
`INVALID_INVOCATION` before traversal. A future RFC must close the complete blocker list recorded in
[implementation-readiness.md](./implementation-readiness.md#explicitly-closed-implementation-paths)
and X-04 before adding any worktree value to a stable schema.

The report's synthetic candidate repeats the input schema, identity scope, mode, base identity,
logical-index digest, derived entry count, and snapshot digest. Each MUST equal the supplied
preimage/projection. A consumer claiming replayability must retain the complete
`IndexProjectionInput` and reachable Git objects. The result calls this a complete logical staged
index, never a Git commit or attested tree.

## Repository policy

The only candidate-owned policy path is `.assure/scanner-policy.json`. Absence has the same policy
semantics as the empty example in
[scanner-policy-v1.json](./spec/examples/scanner-policy-v1.json), but report provenance records its
digest as `null` rather than pretending a file existed. A second policy-looking file has no effect.
When present, the exact path must be an ordinary non-LFS regular blob with mode `100644`. A tree,
mode-`100755` blob, symlink, gitlink, recognized LFS pointer, or other object form is
`CONFIGURATION_INVALID` at `.assure/scanner-policy.json`, makes controls unavailable with
`invalid-repository-policy`, and is never parsed/followed. The same rule is applied independently
to base and candidate before semantic weakening comparison.
The policy can only:

- add an exact document path or exact tree root to discovery;
- add an exact protected inventory path;
- raise one schema-enumerated deterministic structural finding from its built-in
  disposition to `warn` or `fail`.

A `document` include matches exactly its path. A `tree` include matches the root itself and paths
whose bytes begin with `root + "/"`; `docs/api2` is not under `docs/api`. Matching is bytewise,
case-sensitive, and independent of host filesystem rules.

An include does not install a parser. An opted-in `.rst`, `.adoc`, HTML, or other unsupported file
is discovered as `unsupported-document-format`; it never falls back to Markdown or plain-text
equivalence. A candidate cannot exclude a built-in document, lower a disposition, add debt/waivers,
change an engine/adapter/limit, or execute a command.

Base and candidate policies are independently parsed and digested. The evaluator compares their
semantic sets. Removing an include, inventory member, or stronger disposition emits the applicable
unsuppressible meta-finding even when the candidate policy is otherwise valid. Invalid candidate
policy exits 2; it cannot erase an ordinary finding.

Repository protected inventory is an obligation, not merely a disposition hint. Evaluate the union
of base and candidate inventory paths so deleting both a rule and its document cannot erase the
check. A path in that union which is absent, unsupported, or outside candidate document coverage
emits the matching unsuppressible `coverage-reduced` control finding. A newly added inventory rule
for an already bad path therefore fails visibly; a removed rule separately emits
`policy-weakened`. Floor inventory is evaluated by the same state test and cannot be removed by
candidate content. The ordinary `document-removed` detail remains advisory and is never the sole
protection.

## Externally protected controls

Execution constraint, floor, debt, and waiver are separate external values. Their separation
prevents a digest cycle and permits a reviewed scanner release to rotate without changing the
semantic floor digest or invalidating every debt item:

1. for a required run, the provider-controlled wrapper acquires and verifies the execution
   constraint before launching the action;
2. the wrapper applies the engine-fixed raw byte/item ceilings and acquires the expected
   organization-floor digest;
3. the floor is parsed and verified, after which it may only tighten later control limits;
4. base/candidate repository policy, debt, and waiver are parsed under those effective limits;
5. debt and waiver name the already-computed floor digest, and their own expected digests are
   independently supplied and verified.

No digest is embedded in the value it hashes. A candidate-tree, artifact, cache, mutable ref,
issue comment, or repository-owned workflow input is not an external control regardless of its
filename.

The verified bundle digest, external trust source, issuer allow-list, and administrative audit of
that delivery are the authorization evidence. V0 does not accept an opaque
`authorization_evidence_digest` field whose verifier and payload are undefined; a future signed
authorization receipt requires its own schema.

The floor uses exact paths and finding kinds, not globs or regex. It may select a minimum profile,
raise deterministic findings, protect inventory/control paths, restrict waivable kinds, authorize
debt owners/waiver issuers, and tighten built-in resource limits. It cannot raise raw impact or an
unsupported result into a supposedly deterministic fact.

The execution constraint is deliberately not a floor field. Required provider execution is
instead established by the independent verified constraint plus the cross-bindings in the action,
sandbox, and trusted-time sections. For every resolved external control, repository and full ref
must equal `Evaluation.repository` and `Evaluation.ref`. Debt/waiver floor digests must equal the
verified floor digest; either exception input without that floor is invalid. The selected profile
must be at least the floor minimum under `observe < enforce`. Any violation is
`CONTROL_BINDING_MISMATCH`, incomplete, and exit 2. `ControlProvenance` discloses only controls
selected for this evaluation, never a mismatching bundle relabeled as unrelated.

Debt and waiver values are tree-bound and are legal only for `mode = commit-pair` with a complete
Git candidate snapshot. For `mode = index`, both control provenances are exactly
`status/digest/trust_source = none/null/none`; supplying either value is
`CONTROL_BINDING_MISMATCH`, fatal exit 2. V0 does not coerce a synthetic snapshot digest into the
schemas' `candidate_tree` fields. Supporting staged exceptions requires a new discriminated
candidate-identity control schema and separate review.

Every `ref` uses the frozen `ref-format-v1` contract. Its raw value is valid UTF-8, at most 266
bytes, preserved without normalization, starts with `refs/heads/`, and has a nonempty suffix. It
then applies the ten ordinary-ref rules documented by
[`git check-ref-format` for Git 2.42.x](https://git-scm.com/docs/git-check-ref-format/2.42.0): it has no
empty slash component; no component begins `.` or ends `.lock`; it contains no `..`; no byte below
0x20 or equal to 0x7f; none of space, `~`, `^`, `:`, `?`, `*`, `[`, or backslash; no leading,
trailing, or doubled slash; no trailing dot; no `@{`; and it is not the single `@`. No normalize,
branch-shorthand, one-level, or refspec-pattern mode exists. This paragraph, not the behavior of an
installed Git release, defines `ref-format-v1`; the schema regex remains only a lexical prefilter.

Repository identity v1 is GitHub-specific: `host` is exactly `github.com`. The public local CLI
requires owner/name already in canonical ASCII lowercase and rejects other spelling as
`INVALID_EVENT`; a future authenticated provider wrapper folds provider-supplied owner/name before
forming its versioned request. In both cases the stored identity is lowercase before schema
validation, hashing, URL comparison, or external control lookup. Literal unescaped GitHub URL owner/name components are independently ASCII-folded
for comparison only; their original spelling remains in the raw destination digest. Host casing,
percent encoding, IDNA, ports, and other URI components are not folded. The special repositories
`.github` and `.github-private` are representable. Branch
refs preserve provider/Git UTF-8 bytes (including `+`, `@`, and Unicode) after the broad lexical
prefilter; `ref-format-v1`, not an ASCII allow-list or mutable Git binary, decides validity.

## Finding identity, facts, and duplicates

Before finding construction, document and resolution shapes obey these closed rules:

- every non-null document side carries the exact selected Git `entry_oid`; its length matches that
  snapshot's object format, blobs/symlinks name the blob object, and gitlinks name the gitlink
  commit. This identity is recorded even when content is deliberately not read, so side equality
  cannot turn a changed excluded blob, symlink target, or gitlink commit into `unchanged`;
- a scanned document side is a regular blob with `100644`/`100755`, non-null raw digest,
  `content_availability = available`, null unsupported reason, and a compatible adapter; it reports
  exact byte/reference counts plus separate frontmatter, MDX, and HTML region/byte counts;
- a built-in-excluded side retains `entry_oid` but has null digest/adapter, `not-read`, null unsupported reason, and zero
  byte/opaque/reference counts because its content was deliberately not opened;
- every unsupported side has null adapter and zero reference/frontmatter/opaque counts. An
  unsupported-format regular blob has its exact raw digest/byte count, reason
  `unsupported-document-format`, and `available`; an LFS-pointer regular blob has its exact pointer
  digest/byte count, reason `lfs-pointer`, and `lfs-pointer-only`; a symlink has null raw digest,
  zero bytes, mode `120000`, reason `symlink-document`, and `not-read`; a gitlink has null raw
  digest, zero bytes, mode `160000`, reason `gitlink-document`, and `not-read`; symlink/gitlink
  identity remains available through `entry_oid` without opening or following content;
- entry kind and mode pairs are exact, and an invalid/unreadable/over-limit side that prevents these
  facts produces an incomplete error report rather than a partially invented `DocumentSide`;
- `frontmatter_regions` is zero or one; its byte count is zero exactly with zero regions and is at
  most 65,536 and the document byte count; every excluded/unsupported side has zero frontmatter and
  opaque counts;
- for each opaque family, regions are zero exactly when bytes are zero and every nonzero region has
  at least one byte; `markdown-v1` has zero MDX counts, `plain-advisory-v1` has zero MDX and HTML
  counts, and `mdx-v1` uses the maximal interval-union construction in the scanner specification;
- the optional UTF-8 BOM is excluded from frontmatter bytes; frontmatter, MDX-opaque, and HTML-opaque
  spans are pairwise disjoint under that precedence, so their byte-count sum cannot exceed the
  document byte count. The 65,536 bound is measured from the opener after the optional BOM, so a
  BOM-bearing accepted region may end at raw offset 65,539;
- for each side/snapshot, `extracted_references` equals the number of ordinary occurrence values for
  that document across all ObservationComparisons: count the primary side when non-null plus every
  matching `alternatives.<side>` member. Reserved governed definitions and unused definitions are
  charged to reference/parser budgets as specified but are not ordinary occurrences;
- `added` means base null/candidate present, `removed` the reverse, `unchanged` requires equal
  complete sides, and `changed` requires two unequal sides. Fatal-incomplete reports clear the
  document array, so no `unknown` document-change value exists.

`unlinked-document` has one exact v0 derivation. It is emitted once for each candidate-side
document whose status is `scanned` and whose `extracted_references` is zero, and never for an
unsupported, built-in-excluded, or base-only document. “Unlinked” means zero outgoing extracted
occurrences, not absence of inbound reachability. `summary.documents.unlinked` equals both the
number of candidate document sides satisfying this predicate and the number of these findings.

Occurrence schemas admit only `markdown-v1`/`mdx-v1` with their corresponding AST address kinds
and block owners `paragraph`, `list-item`, `table-cell`, or `document-root`. `plain-advisory-v1`
extracts zero occurrences, and opaque raw HTML cannot produce `html-block` occurrences; those
unreachable values are absent rather than accepted and rejected only by prose.

Target intent always contains its raw-destination digest. `repository-path` and
`same-repository-github` require a repository path and target kind and forbid an external scheme;
native `repository-path` uses `either` for an ordinary link, `tree` for a link with the exact
single-terminal-slash directory constructor, and `blob` for an image; an image terminal slash is
invalid and therefore constructs the `unsupported` TargetIntent instead, while
`same-repository-github` permits only `/blob/`→`blob` or `/tree/`→`tree`;
native autolinks cannot construct `repository-path` because every supported autolink grammar is an
absolute URI/email form. The cross-field validator enforces source construct, origin kind, and
target kind together; the kind-only schema condition is necessary but not sufficient.
`external-url` requires only a normalized scheme; `site-route` and `unsupported` have no repository
path, target kind, or external scheme. Query/fragment digests retain parsed bytes when present.
Correlation does not compare this full union directly: it uses the exact per-kind
`CorrelationIntentV1` projection in scanner-v0-spec. Structural reference finding keys continue to
use `RepositoryTargetIntent`; these are distinct closed projections and neither substitutes for
the other.

Target-kind compatibility is set membership: authored `blob` accepts only a regular blob, `tree`
accepts only a tree, and `either` accepts either regular blob or tree. After normalization, absence
is `missing`; a present symlink or gitlink is always the corresponding unsupported object-kind
outcome and is never followed; only a present regular blob/tree outside the accepted set is
`type-mismatch`. Literal enum-string inequality is not the test, so `either` does not mismatch an
ordinary file or directory. An LFS pointer remains a regular blob for path/type compatibility and
uses the orthogonal content-availability rule below.

A `resolved/exact-path` resolution has path, entry kind, and matching mode. An ordinary regular blob
has both content digests and `content_availability = available`; a tree has mode `040000`, null
content digests, and `not-applicable`. For a compatible authored `blob` or `either` intent, an LFS
pointer path still resolves: it is `resolved/exact-path`, regular-blob mode, retains the raw
pointer-blob digest, has null projection, and uses `lfs-pointer-only`; a separate
`unsupported-target-kind` boundary says target content was not evaluated. This preserves path
existence without claiming the LFS object was available.

All remaining resolution shapes use this exact table; “null fields” means path, entry kind, mode,
raw digest, and projection digest are all null unless the row explicitly retains path.

| Status/code | Exact repository-entry fields and availability |
| --- | --- |
| `missing/path-not-found` | Normalized attempted path; all entry/content fields null; `not-applicable` |
| `type-mismatch/target-type-mismatch`, actual regular blob | Path, `blob`, exact `100644`/`100755`, mandatory raw and projection digests; `available` |
| `type-mismatch/target-type-mismatch`, actual LFS-pointer blob | Path, `blob`, exact `100644`/`100755`, mandatory raw pointer digest, null projection; `lfs-pointer-only` |
| `type-mismatch/target-type-mismatch`, actual tree | Path, `tree`, `040000`, null content digests; `not-applicable` |
| `unsupported/symlink-entry` | Path, `symlink`, `120000`, null content digests; `not-read` |
| `unsupported/gitlink-entry` | Path, `gitlink`, `160000`, null content digests; `not-read` |
| `unsupported/unsupported-query-semantics` | Normalized repository path plus the exact already-resolved compatible blob/tree fields; ordinary blob has both digests/`available`, tree has null digests/`not-applicable`, and LFS pointer has raw pointer digest, null projection/`lfs-pointer-only` |
| `unsupported/unsupported-fragment-semantics` | Same retained compatible entry/content alternatives as the query row |
| `unsupported/unsupported-version-scope`, one unique noncandidate/default-only trusted-ref split | Parsed repository path, all entry/content fields null; `not-applicable` |
| `unsupported/unsupported-version-scope`, two distinct trusted-ref splits | Null fields; `not-applicable` |
| `unsupported/unsupported-version-scope`, no trusted-ref split (OID, tag, `HEAD`, short hash, or other nonmatching ref spelling) | Null fields; `not-applicable`; no path boundary is guessed; a branch literally having one of those spellings still matches when it equals a supplied trusted full ref |
| `unsupported/code-fragment-unevaluated` | Same retained compatible entry/content alternatives as the query row |
| `unsupported/site-route-unsupported` | Null fields; `not-applicable` |
| `unsupported/network-path-unsupported` | Null fields; `not-applicable`; no scheme is inferred for a `//authority/path` reference |
| `invalid/invalid-uri`, `invalid-percent-encoding`, `decoded-path-control`, `path-traversal`, `backslash-separator`, `encoded-slash`, `invalid-fragment-encoding`, or `invalid-reference` | Null fields; `not-applicable` |
| `external-out-of-scope/external-url` or `foreign-repository` | Null fields; `not-applicable` |

No other status/code pair exists. Type-mismatched ordinary regular blobs are selected under target
byte caps so their digests are never optional. A type-mismatched LFS pointer is instead selected
under the 1,023-byte recognizer bound and uses the explicit pointer row above; it emits only the
structural `explicit-target-type-mismatch` Finding because an authored `tree` intent never required
blob content. For a compatible `blob`/`either` intent, LFS content unavailability keeps the
occurrence's `resolved/exact-path` shape and is disclosed by a separate
`unsupported-target-kind` Finding; it is not a Resolution code. Unavailable/I/O evidence is a typed
analysis error and has no Resolution. Symlinks and gitlinks are never followed.

Every report finding embeds the exact `FindingKeyInput` and its nullable base/candidate
`FindingFactInput` values, not only their digests. The closed key-scope assignment is:

| Scope | Exact finding kinds |
| --- | --- |
| `reference` | `explicit-target-missing`, `explicit-target-type-mismatch` |
| `observation` | `invalid-reference`, `unsupported-reference-semantics`, `unsupported-target-kind`, `unsupported-version-scope`, `dependency-changed-subject-unchanged`, `dependency-and-subject-cochanged`, `subject-changed`, `explicit-reference-removed`, `external-out-of-scope`, `observation-correlation-ambiguous` |
| `document` | `unsupported-document-format`, `document-removed`, `opaque-mdx-region`, `opaque-html-region`, `unlinked-document` |
| `control` | `unsupported-capability`, `policy-weakened`, `coverage-reduced`, `control-plane-changed`, `debt-worsened`, `debt-expired`, `waiver-invalid` |

A kind under any other scope is malformed. Reference scope contains the exact document,
source-construct kind, normalized repository target intent, and containing-source projection digest.
Observation scope contains one `ObservationId`; document scope contains one `RepoPath`; control
scope contains a stable rule ID and nullable repository path. A global control location uses
`side = global`, `path = null`.

Document-scope location is closed: `document-removed` uses `side = base`, the document path, null
span, and an empty observation-ID set. `unsupported-document-format`, `opaque-mdx-region`,
`opaque-html-region`, and `unlinked-document` use `side = candidate`, the document path, null span,
and an empty observation-ID set. Opaque counts/bytes live in the embedded document fact and never
select a representative span.

For a comparison, observation scope uses the candidate `ObservationId`, falling back to the base ID
only when the candidate side is absent. Document scope uses the compared document path. Control
keys use the exact rule identity in their scope, not message text or array position.

Control-finding rule IDs and derivations are closed:

| Kind | Derivation and exact rule ID |
| --- | --- |
| `unsupported-capability` | Recognized reserved governed declaration: `unsupported/governed-claim` |
| `policy-weakened` | Removed document include: `policy/include-document-removed`; removed tree include: `policy/include-tree-removed`; removed inventory member: `policy/inventory-removed`; lowered/removed disposition: `policy/disposition/<finding-kind>` |
| `coverage-reduced` | Base/candidate repository inventory union path absent/unsupported/outside: `coverage/repository-inventory-missing`, `coverage/repository-inventory-unsupported`, or `coverage/repository-inventory-outside`; floor inventory uses the corresponding `coverage/floor-inventory-*` rule |
| `control-plane-changed` | Externally protected repository control path is added, removed, unsupported, or changes mode/bytes: `control/protected-path`; a candidate absent/unsupported state is also a failure even when base has the same state |
| `debt-expired` | `debt/<debt_id>/expired` |
| `debt-worsened` | `debt/<debt_id>/fact` |
| `waiver-invalid` | One selected-waiver defect rule ID from the waiver table below |

`FindingKeyInput.scope.control_path` is exact, not producer-selected:

| Rule family | `control_path` |
| --- | --- |
| `unsupported/governed-claim` | Affected document path |
| `policy/include-document-removed`, `policy/include-tree-removed` | Removed include path/root |
| `policy/inventory-removed` | Removed inventory path |
| `policy/disposition/<finding-kind>` | `.assure/scanner-policy.json` |
| Every `coverage/repository-inventory-*` or `coverage/floor-inventory-*` | Affected inventory path |
| `control/protected-path` | Protected control path |
| Every `debt/<id>/*` and selected `waiver/<id>/*` | Null |

The matching `FindingLocation.path` equals this non-null control path and normally uses
`side = control`; a null control path uses `side = global` and null location path. The sole
location exception is `unsupported/governed-claim`, whose representative uses `side = candidate`
and the least contributing candidate definition span. Every other control/global location has
`span = null`. Reference/observation locations take side/path/span from the least contributing
occurrence under the published representative tuple (candidate precedes base only after the
path/span/digest components tie); producers cannot attach an unrelated span. Multiple removed paths therefore produce
distinct keys without ordinals. Any other control rule/path combination is malformed.

Every control fact embeds nullable base/candidate `ControlStateInput` values and their digests.
Each non-null state repeats the exact outer rule ID and path and hashes as
`HJ("assure/scanner-control-state/v1", state)`; a digest is null exactly when its state is null.
`sources` is sorted by unique digest and gives each digest a positive multiplicity. Construction is exact:

- policy weakening has both sides, state `present` or `absent`, and the one corresponding semantic
  repository-policy digest with multiplicity one when present;
- coverage/protected-control transitions have both sides; state is `present`,
  `absent`, `unsupported`, or `outside-coverage`, and sources contain exact selected raw-blob or
  semantic-descriptor digests available for that side;
- governed syntax has null base and candidate `unsupported`, with every contributing candidate
  definition-source digest from the scanner constructor and its exact duplicate count; base-only
  definitions do not create a resolved/current control finding. Every reserved definition was already
  charged to the ordinary per-document/per-snapshot reference budgets, so at most 4,096 distinct
  source digests can enter one path-scoped state;
- debt/waiver defects have null base and candidate `invalid`, with the verified snapshot/bundle
  digest plus current fact digest when present; role-specific values also appear in the strict
  exception diagnostic.

An empty source array is valid only for `absent`, or when an unsupported/outside state has no bytes
that could safely be selected. The rule suffix, state, and source set jointly prevent missing,
unsupported, and coverage-excluded transitions from collapsing to one fact.

For each floor `protected_control_paths` member, `present` requires an ordinary non-LFS regular blob
with mode `100644` or `100755`; tree, symlink, gitlink, or LFS-pointer content is `unsupported`, and
absence is `absent`. A present source is exactly the protected-control-evidence digest over path,
mode, and raw digest. Emit `control-plane-changed` when base/candidate state or source differs, or
whenever candidate state is not `present`; if both are the same present descriptor, emit nothing.
The blob is size-checked before hashing under the selected-control per-blob/aggregate resources.
No protected control path is executed or parsed merely because the floor protects it.

Protected inventory uses a different exact state partition. For each path in the union of base and
candidate repository inventory, and for each floor inventory path, construct both sides as:

| Snapshot condition | State | Sources |
| --- | --- | --- |
| No entry | `absent` | Empty |
| Selected non-tree document side has `status = unsupported` | `unsupported` | Its raw digest when non-null, otherwise empty |
| Entry is a tree, the non-tree path is not selected as a document on that side, or its side status is `excluded-built-in` | `outside-coverage` | Empty |
| Selected document side has `status = scanned` | `present` | Its one raw-evidence digest |

Candidate `absent`, `unsupported`, and `outside-coverage` emit respectively the matching
`*-missing`, `*-unsupported`, and `*-outside` `coverage-reduced` rule, even when base has the same
state. Candidate `present` emits no coverage finding. Repository and floor prefixes are selected by
the obligation source; if both protect the same path, emit both distinct rule keys. Content change
between two present sides is not coverage reduction. Tree/symlink/gitlink/LFS and excluded cases
therefore cannot move among rule IDs by implementation choice.

V0 does not parse workflow YAML to infer an action pin or status name. Candidate workflow/action
files matter only when their exact paths are externally protected, in which case any raw digest
transition uses `control/protected-path`. The external execution constraint and provider API verify
the actual action/status source directly; their administrative rotation is not misrepresented as a
candidate-tree semantic finding.

Observation comparisons implement the scanner's exact bipartite-component algorithm. Equal IDs are
removed as exact pairs first; remaining plausible components choose the lowest base/candidate ID as
primary and contain every other full occurrence in the corresponding sorted alternatives array.
No occurrence may appear in more than one primary/alternative position, and none may be omitted.
Alternatives are empty unless correlation is `ambiguous`; ambiguous requires more than one total
base or candidate occurrence in the component. The observation finding key uses the primary
candidate ID, falling back to primary base ID, so it is deterministic without pretending one
alternative is the true counterpart.

`finding_key` MUST equal `HJ("assure/scanner-finding-key/v1", key_input)`. Each present fact MUST
repeat the same kind and key input, then carry exactly one evidence family: reference
resolution plus multiplicity, complete observation comparison, complete document result, or exact
control before/after digests plus nullable exception diagnostic. `debt-expired` and
`debt-worsened` require the full selected debt diagnostic; `waiver-invalid` requires the full
selected waiver diagnostic; every other control fact requires null. Invalid exceptions retain
owner, issuer, reason, times, candidate, and accepted/current fact identities there while the
top-level application remains null because no policy step applied. The fact digest MUST equal
`HJ("assure/scanner-fact/v1", fact_input)`. The repeated top-level kind value and every
scope/evidence discriminator MUST agree. At least one fact side is present.

Only `reference`-scope structural findings use the base/candidate attribution comparison. Every
other scope is a fact derived from the supplied evaluation pair or its control context: it
uses `attribution = not-applicable`, `base_fact = null`, and one `candidate_fact` containing the
complete observation comparison, document comparison, or control before/after digests. This avoids
pretending that a pair-derived impact fact has an independent “base side.”

Invocation, configuration, Git, discovery, parse, resolution, policy, output, and internal
failures are represented only by bounded `AnalysisError` values in `errors`. They never have a
`FindingKeyInput`, fact digest, attribution, disposition, policy trace, debt, or waiver. Their
presence makes the report incomplete with exit 2.

The canonical reference `FindingKeyInput` is also visible in every debt/waiver item. It contains:

- schema and one debt-eligible structural finding kind;
- exact document `RepoPath`;
- exact supported source-construct kind;
- canonical normalized repository target intent;
- one exact containing-source projection digest.

When several matching constructs have distinct containing-source projections, the projection binds
each exception to its reviewed context. When two or more remain indistinguishable after that
projection, no stable occurrence key can say which one an administrator reviewed. Every such
finding is ineligible for debt and waiver; a `unique` shortcut or ordinal would allow deletion or
insertion to transfer the exception to another occurrence.

Line, column, heading, human wording, and current resolution text are excluded. A source edit that
changes the contextual projection intentionally changes the key and does not inherit legacy debt.
The report still contains source positions for display.

Finding aggregation is deterministic. Construct all contributing per-occurrence facts, group them
by `(finding_key, kind)`, and emit exactly one `Finding` per group. For reference and observation
groups, `observation_ids` contains every distinct contributing ID from both snapshots, sorted by
digest bytes. `member_count` is instead the number of contributing occurrence-side locations: the
same exact ID present in base and candidate contributes two locations, not one. Thus
`locations_omitted = member_count - 1` and both values remain honest when text before an otherwise
equal occurrence moved its display span. The 8,192 bound follows from 4,096 references per document
in each of two snapshots. Document and control findings normally have no observation IDs,
`member_count = 1`, and `locations_omitted = 0`. The one exception is path-scoped
`unsupported/governed-claim`: every reserved definition in that candidate document contributes a
location, so its exact member/omitted counts and least representative span disclose aggregation
without inventing observation identity.

The representative location is the least contributing location under the total tuple
`(path, start_byte, end_byte, observation_id, side)`: UTF-8 path bytes sort first with null after
all paths; numeric span values sort next with null after integers; digest bytes sort next with null
after digests; side order is `candidate < base < control < global`. Its display line/column values
come from that same span. Aggregation never drops an ID or changes the per-side
`occurrence_multiplicity`; multiplicity greater than one remains ineligible for debt/waiver.

The policy-free structural fact includes the complete key input, resolution status/code/path,
resolved entry kind/mode/raw/projection digests when present, and the number of
occurrences sharing that key input in the evaluated document. Debt and waiver items embed that fact
body beside its digest. Creation requires multiplicity exactly one and a body reproduced by a
complete report for the named adoption/candidate tree. A later duplicate therefore changes the fact
digest and cannot borrow an existing exception.

`explicit-target-missing` requires `missing/path-not-found` with null entry/content fields.
`explicit-target-type-mismatch` requires `type-mismatch/target-type-mismatch`, a non-null actual
regular blob/tree kind and mode, and that actual kind to fall outside the target intent's accepted
set under the compatibility law above. No
other resolution status/code is debt- or waiver-eligible.

## Debt semantics

Scanner v0 defines no per-kind numeric partial order. An eligible debt item binds:

- exact finding key and key input;
- exact accepted policy-free fact digest;
- exact adoption tree;
- exact complete adoption-report payload digest whose evaluation candidate is that tree;
- authorized owner, reason, creation time, and expiry;
- exact authorizing floor digest.

Before expiry, only byte-identical fact-digest equality is `debt-tolerated`. If the candidate
finding is absent, no debt application is emitted; the ordinary base-only resolved structural
finding and the separately retained debt snapshot are sufficient audit evidence. A present key
with any other fact digest is `debt-worsened` and fails. A finding not present in the snapshot
receives no debt treatment. This exact-equality-or-resolution law
replaces the undefined `maximum_measure`; a future schema must define a per-kind order before it can
tolerate partial improvement.

At snapshot creation, the external administrator MUST verify that each item body/digest reproduces
a multiplicity-one candidate finding in the complete report named by
`adoption_report_payload_digest`, that the report candidate tree equals `adoption_tree`, and that
kind/key/fact/adapter evidence agrees. The runtime cannot reconstruct that historical payload from
its digest and does not fetch it; the field is an audit locator covered by the externally expected
DebtSnapshot digest, not standalone proof. Snapshot creation is not before any contained item
creation, every item creation time is strictly before expiry, IDs/keys are globally unique, and
items are strictly debt-ID sorted.

This historical binding is not trusted merely because its digest is present. Before matching any
current finding, the current engine reopens `adoption_tree` and policy-free re-evaluates every
distinct debt document under its current advertised adapter contracts. For each item, exactly one
ordinary occurrence must reproduce the embedded key input and accepted fact; zero, multiple,
different, or incomplete reproduction is `CONTROL_BINDING_MISMATCH`, fatal exit 2. The current
candidate's absence is treated as resolution only after **all** adoption items reproduce. Thus an
adapter/parser regression or semantic migration cannot silently erase debt; a deliberate contract
change requires a newly reviewed snapshot/report binding. Reproduction consumes the ordinary
document/parser/reference/target budgets, deduplicated by adoption document and target under the
same charging laws as a normal snapshot.

The future required wrapper must pre-acquire every object needed for that bounded adoption
reproduction into the primary object database before the networkless evaluator starts. No ambient
fetch, promisor lookup, or fallback tree is allowed. A missing/unreadable adoption object is the
ordinary `GIT_OBJECT_MISSING`/`GIT_OBJECT_UNREADABLE` fatal error; shallow/provider acquisition and
framing remain part of the request-wire/X-07 gate. The current disposable CLI is unaffected because
its debt provenance is always `none`.

The snapshot's own `created_at` must be no later than `evaluation_instant`. A future-dated
snapshot/envelope is a trusted-control time-binding error and exits 2 even if one contained item
would otherwise appear active.

At evaluation, debt is active exactly when `item.created_at <= evaluation_instant <
item.expires_at`. Equality at expiry is expired. An evaluation instant before item creation is an
invalid external-control/time binding and exits 2; it cannot suppress. An expired matching item
emits `debt-expired` and fails. Fact inequality independently emits `debt-worsened`; a matching item
that is both expired and unequal emits both findings. Expiry is evaluated before fact inequality,
but wire/human findings still use the global canonical finding-key order. There is no first-defect
precedence. Their control rule IDs are exactly `debt/<debt_id>/expired` and
`debt/<debt_id>/fact`, respectively.

Debt never applies to impact observations, analysis errors, unsupported results, policy/control
meta-findings, or indistinguishable duplicates.

Before matching items, the complete snapshot must pass schema/digest/order/uniqueness validation;
its repository/ref/floor digest must equal the current verified evaluation controls, and its
`adoption_tree` and `adoption_report_payload_digest` must name the historical report used to
authorize all embedded facts. A binding
mismatch, a debt/waiver input without a verified floor, a future evaluation before item creation,
or an unauthorized owner is `CONTROL_BINDING_MISMATCH`, incomplete, and exit 2—not a suppressible
finding.

## Waiver semantics

A waiver binds one exact finding key, authorized fact digest, repository, branch ref, and exact
candidate tree. It requires distinct accountable owner/authorized issuer, nonempty reason,
`not_before`, `expires_at`, and residual exactly `warn`.
Wildcards, missing expiry, `record` residual,
analysis/meta targets, and indistinguishable duplicates are unrepresentable.

Global bundle validation covers strict schema/digest/canonical order, unique waiver IDs, unique
`(candidate_tree, finding_key)` pairs, and intrinsic causal laws: bundle creation is not before any
contained item creation, every item satisfies `created_at <= not_before < expires_at`, and bundle
`created_at <= evaluation_instant`. It deliberately does not recompute an inactive item against the
current candidate. Current-tree selection happens only after these global checks; issuer/floor
authorization, owner distinction, active time, and key/fact-body agreement are selected-item
semantics.

A selected waiver is temporally active exactly when `not_before <= evaluation_instant <
expires_at`; equality at expiry is expired. A selected item before `not_before` or at/after expiry
has no suppressive effect and emits `waiver-invalid`.

Bundle verification precedes selection: schema, digest, canonical order, global waiver-ID
uniqueness, and global `(candidate_tree, finding_key)` uniqueness must be valid. Its lexical
repository/ref and floor digest must exactly equal the evaluation tuple; a mismatch is
`CONTROL_BINDING_MISMATCH`, incomplete, and exit 2. Only items whose `candidate_tree` differs from
the current candidate tree are inactive inventory. They have no suppressive effect, no finding,
and no selected-item semantic validation.

For each selected item, evaluate all of these closed defects in the listed construction order; each
applicable row emits one `waiver-invalid`, so several may coexist and no first error hides another.
The final wire/human findings use global canonical finding-key order, not table order:

| Defect | Exact control `rule_id` suffix |
| --- | --- |
| `evaluation_instant < not_before` | `waiver/<waiver_id>/not-yet` |
| `evaluation_instant >= expires_at` | `waiver/<waiver_id>/expired` |
| issuer absent from the floor allow-list | `waiver/<waiver_id>/issuer` |
| finding kind absent from the floor's waivable allow-list | `waiver/<waiver_id>/kind` |
| owner equals issuer | `waiver/<waiver_id>/same-owner` |
| selected finding kind/key/key body mismatch | `waiver/<waiver_id>/key` |
| authorized fact body/digest mismatch | `waiver/<waiver_id>/fact` |

Any defect means the waiver has no suppressive effect. A valid selected waiver changes only
effective disposition. It applies only when the configured disposition is `fail`, producing
`fail -> warn`; if the incoming value is already `warn`/`record`, the item is valid but inapplicable,
creates no policy step/application, and is not counted as waived. The underlying fact, finding,
attribution, fact digest, bundle-scoped waiver
ID, issuer, reason, and expiry remain in the report.

## Evaluation instant and determinism

Expiry evaluation uses `evaluation_instant`, an explicit RFC 3339 UTC value supplied by the trusted
wrapper. It is `null` when no time-dependent external debt/waiver is supplied. A required run that
needs expiry decisions without a trusted instant is incomplete and exits 2.

The trusted-time statement binds time to the current evaluation rather than merely naming a clock.
`CandidateIdentityInput` is exactly this object, populated from the strict `ResolvedEvaluation`
value before adding time:

```json
{
  "schema": "assure/scanner-candidate-identity/v1",
  "mode": "<evaluation.mode>",
  "event_kind": "<evaluation.event_kind>",
  "finality": "<evaluation.finality>",
  "repository": "<evaluation.repository>",
  "ref": "<evaluation.ref>",
  "default_branch_ref": "<evaluation.default_branch_ref>",
  "base": "<complete evaluation.base value>",
  "candidate": "<complete evaluation.candidate value>",
  "materialization": "<evaluation.materialization>",
  "skip_worktree_paths": "<evaluation.skip_worktree_paths>",
  "index_only_materialized_paths": "<evaluation.index_only_materialized_paths>"
}
```

Angle-bracket strings above are metavariables: the preimage contains the referenced JSON value with
its original type, not those strings. `candidate_identity_digest` is `HJ` over that object. Including
base, mode, finality, refs, materialization, and both snapshots prevents a time statement for the
same head commit from being replayed after a merge-base, synthetic merge, scope, or local-materialization
change. `TrustedTimeStatement.candidate_identity_digest` and
`SandboxVerification.evaluation_identity_digest` carry this same digest; the different field names
do not define different preimages.

A verified `TrustedTimeStatement` is issued by
`github-actions-required-workflow-clock-v1` inside the externally controlled run, after provider
authentication of repository, ref, run ID, attempt, and current candidate identity. Its
`statement_digest` MUST recompute using the trusted-time domain. Statement repository/ref and
candidate identity MUST equal the current evaluation; statement `evaluation_instant` MUST equal the
report field; and provider run ID/attempt MUST identify the currently executing authenticated run
and, when present, equal `SandboxVerification`. The report sets `trusted_time = true` exactly for
this verified case. A repository-authored statement, candidate output, copied run number, or
provider display string is not a trust source.

The controller issues whole-second UTC times with
`evaluation_instant < valid_until <= evaluation_instant + 600 seconds`. Before accepting the result
and publishing status, the trusted wrapper rechecks with the controller clock that
`evaluation_instant <= current_time < valid_until`, that `current_time` is still strictly before
the expiry of every applied debt/waiver item, and that the externally acquired floor, debt, waiver,
execution-constraint, ruleset, and immutable workflow-source identity digests still equal the
authenticated inputs. An exception that expires during the scan or any control/workflow rotation
before publication discards the result and requires a fresh run;
it cannot publish an earlier evaluation-time green. Thus the maximum statement TTL is ten minutes;
there is no skew allowance or date-only expiry. A different repository/ref, evaluation identity,
run ID, or attempt, a new provider rerun, an expired statement, a future issuance time, or a TTL over
600 seconds is replay/verification failure and exits 2. Repeated deterministic evaluation inside
the same authenticated attempt may reuse the same statement only while it remains valid; a later
attempt obtains a new statement.

Publication checks do not by themselves make a provider status fresh until merge: GitHub status
identity is normally candidate SHA plus context, while the base and external-control epoch are
additional validity inputs. A `stable-v1` required deployment is therefore blocked on a separate
control-epoch/provider-freshness RFC and X-07 evidence. That RFC must define an externally owned
epoch over base, candidate, ref/event, floor/debt/waiver/constraint digests and expiries plus the
active ruleset and immutable workflow-source/dependency-closure identity; an
authenticated merge-time check; and deterministic invalidation/rerun of every affected open
candidate when the base, epoch, revocation, ruleset/workflow source, or wall-clock validity changes. A stale successful
check run may not satisfy merge authorization. Until that mechanism is implemented and tested,
expiry-bearing exceptions and externally controlled required enforcement are not authorized; v0
reports claim validity only at their recorded evaluation instant and never durable merge approval.

The instant, provider run ID, and run attempt are validity inputs inside the trusted-time/sandbox
payload when provider verification is used. Determinism means byte-identical output for the same
complete evaluation tuple, including those values. Wall time used to acquire inputs, runner
hostname, provider display strings, and log timestamps remain acquisition metadata outside the
payload and cannot affect policy. This resolves the former contradiction between time-based expiry
and byte-identical reports.

## Classification and policy trace

`scanner-policy-defaults-v1` is the closed built-in table below. `observe/enforce` entries are the
first policy-step result for a candidate fact. Typed analysis errors are not rows in this table:
they have no disposition and make the result incomplete with exit 2.

| Kinds | Evidence class | Invariant class | Observe | Enforce |
| --- | --- | --- | --- | --- |
| `explicit-target-missing`, `explicit-target-type-mismatch`, `invalid-reference` | `deterministic-structural` | `ratcheted` | `warn` | `fail` |
| `unsupported-capability` | `unsupported` | `analysis-integrity` | `fail`, exit 2 | `fail`, exit 2 |
| `unsupported-reference-semantics`, `unsupported-document-format`, `unsupported-target-kind`, `unsupported-version-scope` with `coverage_requirement = none` | `unsupported` | `advisory` | `record` | `record` |
| The same four unsupported kinds with `built-in`, `repository-requested`, or `externally-protected` coverage | `unsupported` | `analysis-integrity` | `fail`, exit 2 | `fail`, exit 2 |
| `dependency-changed-subject-unchanged` | `impact-observation` | `advisory` | `warn` | `warn` |
| `dependency-and-subject-cochanged`, `subject-changed` | `impact-observation` | `advisory` | `record` | `record` |
| `explicit-reference-removed` | `coverage-boundary` | `advisory` | `warn` | `warn` |
| `document-removed`, `external-out-of-scope`, `opaque-mdx-region`, `opaque-html-region`, `observation-correlation-ambiguous`, `unlinked-document` | `coverage-boundary` | `advisory` | `record` | `record` |
| `policy-weakened`, `coverage-reduced`, `control-plane-changed`, `debt-worsened`, `debt-expired`, `waiver-invalid` | `control-plane` | `absolute` | `fail` | `fail` |

`coverage_requirement` is constructed, not producer-selected. First assign each candidate document
one coverage origin, using this precedence when several apply:
`externally-protected > repository-requested > built-in`. An exact floor inventory member is
externally protected; a policy include is repository requested; a built-in suffix/name match is
built in. Then apply this closed mapping:

| Finding family | Coverage requirement |
| --- | --- |
| `explicit-target-missing`, `explicit-target-type-mismatch`, `invalid-reference` | Candidate document's coverage origin |
| Base-only resolved projection of a former structural finding | `none` |
| `unsupported-document-format` | `repository-requested` or `externally-protected` only when that explicit origin protects/includes the path; otherwise `none` for a merely built-in-named unsupported object |
| `unsupported-capability` for reserved governed syntax | `built-in` |
| Other unsupported, impact, correlation, opaque-region, external, removal, and unlinked findings | `none` |
| Every control-plane finding | `control-plane` |

No other combination is valid. In particular, anchors, raw HTML, site routes, foreign/history
scope, LFS content, and renderer-specific syntax are disclosed non-promises; merely discovering or
protecting the containing document does not invent support for them. If a future policy requests
one of those capabilities as complete coverage, v0 must return `UNSUPPORTED_CAPABILITY` and exit 2
rather than relabel an ordinary boundary finding. A protected removal also emits the separate
absolute `coverage-reduced` fact; the raw removal observation remains `none`/advisory.

Every unsupported finding with a non-`none` coverage requirement makes the report incomplete and
is accompanied by `UNSUPPORTED_CAPABILITY`. Emit one such error for each distinct valid document
path containing one or more requested unsupported findings; use one path-null error only when no
repository path exists. The findings retain the exact kinds/evidence, while the error supplies the
exit-2 integrity signal without duplicating indistinguishable error tuples. Unsupported findings
whose requirement is `none` do not create that error and may occur in a complete report.

Policy trace construction is exact. A base-only resolved projection bypasses steps 1–6 and has
only the `resolved-projection` row defined below; every candidate/non-resolved finding uses:

1. `built-in` starts with `before = record` and applies the table for the selected profile.
2. A matching repository rule, if legal and strictly raising, applies `max`; otherwise that step is absent.
3. A matching verified floor rule, if strictly raising, applies `max`; otherwise that step is absent.
4. Exact active debt may change only an eligible reference fact to residual `warn`.
5. One exact selected waiver may change only an authorized eligible fact to residual `warn`.
6. `unsuppressible-clamp` sets an analysis-integrity or absolute control-plane finding to `fail`
   only when that strictly raises the preceding value; otherwise the step is absent.

Every adjacent step's `before` MUST equal the preceding `after`; `configured_disposition` equals
the value after step 3 (or the last earlier step), and `effective_disposition` equals the final
step's `after`. Steps appear only when applicable. Debt and waiver cannot target an unsupported,
impact, coverage, or control fact, and
they cannot target an `AnalysisError`. The clamp is last. A policy source cannot occur twice.
Apart from the mandatory built-in step on every candidate/non-resolved finding, the sole-step
`resolved-projection` trace for a base-only resolved finding, and a valid debt step retained as
adoption provenance, every emitted step MUST strictly raise or change
its input; repository/floor/clamp no-ops are omitted.

`PolicyStep.rule_id` is also closed. Producers use exactly:

| Source | Rule ID constructor |
| --- | --- |
| `built-in` | `scanner-policy-defaults-v1/<finding-kind>/<profile>` |
| `repository-policy` | `repository/<finding-kind>` |
| `organization-floor` | `floor/<finding-kind>` |
| `debt-snapshot` | `debt/<debt_id>` |
| `waiver-bundle` | `waiver/<waiver_id>` |
| `unsuppressible-clamp` | `unsuppressible/<invariant-class>` |
| `resolved-projection` | `resolved-projection-v1` |

Angle brackets denote the exact lower-case enum/ID value and are not literal characters. There is
at most one matching repository/floor disposition for a kind, so no ordinal is needed. A rule ID
outside these constructors is a malformed report.

`debt` is non-null exactly when one `debt-snapshot` step applies; `waiver` is non-null exactly when
one `waiver-bundle` step applies. The embedded application reproduces the selected item and control
digest, so its durable audit identity is `(debt_snapshot_digest, debt_id)` or
`(waiver_bundle_digest, waiver_id)`, never the bare reusable label. They are mutually exclusive: if
both valid, active, defect-free external items exactly match one finding, control evaluation fails
with `EXCEPTION_OVERLAP`, exit 2, and neither is applied. Global control validation runs first,
selected debt/waiver defects run second, and overlap runs only on the remaining applicable set. An
invalid/expired/worsened item never participates in overlap. Thus a valid debt may still be recorded
when a selected waiver is invalid, but the independent unsuppressible `waiver-invalid` finding keeps
the result failed. A valid exact exception step is retained for
provenance even when its residual equals the incoming disposition; only a debt step and the exact
`resolved-projection` step are permitted non-built-in policy no-ops. A waiver step must be
`fail -> warn`. Summary `debt_tolerated` and
`waived` count non-null applications,
respectively; they are not inferred from attribution or human text.

A base-only resolved reference finding is a diagnostic projection, not a candidate violation. It has
`candidate_fact = null`, attribution `resolved`, null debt/waiver applications, both dispositions `record`,
and exactly one `resolved-projection` step `record -> record`. Conversely, `resolved` requires a
base fact and no candidate fact. For reference scope, `introduced` requires an available fully
evaluated base, no base fact for the key, and one candidate fact; `pre-existing` requires both fact
bodies and equal recomputed digests; `resolved` requires base present/candidate absent; and
`unknown` requires present base and candidate facts with the same key and unequal fact bodies.
No other fact-presence/attribution combination is legal. Every nonreference scope is
`not-applicable` with null base fact and one candidate fact. A base/candidate self-comparison is an
invalid invocation before attribution, so it cannot mass-produce `pre-existing`.
Every candidate deterministic structural failure in `enforce` remains `fail` unless its exact
current key and fact match valid active debt or waiver; attribution alone never grandfathers it.

## Report payload

The strict report schema contains:

- explicit `experimental` or `stable-v1` producer compatibility status;
- engine, scanner-action/release-manifest, and complete adapter contract identities and digests;
- exact evaluation event/mode, repository, candidate destination ref, default-branch ref, base,
  candidate, finality, materialization/skip-worktree counts, and trusted instant;
- base/candidate repository-policy digests plus verified external-control, execution-constraint,
  sandbox, and trusted-time provenance;
- one sorted base/candidate document comparison for every discovered document path;
- one sorted observation comparison for every extracted supported/advisory adapter construct;
- policy-free resolutions, source/target projections, correlation, and impact facts;
- every finding with embedded stable key/fact preimages and digests, attribution, invariant class,
  policy trace, debt/waiver reference, and effective disposition;
- bounded typed analysis errors;
- exact or explicitly lower-bound coverage/finding counts, including omitted human-detail count;
- completeness, status, and exit class.

No report field says prose is true, fresh, correct, or reviewed. Governed-claim counts are always
zero in a complete scanner-v0 run; encountering reserved syntax instead yields unsupported
capability and an incomplete run under either profile.

The machine payload stores a digest, not the raw link destination. Canonical repository path,
query/fragment semantics, source span, and external scheme remain sufficient to reproduce the
fact from the evaluated tree without copying possible URL credentials into a report. Human output
prints no source excerpt, userinfo, raw destination, or query value and uses the exact inert
`human-atom-v1` projection in scanner-v0-spec.

### Complete and small-error reports

For `complete = true`, every detail array and summary count is exact, `counts_complete = true`, and
exit is 0 or 1. Exit 0 requires no effective `fail`; exit 1 requires at least one.

Incomplete output has two exact projections. A **boundary-incomplete** report is legal only when
every retained/logical error is `UNSUPPORTED_CAPABILITY` and no other failure occurred; the engine
finishes the full bounded structural scan, retains every document/observation/finding, derives every
count exactly, sets only `counts_complete = false`, and exits 2 because the requested semantic
surface was unsupported. A **fatal-incomplete** report is used when any other error occurs: its
`documents`, `observations`, and `findings` arrays are empty; `finding_count` and every document,
reference, disposition, attribution, debt, waiver, unsupported-capability, truncation, and governed
summary count are zero; `analysis_errors` and `error_count` alone equal the retained error-array
length; `counts_complete = false`; status is `incomplete`; and exit is 2. Evaluation, control,
engine, and unavailable-reason provenance remain because they explain the failure. Empty details in
that shape mean “discarded by the fatal projection,” never “none existed.” If the bounded small
envelope cannot be emitted, the wrapper treats missing/malformed output as failure.

The effective `machine-json-bytes` limit is exactly 67,108,864 bytes: the floor cannot tighten it.
That reservation covers the schema-maximum embedded release manifest after worst-case JSON escaping,
64 retained errors, and the fixed provenance envelope. E0 must generate the maximal-shape golden
and prove the fatal-incomplete wire fits. A schema change that exceeds it requires a new report
contract rather than an error envelope that cannot serialize itself.

The paper bound is below 48 MiB: at most 1,536 runtime paths, 32 dependency-lock paths, the bounded
manifest/artifact path fields, and 64 error paths can occur in the fatal shape; each 4,096-byte
`RepoPath` expands to at most 24,576 JSON bytes under worst-case `\u00xx` escaping, while all other
arrays are absent or have much smaller closed strings/digests. The 64 MiB reservation leaves more
than 16 MiB for fixed/object overhead. The E0 golden verifies the calculation against the actual
serializer rather than replacing it.

Let `E = min(64, verified floor typed-analysis-errors-retained limit)`, with no-floor value 64 and
schema minimum 1. First construct the logical error set required by the closed validation rules,
deduplicate full tuples, and sort by the canonical error key while retaining only the lowest `E`
keys in bounded memory. If its cardinality is at most `E`, emit it exactly. Otherwise emit the first
`E - 1` ordinary errors and one `TOO_MANY_ERRORS` sentinel with resource
`typed-analysis-errors-retained`, configured limit `E`, and observed lower bound exactly `E + 1`.
The sentinel is the explicit substitute for every omitted reason anchor; unavailable reason arrays
still retain all safely established reasons from the logical set. This overflow always selects the
fatal-incomplete projection, including when `E = 1` and the sentinel is the only retained error.
Complete reports necessarily have no analysis errors. The report schema also caps the union arrays
at 200,000 documents, 2,000,000 observations, and 100,000 findings, matching two bounded snapshots
and the stricter complete-finding ceiling; the 64 MiB wire cap may terminate earlier with exit 2.

Non-resource analysis error codes have this exact phase assignment; `RESOURCE_LIMIT_EXCEEDED`
instead takes its phase from the resource partition below:

| Phase | Codes |
| --- | --- |
| `invocation` | `INVALID_INVOCATION`, `UNSUPPORTED_PROVIDER_HOST`, `INVALID_EVENT`, `INVALID_PROFILE`, `REQUEST_UNREADABLE` |
| `configuration` | `CONFIGURATION_INVALID`, `DUPLICATE_JSON_KEY`, `INVALID_UTF8`, `INVALID_JSON`, `UNKNOWN_SCHEMA`, `UNKNOWN_FIELD`, `NONCANONICAL_ARRAY`, `DIGEST_MISMATCH`, `CONTROL_BINDING_MISMATCH`, `EXCEPTION_OVERLAP`, `TRUSTED_TIME_INVALID` |
| `git` | `GIT_REPOSITORY_UNAVAILABLE`, `GIT_OBJECT_MISSING`, `GIT_OBJECT_WRONG_KIND`, `GIT_OBJECT_UNREADABLE`, `GIT_INDEX_INVALID`, `GIT_INDEX_UNMERGED`, `GIT_INTENT_TO_ADD`, `GIT_SNAPSHOT_CHANGED`, `UNREPRESENTABLE_PATH` |
| `parse` | `DOCUMENT_INVALID`, `PARSER_ERROR`, `PARSER_PANIC`, `INVALID_SOURCE_SPAN` |
| `resolution` | `RESOLUTION_ERROR` |
| `policy` | `UNSUPPORTED_CAPABILITY` |
| `output` | `OUTPUT_LIMIT_EXCEEDED`, `REPORT_CONSTRUCTION_FAILED` |
| `internal` | `SANDBOX_VIOLATION`, `TOO_MANY_ERRORS`, `INTERNAL_ERROR` |

Parse codes are mutually exclusive per document and use this precedence. Invalid UTF-8 document
bytes or a deterministic adapter grammar rejection attributable to the complete source (including
invalid MDX syntax) is `DOCUMENT_INVALID`. After valid source has been accepted, an adapter's
declared non-source operational failure is `PARSER_ERROR`; a caught thrown exception/panic that
bypasses that result is `PARSER_PANIC`; a returned tree whose byte/line/node spans violate the
closed source contract is `INVALID_SOURCE_SPAN`. A producer emits the first applicable row and does
not relabel a grammar rejection as parser failure or add optional lower rows.

Git object codes are likewise disjoint. Absence from the primary object database is
`GIT_OBJECT_MISSING`; a present object of a different Git type than the requested commit/tree/blob
is `GIT_OBJECT_WRONG_KIND`; corrupt/truncated/hash-mismatching object bytes, an invalid tree record
or commit header, and a tree entry with a mode/object encoding outside Git's supported object
grammar are `GIT_OBJECT_UNREADABLE`. A structurally valid tree/index name outside `RepoPath` is
instead `UNREPRESENTABLE_PATH`. Invalid stage layout/mode-kind pairing in the logical index is
`GIT_INDEX_INVALID`, and any nonzero conflict stage is `GIT_INDEX_UNMERGED`. Public OID/ref lexical
defects are invocation/event errors before Git acquisition; there is no `GIT_REF_INVALID` code or
snapshot `invalid-identifier` reason.

Fatal validation order is fixed: invocation/provider tuple; release/execution constraint plus the
static sandbox descriptor/digest and actual mechanism setup (but not its candidate-bound
verification receipt); bounded acquisition, parsing, digest verification, and repository/ref applicability of the
external organization floor; repository plus base then candidate snapshot; remaining trusted-time
and floor candidate bindings plus `SandboxVerification` run/attempt/evaluation-identity
verification, debt, waiver, base policy, then candidate policy; discovery; document
parsing; resolution/correlation; policy/exception evaluation; report construction/output. Moving
the floor's bounded parse before Git is intentional: its Git-resource ceilings are active before
any tree/index/object charge. Within a stage, role order is the order just listed and repository
paths use UTF-8 byte order. A stage establishes every defect safely decidable from bytes/metadata it
has completely acquired, then no later stage runs after a non-`UNSUPPORTED_CAPABILITY` error. There
are no optional diagnostic errors.

Every malformed control emits its aggregate `CONFIGURATION_INVALID` anchor plus each safely
established applicable specific code exactly once after full-tuple deduplication:
`DUPLICATE_JSON_KEY`, `INVALID_UTF8`, `INVALID_JSON`, `UNKNOWN_SCHEMA`, `UNKNOWN_FIELD`,
`NONCANONICAL_ARRAY`, `DIGEST_MISMATCH`, or `EXCEPTION_OVERLAP`. Repository-policy defects use the
exact policy path; external control defects use null path. `CONTROL_BINDING_MISMATCH` is emitted once
for any set of cross-control/evaluation equality failures. `TRUSTED_TIME_INVALID` is emitted once,
with null path, when statement shape/digest, TTL, controller time, replay, repository/ref,
candidate, run, or attempt verification fails; it also makes external controls unavailable with
`invalid-external-control`. No producer may omit a specific code or add one merely because a parser
reports a secondary message.

`UNSUPPORTED_CAPABILITY` is emitted once per affected representable document path containing one or
more unsupported findings whose coverage requirement is not `none`. This includes built-in reserved
governed declarations and explicitly repository-requested or externally protected unsupported
document formats; multiple kinds/occurrences at one path share the error while their Findings retain
exact evidence and multiplicity. These boundary-incomplete errors are accumulated through the full
bounded scan, unlike fatal errors, and produce complete details with exit 2. A future global
protected unsupported request would use null path only after its request shape and rule ID were
published; v0 has no path-null request surface.

`RESOURCE_LIMIT_EXCEEDED` takes its phase only from this resource partition:

| Phase | Resources |
| --- | --- |
| `configuration` | `control-input-bytes`, `repository-policy-entries`, `debt-items`, `waiver-items`, `organization-policy-entries` |
| `git` | `git-object-bytes`, `git-compressed-object-bytes`, `aggregate-git-compressed-object-bytes-per-evaluation`, `git-pack-directory-entries`, `git-pack-files`, `git-pack-index-bytes`, `aggregate-git-pack-index-bytes`, `git-delta-depth`, `git-index-bytes`, `git-tree-entries-per-snapshot`, `raw-path-bytes` |
| `discovery` | `documents-per-snapshot`, `document-blob-bytes`, `aggregate-document-bytes-per-snapshot`, `selected-control-blob-bytes`, `aggregate-selected-control-bytes-per-snapshot` |
| `parse` | `raw-link-destination-bytes`, `parser-nesting`, `parser-nodes-per-document`, `parser-nodes-per-snapshot`, `references-per-document`, `references-per-snapshot` |
| `resolution` | `referenced-target-blob-bytes`, `aggregate-referenced-target-bytes-per-snapshot` |
| `policy` | `complete-findings` |
| `internal` | `evaluator-managed-memory-bytes`, `private-temporary-storage-bytes` |

Only `RESOURCE_LIMIT_EXCEEDED`, `OUTPUT_LIMIT_EXCEEDED`, and `TOO_MANY_ERRORS` carry a non-null
resource and limits. `OUTPUT_LIMIT_EXCEEDED` uses only `machine-json-bytes`;
`TOO_MANY_ERRORS` uses only `typed-analysis-errors-retained`. Both integers are present and use the
following exact observation law; every other code has all three fields null.

- Count resources report exactly `configured_limit + 1` and stop retaining/expanding at the first
  crossing. This covers tree/document/policy/debt/waiver/node/reference/finding cardinalities and
  parser nesting, pack-pair enumeration, and Git delta depth.
- A per-value byte resource reports the exact declared Git-object/fully acquired stream/path value
  length. If EOF/declared size is unavailable after reading `limit + 1`, it reports exactly
  `limit + 1` and stops. This includes inflated Git objects/delta bases, pack indexes, and the raw
  staged index. `git-compressed-object-bytes` uses the held loose-file length or the exact selected
  packed-entry interval; if a loose-file race makes that unavailable, the reader stops at limit+1.
- An aggregate byte resource reports the exact prior charged total plus the exact declared size of
  the first member that would cross it; per-value limits are checked first, so a rejected oversized
  member is not also charged to the aggregate.
- Managed-memory and private-temporary-storage errors report current exact charged bytes plus the
  exact next allocation/write request. Allocation and write accounting is engine-owned and occurs
  before the operation.
- When a verified floor first activates a managed-memory or temporary-storage limit below usage
  already charged while acquiring/parsing that floor, activation is itself the crossing and
  `observed_lower_bound` is the exact current charged usage (no synthetic next request). Equality
  is allowed; the next nonzero allocation/write then uses the ordinary rule. This makes a declared
  zero limit deterministic rather than an implementation-selected delayed failure.
- `OUTPUT_LIMIT_EXCEEDED` reports the exact byte length produced by a counting canonical-serialization
  pass for the attempted non-error envelope. `TOO_MANY_ERRORS` reports exactly `E + 1` as defined
  above.

All arithmetic saturates at the maximum safe integer only after establishing a crossing; a
saturated value is exactly `9007199254740991`. The two dedicated resources MUST NOT use
`RESOURCE_LIMIT_EXCEEDED`, eliminating two codes for one event.

The wrapper reserves a streaming fatal-envelope serializer and its fixed scratch space before
evaluator allocation accounting begins. That reserve is outside the floor-tunable
`evaluator-managed-memory-bytes` pool but inside the 1 GiB physical sandbox cap and exact 64 MiB
wire cap; it can emit only the fatal projection, never continue analysis. E0's maximal error golden
must prove both its byte cap and scratch bound. A floor may therefore force immediate evaluator
failure without making the failure unreportable.

Resource-error paths use this exact partition:

| Resource | `AnalysisError.path` |
| --- | --- |
| `control-input-bytes` | `.assure/scanner-policy.json` for either repository policy; null for floor/debt/waiver |
| `selected-control-blob-bytes` | Exact selected repository control path |
| `repository-policy-entries` | `.assure/scanner-policy.json` |
| `git-object-bytes`, `git-compressed-object-bytes`, `git-pack-index-bytes` | Null |
| `aggregate-git-compressed-object-bytes-per-evaluation`, `git-pack-directory-entries`, `git-pack-files`, `aggregate-git-pack-index-bytes`, `git-delta-depth`, `git-index-bytes` | Null |
| `raw-path-bytes` | Null |
| `document-blob-bytes` | Exact document path |
| `referenced-target-blob-bytes` | Lowest UTF-8-byte normalized path that references the charged object |
| `raw-link-destination-bytes`, `parser-nesting`, `parser-nodes-per-document`, `references-per-document` | Exact source document path |
| `git-tree-entries-per-snapshot`, `documents-per-snapshot`, `aggregate-selected-control-bytes-per-snapshot`, `debt-items`, `waiver-items`, `aggregate-referenced-target-bytes-per-snapshot`, `aggregate-document-bytes-per-snapshot`, `parser-nodes-per-snapshot`, `references-per-snapshot`, `organization-policy-entries`, `complete-findings`, `typed-analysis-errors-retained`, `machine-json-bytes`, `private-temporary-storage-bytes`, `evaluator-managed-memory-bytes` | Null |

Parse/resolution errors otherwise carry their exact valid source/target path as defined by their
constructor. `UNREPRESENTABLE_PATH` alone requires null path and non-null full lowercase
`path_bytes_hex`; a `raw-path-bytes` resource breach has both null because the over-limit path
cannot fit that field. Every other error has null byte hex.

Charging membership is logical and independent of caching. `git-tree-entries-per-snapshot` counts
each distinct non-root logical path node in the complete snapshot trie exactly once—directory
prefixes plus blob/symlink/gitlink leaves. Commit trees and supported ordinary indexes therefore
charge the same logical surface; sparse-directory indexes are rejected before charging. Traversal
is raw-path-byte ordered and stops on node `limit + 1`.
`git-pack-directory-entries` counts every actual native directory entry returned beneath the primary
`objects/pack` handle, including ignored, temporary, symlink, and malformed names, before any names
are retained or sorted; only `.`/`..` pseudoentries are excluded if an API exposes them. After a within-limit enumeration, candidate pair names sort by raw bytes;
per-index then aggregate-index bytes charge in that order. Compressed storage charges use the exact
per-snapshot/OID rule in scanner-v0-spec, including selected delta members and independent
base/candidate charges.
Document aggregate bytes charge each discovered non-null document blob once per snapshot/path in
path order, even when two paths share an OID. Selected-control aggregate bytes use the same
per-snapshot/path rule.

Referenced-target aggregate bytes instead charge each distinct regular-blob Git object identity
`(object_format, blob_oid)` once per snapshot, regardless of occurrence/path multiplicity; tree,
symlink, and gitlink entries charge zero target bytes. After resolution discovers the complete
bounded target set, identities sort by object-format enum order then raw OID bytes. The per-object
limit is checked from the Git header first, followed by aggregate prior-total-plus-object-size. If a
per-object error needs a path, use the smallest normalized referring path; the aggregate crossing
path is null. Memoizing a blob may improve runtime but cannot alter either charge.

### Canonical ordering

The producer orders:

1. adapters by `adapter_id`;
2. documents by `path` UTF-8 bytes, with paths unique;
3. observation comparisons by primary candidate `observation_id`, falling back to primary base ID,
   with that key unique and every primary/alternative occurrence globally unique;
4. findings by `finding_key`, then kind, with `(finding_key, kind)` unique and every
   `observation_ids` array digest-sorted and duplicate-free;
5. errors by the total tuple `(phase, code, path, path_bytes_hex, resource, configured_limit,
   observed_lower_bound)`: phase uses schema enum order, strings otherwise use UTF-8 bytes, numbers
   use numeric order, and null sorts before a present value at each nullable component; duplicate
   full tuples are rejected;
6. policy trace in actual composition order.

All counts are derived from the arrays and full scan, not trusted producer inputs. The consumer
checks `finding_count`, `error_count`, and disposition totals before accepting a report.

For complete and boundary-incomplete reports, document and reference summary counts describe the
complete candidate snapshot: base-only removed
documents/occurrences remain in the detail arrays but do not inflate candidate discovery or
resolution totals. Candidate occurrences means every non-null primary candidate plus every member
of `alternatives.candidate`, each counted once. `extracted` is that cardinality;
`explicit_local` selects exactly `intent.kind = repository-path`; `same_repository_github` selects
exactly `intent.kind = same-repository-github`; `external_out_of_scope` selects exactly
`resolution.status = external-out-of-scope`; `unsupported` selects exactly
`resolution.status = unsupported`; and `resolved`/`missing` select those two exact statuses. These
buckets intentionally overlap across intent and outcome axes. Across the outcome axis,
`extracted = resolved + missing + unsupported + external_out_of_scope + type-mismatch occurrences
+ invalid occurrences`, where the final two derived counts are checked but not separately published.
Finding totals describe the base/candidate union and therefore include base-only `resolved`
projections. `human_details_truncated` counts findings omitted only from the human projection; no
JSON finding is omitted in a complete or boundary-incomplete report.
`analysis_errors` equals the typed `errors` array length; `unsupported_capabilities` counts findings
whose kind is exactly `unsupported-capability`. Disposition and attribution buckets each partition
the complete finding array.

`outside_document_set` counts candidate non-tree entries—regular blobs, symlinks, and gitlinks—whose
paths are neither a built-in document-name candidate nor a policy document/tree include. Directory
nodes are derived and never counted. Built-in-excluded document names and unsupported/policy-included
documents remain inside the document denominator; an exact path is counted once even when several
selection rules match. Thus `outside_document_set + documents.discovered` partitions the flattened
candidate non-tree path set before document status, without pretending all code/assets are docs.
Frontmatter/opaque document counts count candidate documents with at least one region of that exact
family; region and byte totals are sums of the matching candidate `DocumentSide` fields.
Frontmatter, MDX, and HTML totals never substitute for, overlap into, or infer each other.

### Cross-field validity

The producer and wrapper additionally enforce rules that JSON Schema cannot express compactly:

- `sha1` object format requires 40 hex characters and `sha256` requires 64;
- every `UtcInstant` is parsed as an actual proleptic-Gregorian calendar instant with exact
  `YYYY-MM-DDTHH:MM:SSZ` spelling; impossible dates and leap seconds are rejected;
- adapter IDs/descriptors/digests and every action commit/tree/manifest-path/artifact-path,
  release-manifest/lock/engine-version/engine-digest, and execution-constraint binding obey the
  exact constructors and runtime-closure rules above;
- complete commit-pair candidates are Git commits, index mode uses the matching synthetic kind, and
  event/finality combinations follow the CI event table; `unavailable` snapshot values
  are legal only in an incomplete exit-2 error payload;
- commit-pair mode has `materialization = git-objects`, zero skip/index-only counts, and the exact
  provider/explicit event-finality row; index mode has `local-index/local-nonfinal` and
  `materialization = index`; when its candidate is resolved, candidate kind is `index`,
  `skip_worktree_paths` equals the number of logical index entries whose `skip_worktree` bit is
  `true`, and `index_only_materialized_paths = 0`; when its candidate is unavailable, both counters
  are the exact zero failure sentinels;
- synthetic snapshot summary fields reproduce the supplied canonical input and referenced complete
  logical-index projection, including derived entry count, base identity, and snapshot digest;
- a non-null repository identity requires exact candidate and default branch refs that pass Git
  ref validation; without all three, GitHub URLs are external/not recognized as same-repository
  rather than resolved from ambient remotes;
- a `none` floor/debt/waiver control has `digest = null` and `trust_source = none`; `verified` has
  both a digest and exactly `external-required-workflow` or `organization-ruleset` as its external
  source—action artifacts are never control trust sources;
- `trusted_time_source.status = none` requires null `evaluation_instant` and
  `trusted_time = false`; `verified` requires the exact statement/digest/controller, current
  candidate identity, repository/ref, run/attempt, instant, maximum-TTL, and replay checks above,
  and sets `trusted_time = true`; expiry-bearing controls require the verified form;
- the sandbox descriptor and optional verification obey their constructors and zero-capability
  contract; `self-asserted/local-process/null` and
  `provider-verified/external-required-workflow/non-null` are the only assurance/source/verification
  combinations, and the verified run/attempt/constraint/descriptor/evaluation bindings match
  current controls and evaluation;
- the reserved future `stable-v1` shape has verified execution constraint, GitHub-action
  provenance, and provider-verified sandbox; all pinned action fields agree, but no deployment is
  authorized until the separate request/control-epoch proof also authenticates the exact active
  workflow source, current candidate/ref/event applicability, and merge-time epoch; a local
  experimental run has no execution constraint;
- source spans are ordered and remain inside their document byte/line bounds; byte offsets are
  zero-based half-open offsets into raw blob bytes, while line and column are one-based Unicode
  scalar positions after CRLF and bare-CR conversion to LF; no endpoint may split a CRLF pair;
- every within-limit Git path outside the `RepoPath` domain uses lowercase full path-byte hex in
  `path_bytes_hex`; a valid `RepoPath` and byte-hex identifier are mutually exclusive for one error;
- resolution status controls which path, object kind, projection, and candidate fields may exist;
- every occurrence's repeated fields and adapter/address compatibility match its embedded
  `ObservationIdInput`, whose digest reproduces `observation_id`;
- an observation comparison has at least one side and its correlation/source/target/impact tuple
  is one legal row of the scanner derivation tables;
- finding scope/evidence families, repeated values, key/fact digests, attribution fact-presence law,
  classification table, policy-step chain/order, dispositions, debt/waiver applicability, and
  resolved projection obey the preceding sections;
- complete reports have exact summaries; `finding_count`, `error_count`, disposition and
  every attribution total including `not_applicable`, debt/waiver usage, analysis-error, unsupported
  capability, and reference-category totals match arrays; result status and exit are `pass/0` or
  `fail/1`;
- incomplete reports use `incomplete/2`, contain at least one error, and never claim exact unknown
  denominators;
- governed/unattested claim counts are zero because governed syntax is unsupported in scanner v0.

A violation is a malformed producer result, not a policy finding. The wrapper rejects it.

## Closed v0 taxonomy

The machine enum in the report schema is authoritative. Typed analysis failures are separately
represented by `AnalysisError` and are not members of `FindingKind`. The finding families are:

- deterministic structure: missing/type-mismatched targets and invalid references;
- explicit unsupported boundaries: reference semantics, document format, target kind, version
  scope, and requested capability;
- raw impact: dependency changed with unchanged subject, dependency/subject co-change, and subject
  change;
- coverage/boundary: explicit reference/document removal, external URL, opaque MDX/HTML,
  ambiguous observation correlation, and unlinked document;
- control plane: `policy-weakened`, `coverage-reduced`, `control-plane-changed`, `debt-worsened`,
  `debt-expired`, and `waiver-invalid`.

Historical prose aliases such as `external-unevaluated`, `out-of-scope-external`,
`probable-path-missing`, and `migration-candidate` are not scanner-v0 machine kinds. Inference is
outside the stable scanner-v0 API and report rather than an inference-request finding/count.

## Compatibility and rejection rules

Consumers reject an unknown major, unknown validity enum, missing required field, noncanonical
array, digest mismatch, inconsistent totals, or impossible result/exit combination. Minor additive
fields require a new schema that explicitly permits them; `additionalProperties: false` means a v1
consumer never guesses whether an unknown value affects validity.

The following remain outside this wire contract:

- governed claim definitions, records, acceptance, lifecycle, and provider receipts;
- named checks, executable validators, renderer adapters, and inference results;
- SARIF, PR comments, fixes, artifacts, editor state, and caches;
- signature algorithms, key rotation/revocation, and service replay policy.

Those capabilities require separate schemas and their closed implementation gates. Reusing a v0
field to smuggle them in is incompatible.
