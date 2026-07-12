import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { writeFileSync } from "node:fs";
import { dirname } from "node:path";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const experimentDir = dirname(fileURLToPath(import.meta.url));
const root = resolve(experimentDir, "../..");
const outputArg = process.argv.indexOf("--out");
const outputPath = outputArg >= 0 ? resolve(root, process.argv[outputArg + 1]) : undefined;
const scan = JSON.parse(readFileSync(resolve(experimentDir, "current-scan.json"), "utf8"));

function key(record) {
  return `${record.document}:${record.position.start.line}:${record.token}`;
}

function sampleHash(record) {
  return createHash("sha256").update(key(record)).digest("hex");
}

const labels = {
  ".github/PULL_REQUEST_TEMPLATE.md:11:proofs/isabelle/SpecRest/IR.thy": ["actionable-current-reference", "Current pull-request instructions name the pre-split IR path; the tracked file is now under core/IR.thy."],
  ".github/PULL_REQUEST_TEMPLATE.md:18:proofs/isabelle/SpecRest/IR.thy": ["actionable-current-reference", "Current pull-request checklist repeats the pre-split IR path."],
  ".github/PULL_REQUEST_TEMPLATE.md:21:proofs/isabelle/SpecRest/Soundness.thy": ["actionable-current-reference", "Current pull-request checklist names a removed proof path; the current soundness tree is split."],
  "docs/content/docs/pipelines/concurrency.mdx:222:./modules/cli/target/native-image/spec-to-rest": ["expected-generated-output", "Build output is intentionally absent from the tracked tree."],
  "docs/content/docs/targets/python/fastapi/postgres.mdx:186:modules/codegen/src/main/scala/specrest/codegen/RouteKind.scala": ["actionable-current-reference", "Current target reference names a deleted source file."],
  "docs/content/docs/targets/python/fastapi/postgres.mdx:200:modules/codegen/src/main/scala/specrest/codegen/RouteKind.scala": ["actionable-current-reference", "Current target reference repeats the deleted source path."],
  "proofs/isabelle/README.md:127:proofs/lean/": ["historical-or-planned-reference", "A phase table intentionally describes retiring the former Lean tree."],
  "proofs/isabelle/SPEEDUP.md:343:proofs/isabelle/SpecRest/IR.thy": ["historical-record", "The speedup log records a path as it existed before the proof-session split."],
  "proofs/isabelle/SPEEDUP.md:344:proofs/isabelle/SpecRest/Semantics.thy": ["historical-record", "The speedup log records the former pre-split path."],
  "proofs/isabelle/SPEEDUP.md:345:proofs/isabelle/SpecRest/Smt.thy": ["historical-record", "The speedup log records the former pre-split path."],
  "proofs/isabelle/SPEEDUP.md:346:proofs/isabelle/SpecRest/Soundness.thy": ["historical-record", "The speedup log records the former pre-split path."],
  "proofs/isabelle/SPEEDUP.md:348:proofs/isabelle/SpecRest/Codegen.thy": ["historical-record", "The speedup log records the former pre-split path."],
  "proofs/isabelle/STATUS.md:3:proofs/lean/": ["historical-record", "The status file explicitly says this tree was retired."],
  "proofs/isabelle/STATUS.md:32:.github/workflows/lean-certs.yml": ["historical-record", "A completed-phase row explicitly records deletion of the workflow."],
  "proofs/isabelle/STATUS.md:36:.github/workflows/lean.yml": ["historical-record", "A completed-phase row explicitly records deletion of the workflow."],
  "proofs/isabelle/STATUS.md:36:proofs/lean/": ["historical-record", "A completed-phase row explicitly records deletion of the tree."],
  "docs/content/docs/synth/realizability.mdx:64:_externs.py": ["generated-or-reader-created-path", "The page names a generated target-project file, not a tracked repository file."],
  "docs/content/docs/targets/ts/express/postgres.mdx:227:_strategies.ts": ["generated-or-reader-created-path", "The page names a generated test harness file."],
  "proofs/isabelle/STATUS.md:343:Soundness.thy": ["historical-record", "The status record uses the basename of the former monolithic proof file."],
  "proofs/isabelle/SPEEDUP.md:46:Soundness.thy": ["historical-record", "The speedup report measures the former monolithic proof file."],
  "proofs/isabelle/SPEEDUP.md:715:Soundness.thy": ["historical-record", "The speedup report discusses a deferred edit against the then-current file."],
  "docs/content/docs/targets/ts/express/postgres.mdx:178:docker-compose.override.yml": ["generated-or-reader-created-path", "The page instructs the reader to create this file from the tracked example."],
  "proofs/isabelle/SPEEDUP.md:483:Preservation.thy": ["historical-record", "The speedup report names the former pre-split proof file."],
  "proofs/isabelle/README.md:136:EvalGenerated.scala": ["generated-or-reader-created-path", "The file is produced during the documented extraction/evaluation flow."],
  "proofs/isabelle/STATUS.md:26:SpecRest.thy": ["historical-record", "A completed-phase status row records an earlier skeleton file."],
  "docs/content/docs/pipelines/test-generation/behavioral.mdx:144:_testgen_skips.json": ["generated-or-reader-created-path", "The page names a generated output contract."],
  "proofs/isabelle/README.md:117:SpecRest.thy": ["historical-record", "A phase-plan table records an earlier skeleton file."],
  "proofs/isabelle/STATUS.md:378:Soundness.thy": ["historical-record", "The status record names the former monolithic proof file."],
  "docs/content/docs/targets/go/chi/postgres.mdx:119:TargetLanguage.Go": ["not-a-path", "A qualified Scala enum/member was selected only because its suffix resembles a .go extension."],
  "proofs/isabelle/SPEEDUP.md:497:Preservation.thy": ["historical-record", "The speedup report explicitly discusses splitting the former file."],
  "proofs/isabelle/SPEEDUP.md:406:Preservation.thy": ["historical-record", "The speedup report is a measurement table for the former file."],
  "docs/content/docs/targets/ts/express/postgres.mdx:227:_predicates.ts": ["generated-or-reader-created-path", "The page names a generated test harness file."],
  "docs/content/docs/targets/go/chi/postgres.mdx:168:docker-compose.override.yml": ["generated-or-reader-created-path", "The page instructs the reader to create this file from the tracked example."],
  "fixtures/golden/codegen/python/fastapi/sqlite/url_shortener/README.md:3:UrlShortener.spec": ["generated-artifact-prose", "A generated README names its logical source, not a same-directory tracked path."],
  "docs/content/docs/pipelines/test-generation/behavioral.mdx:87:_testgen_skips.json": ["generated-or-reader-created-path", "The page names a generated output contract."],
  "fixtures/golden/codegen/python/fastapi/postgres/url_shortener/README.md:3:UrlShortener.spec": ["generated-artifact-prose", "A generated README names its logical source, not a same-directory tracked path."],
};

const missing = scan.inlinePaths.records.filter((record) => record.resolution.status === "missing");
const repositoryRooted = missing.filter((record) => record.resolution.classification === "repository-rooted-inline");
const ambiguous = missing
  .filter((record) => record.resolution.classification === "ambiguous-inline")
  .map((record) => ({ ...record, sampleHash: sampleHash(record) }))
  .sort((a, b) => a.sampleHash.localeCompare(b.sampleHash))
  .slice(0, 20);
const selected = [...repositoryRooted, ...ambiguous].map((record) => {
  const label = labels[key(record)];
  if (!label) throw new Error(`missing manual label for ${key(record)}`);
  return {
    stratum: record.resolution.classification === "repository-rooted-inline" ? "repository-rooted-census" : "deterministic-ambiguous-sample",
    sampleHash: record.sampleHash,
    document: record.document,
    line: record.position.start.line,
    token: record.token,
    context: record.context,
    reviewLabel: label[0],
    rationale: label[1],
    actionableCurrentReference: label[0] === "actionable-current-reference",
  };
});

function summarize(items) {
  const labels = {};
  for (const item of items) labels[item.reviewLabel] = (labels[item.reviewLabel] ?? 0) + 1;
  return {
    occurrences: items.length,
    actionableCurrentReferences: items.filter((item) => item.actionableCurrentReference).length,
    actionableFraction: items.length === 0 ? undefined : items.filter((item) => item.actionableCurrentReference).length / items.length,
    labels: Object.fromEntries(Object.entries(labels).sort(([a], [b]) => a.localeCompare(b))),
  };
}

const report = {
  schema: "ci-idea/inline-path-sample/v1",
  method: {
    repositoryRooted: "Census of all current missing repository-rooted inline-code candidates.",
    ambiguous: "Twenty records with the lexicographically smallest SHA-256(document:line:token), selected before labeling.",
    limitation: "One repository and manual labels; this is a calibration sample, not an external precision estimate.",
  },
  population: {
    allInlineCandidates: scan.inlinePaths.bindingCandidateCount,
    allMissingCandidates: missing.length,
    missingRepositoryRooted: repositoryRooted.length,
    missingAmbiguous: missing.filter((record) => record.resolution.classification === "ambiguous-inline").length,
  },
  summary: {
    allSelected: summarize(selected),
    repositoryRootedCensus: summarize(selected.filter((item) => item.stratum === "repository-rooted-census")),
    ambiguousSample: summarize(selected.filter((item) => item.stratum === "deterministic-ambiguous-sample")),
  },
  records: selected,
};
const serialized = `${JSON.stringify(report, null, 2)}\n`;
if (outputPath) writeFileSync(outputPath, serialized);
else process.stdout.write(serialized);
