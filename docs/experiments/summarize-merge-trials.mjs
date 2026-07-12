import { readFileSync } from "node:fs";
import { writeFileSync } from "node:fs";
import { resolve } from "node:path";

function argument(name) {
  const index = process.argv.indexOf(name);
  if (index < 0) throw new Error(`${name} is required`);
  return resolve(process.argv[index + 1]);
}

function rows(path) {
  const lines = readFileSync(path, "utf8").trim().split(/\r?\n/u);
  const header = lines[0].split("\t");
  return lines.slice(1).map((line) => Object.fromEntries(line.split("\t").map((value, index) => [header[index], value])));
}

function wilson(successes, total) {
  if (total === 0) return { low: 0, high: 0 };
  const z = 1.959963984540054;
  const p = successes / total;
  const denominator = 1 + z * z / total;
  const center = (p + z * z / (2 * total)) / denominator;
  const margin = z * Math.sqrt((p * (1 - p) + z * z / (4 * total)) / total) / denominator;
  return { low: Math.max(0, center - margin), high: Math.min(1, center + margin) };
}

const metadata = JSON.parse(readFileSync(argument("--metadata"), "utf8"));
const singleRows = rows(argument("--single-results"));
const shardedRows = rows(argument("--sharded-results"));
const results = metadata.workloads.map((updatesPerBranch) => {
  const single = singleRows.filter((row) => Number(row.updates_per_branch) === updatesPerBranch);
  const sharded = shardedRows.find((row) => Number(row.updates_per_branch) === updatesPerBranch);
  const conflicts = single.filter((row) => row.result === "conflict").length;
  return {
    updatesPerBranch,
    semanticallyDisjoint: true,
    layouts: {
      singleSortedJsonl: {
        engine: "git merge-file -p --diff3 for every trial",
        trials: single.length,
        conflicts,
        conflictRate: conflicts / single.length,
        wilson95: wilson(conflicts, single.length),
      },
      oneFilePerClaim: {
        engine: "Representative real git merge per workload; zero path overlap by construction in every generated trial",
        trialsRepresented: metadata.trialsPerWorkload,
        representativeMergeResult: sharded.result,
        conflicts: sharded.result === "conflict" ? 1 : 0,
        structuralConflictRateUnderStatedNoRenameAssumption: sharded.result === "conflict" ? undefined : 0,
      },
    },
  };
});
const report = {
  schema: "ci-idea/merge-conflict-simulation/v1",
  workload: metadata,
  interpretation: {
    measured: "Git textual conflicts for every single-file trial and a real repository merge for one representative sharded trial per workload.",
    conditionalConclusion: "With disjoint ClaimId paths and no renames or directory/file collisions, every generated sharded trial has zero both-modified paths; the representative Git merges validate that tree-level assumption.",
    notMeasured: [
      "same-claim compare-and-swap conflicts",
      "filesystem performance or path-count limits",
      "renames, case-folding, or directory/file collisions",
      "document/declaration conflicts",
      "a production ledger serializer",
    ],
  },
  results,
};
writeFileSync(argument("--out"), `${JSON.stringify(report, null, 2)}\n`);
