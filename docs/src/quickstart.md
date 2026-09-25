# Quickstart

Amiss checks the links and file references in your documentation against the git tree and
fails the run when one stops resolving. This page goes from install to a green run. The
[Introduction](introduction.md) says what the tool is and is not, and
[Invocation](invocation.md) has the whole grammar.

Install from crates.io:

```sh
cargo install --locked amiss
```

Or download a binary from the [release page](https://github.com/HardMax71/amiss/releases).
Each release ships `amiss-linux-x86_64`, `amiss-linux-aarch64`, `amiss-macos-x86_64`,
`amiss-macos-aarch64` and `amiss-windows-x86_64.exe`, plus a `SHA256SUMS` file and its sigstore
bundle; the `amiss-probe-*` files beside them are the external prober, not the CLI. The
download arrives without the executable bit, and `SHA256SUMS` lists every asset, so the check
skips the ones you did not fetch:

```sh
curl -sSLO https://github.com/HardMax71/amiss/releases/latest/download/amiss-linux-x86_64
curl -sSLO https://github.com/HardMax71/amiss/releases/latest/download/SHA256SUMS
sha256sum -c --ignore-missing SHA256SUMS
chmod +x amiss-linux-x86_64 && mv amiss-linux-x86_64 amiss
```

Put `amiss` on your PATH. On macOS, `shasum -a 256 -c --ignore-missing SHA256SUMS` is the
same check. `cargo binstall amiss` does the download and rename for you, and
`gh attestation verify amiss --repo HardMax71/amiss` (gh 2.49 or later) proves the file came
from this repository's release workflow; [Security model](security.md#verified-consumption)
has the CI form of that.

Now run it from the repository root. The command checks the staged index against `HEAD`, so
it works on a depth-1 clone and on any repository with a commit. Before the first commit there
is nothing to compare against, and the published hook passes with a note until there is one.

```sh
amiss check --repo . --object-format sha1 \
  --base "$(git rev-parse HEAD)" --index --profile observe
```

`--object-format` is `sha1` for nearly every repository; `git rev-parse --show-object-format`
prints yours. `--base` takes a full commit id, never a branch name, and `HEAD~1` has no
answer on a one-commit repository or a shallow clone, so the parent form is refused before
the scan starts.

The first line is the verdict, and the rest is detail:

```text
amiss: pass (fix 0, check 0, pre-existing 0, errors 0, exit 0)
```

A Fix is a reference this change broke; `observe` reports it without blocking, `enforce`
fails the run on it. A Check is a file that changed under a paragraph that did not, listed
for a person to read and never a verdict. Pre-existing is the backlog, the problems that were
already there before this change. The same Fix counts under both profiles; the exit code
carries the verdict. 0 means the run completed and nothing blocks, 1 means a finding blocks,
and 2 means the run itself could not be trusted, so there is no verdict to act on.
[Profiles and findings](profiles.md) lists every finding kind with its disposition, and
[Analysis errors](errors.md) every code behind that 2.

There is no ignore file, no exclude list, and no way to silence one finding. The nine skipped
directory names are fixed (`node_modules`, `vendor`, `third_party`, `dist`, `build`, `.next`,
`target`, `test`, `tests`), and a run always reads the whole repository, so a monorepo cannot
scope it to one package. The ramp for a repository with a backlog is
`--profile enforce-introduced`: what a change introduces blocks, the pre-existing rows stay
warnings, and you work them off on your own clock. External http links are never fetched;
the report lists them, and [Amiss and link checkers](comparison.md) shows the one pipe that
hands them to lychee.

A first run can still report hundreds of missing targets against a tree nobody has broken. That
happens when the site is built somewhere else. A generator's configuration file in the tree is
what selects its rule, so a repository that keeps only its documentation gets none, and every
destination is read against the source files rather than the URLs the site publishes.

A repository can name its own router in one line. `.amiss/router.yml` says which router
publishes the directory it sits in, and that directory is where the rule anchors:

```yaml
router: directory-pages
```

That name is a shape rather than a generator: a site publishing every page at a directory of
its own name and rewriting no destination, which Hugo, Jekyll, Eleventy and Astro all build by
default. `docusaurus`, `mkdocs`, `sphinx` and `zola` are the other four names that turn a
spelling on, and every router name is read for the second key below, whether it turns one on
or not.

A second line says where that directory is published, and it is what answers the destinations
opening with a slash:

```yaml
router: astro
base: /
```

With that file under `src/content/docs`, `/en/guides/astro-components/` is the `.mdx` of that
name under it, and the Astro documentation resolves 8,663 references it used to leave
undecided. A route the tree cannot answer stays undecided, so the key adds no missing target.

The promise above holds. A declaration can move a destination onto a file the tree already
holds, and it can never clear one the tree lacks, so nothing that is really missing goes quiet.
Grafana keeps its Hugo configuration in a Docker image and a sibling repository. With that file
under `docs/sources` its run reports 153 missing targets instead of 546, and every one of the
393 that go reaches a file the tree already holds. Adding the file is safe under every profile:
both sides of a comparison are read under the routers the candidate declares, so the commit that
writes one introduces nothing. Deleting one is reported at the file that held it.
[What a documentation router serves](route-spellings.md) lists every name you can write and
what each one turns on.

In CI the same engine ships as a GitHub Action that derives both commits from the event:

```yaml
name: docs
on:
  pull_request:
  push:
    branches: [main]
permissions:
  contents: read
jobs:
  amiss:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1
        with:
          fetch-depth: ${{ github.event_name == 'pull_request' && 2 || 0 }}
      - id: amiss
        uses: HardMax71/amiss@v0
        with:
          profile: observe
      - if: always()
        uses: actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a # v7.0.1
        with:
          name: amiss-report
          path: ${{ steps.amiss.outputs.report }}
          if-no-files-found: ignore
```

The `profile` input defaults to `enforce`; the snippet starts at `observe` so the first
report can be triaged without blocking anyone. `fetch-depth` gives the checkout both commits
the action compares: a pull request compares the merge commit with its first parent, a push
compares the event's before and after. The upload keeps the JSON report where a failed run
can be read. [Running it in CI](ci.md) has the direct form, GitLab, and the pre-commit hook.
