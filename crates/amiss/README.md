# amiss

Amiss checks the links and file references in a repository's documentation against the
git tree and fails the run when one stops resolving. It compares two commits, or a commit
and the staged index, so it also reports a file that changed under a paragraph that did
not. It runs in-process with no subprocesses, no network, and no writes, and the same input
through the same binary always produces the same report bytes.

Install from crates.io, or with cargo-binstall, which takes the prebuilt release binary:

```sh
cargo install --locked amiss
```

Then check the staged state against the last commit. `--object-format` is `sha1` for nearly
every repository, and `git rev-parse --show-object-format` prints yours:

```sh
amiss check --repo . --object-format sha1 \
  --base "$(git rev-parse HEAD)" --index --profile observe
```

Exit 0 means nothing blocks, 1 means a finding blocks, and 2 means the run could not be
trusted. The [quickstart](https://hardmax71.github.io/amiss/quickstart.html) goes from
install to a green run on one page, and the rest of the
[documentation](https://hardmax71.github.io/amiss/) has the grammar and the CI forms.
