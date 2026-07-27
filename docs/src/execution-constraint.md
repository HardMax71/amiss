# Prepare the execution constraint

Every provider lane loads one execution constraint from operator-owned storage. It pins the
action repository, exact action commit and tree, release manifest, target platform, stable result
name, and the exact bootstrap executable the service may run.

`amiss-constraint` builds that existing contract from a local action checkout and bootstrap. It
does not introduce another configuration format.

## Build

Build the bootstrap and the companion tool from the same reviewed source commit as the provider
service:

```sh
cargo build --release --locked -p amiss-bootstrap --bin amiss-bootstrap
cargo build --release --locked \
  -p amiss-controller-constraint --bin amiss-constraint
```

Both land in `target/release`; no system-wide installation is needed. Windows binary names carry the normal `.exe` suffix.

Acquire the published action repository independently, as an ordinary non-bare checkout with a
real `.git` directory. Choose and record the full immutable action commit. The checkout must
already contain that object; the tool does not fetch, run `git`, read a ref, or consult `HEAD`,
remotes, and worktree files.

Then create a new constraint file:

```sh
target/release/amiss-constraint \
  --action-repository /absolute/path/to/action-checkout \
  --action-identity github.com/example/amiss \
  --action-commit eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee \
  --bootstrap /absolute/path/to/amiss-bootstrap \
  --required-status-name "amiss / assure" \
  --output /absolute/path/to/execution-constraint.json
```

A lone `--version` reports the tool's own version instead of building anything, which is the way
to confirm the producer matches the release it is provisioning for.

Use native absolute paths on the deployment system. `--action-identity` is the separately
supplied logical `host/owner/name`; a nested GitLab owner such as
`gitlab.example/platform/security/amiss` is valid. The tool does not read a Git remote or compare
that identity with the manifest's build source. For a provider-local mirror, use the exact commit
present on that provider even if recreated commits differ from upstream.

`--action-commit` is the action-tree commit consumers pin, not the source commit recorded inside
the release manifest. Current provider lanes require a full 40-character SHA-1 commit, and this
provider-facing command rejects every other object format.

The output path must not exist. The tool validates everything before publishing the canonical
file and never replaces an existing file. On success it prints the constraint's semantic digest,
which is useful for the deployment record but is not a signature.

The bootstrap and output parent must resolve outside the action checkout. Create a private output
directory first. Generate the file as the service account or have the deployment mechanism make
the new file readable by that account without exposing it to the checked repository.

## Supplied and derived values

Only the values that require an operator decision are supplied:

| Supplied value | Meaning |
| --- | --- |
| Action checkout | The local primary checkout that already contains the action commit. |
| Action identity | The forge host and repository whose action tree is trusted. |
| Action commit | The independently selected full commit ID already present in the local object store. |
| Bootstrap | The exact local executable that the service will later load. |
| Required status name | The stable result name bound into controller state and the report. |

The rest comes from those exact bytes:

| Derived value | Source |
| --- | --- |
| Object format | The current provider contract's fixed SHA-1 namespace, confirmed while reading the commit. |
| Action tree | The tree named by that commit object. |
| Manifest path | The release's fixed `release-manifest.json` path. |
| Manifest digest | The parsed manifest's semantic digest. |
| Target platform | The bootstrap executable header. |
| Bootstrap digest | The domain-separated digest of the bootstrap bytes. |
| Schema, bootstrap contract, descriptor digest | The existing wire constructor and canonical writer. |

The tool resolves the manifest, dependency locks, engine, launcher, and action metadata from the
pinned Git tree. It checks every dependency-lock digest and every mode and digest in the selected
platform's runtime closure, then requires the engine and bootstrap headers to name the same
platform. It also requires `release-manifest.digest` to reproduce the parsed manifest digest.
That small file is a consistency marker, not a trust anchor; the semantic digest is recomputed
from the manifest itself.

## Trust and rotation

Generation proves the supplied action object store, commit, manifest, and selected runtime
closure are internally consistent, and that the runtime and bootstrap headers name the same
platform. It does not authenticate where those inputs came from, select the trusted commit, prove
that the supplied executable is an Amiss bootstrap, sign the output, or make a report
provider-verified. It binds the bootstrap's exact bytes. The operator-controlled acquisition,
service host, and deployment storage must protect input origin and program identity.

Keep the action checkout used for preparation read-only. Store the generated constraint,
bootstrap, provider credentials, and optional controls outside both the checked repository and
the action checkout. Protect them from the repository and its CI identities.

Generate a new file when the action repository or commit, bootstrap executable, or required
status name changes. Use a fresh versioned output path because the producer never overwrites.
Review its printed digest, update the service configuration and provider gate as one change, then
retain the old deployment material until in-flight work under the old binding has drained. Do
not regenerate the constraint automatically at service startup: that would turn observed drift
into newly trusted input.
