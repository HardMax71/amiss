process.stderr.write(
  [
    "amiss: this action cannot be run with `uses:`, and nothing was checked.",
    "",
    "The launcher is listed in the release manifest so that its bytes are pinned",
    "and reviewed, but the required path never executes it. A verifying run",
    "acquires this action tree as data, validates the manifest, the runtime",
    "closure, and the engine digest with the separately protected amiss-bootstrap,",
    "and only then execs the engine. Letting a Node process that the action tree",
    "itself supplies do that job would run unvalidated code to decide whether the",
    "code is valid.",
    "",
    "That lane is not built yet. Until it is, run the engine directly:",
    "",
    "  amiss check --repo . --object-format sha1 \\",
    "              --base <full-oid> --candidate <full-oid> --profile observe",
    "",
    "https://github.com/HardMax71/amiss",
    "",
  ].join("\n"),
);
process.exit(2);
