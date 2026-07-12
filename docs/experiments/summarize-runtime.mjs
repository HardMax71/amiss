import { readFileSync } from "node:fs";
import { writeFileSync } from "node:fs";
import { resolve } from "node:path";

function argument(name) {
  const index = process.argv.indexOf(name);
  if (index < 0) throw new Error(`${name} is required`);
  return resolve(process.argv[index + 1]);
}

function percentile(values, quantile) {
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.min(sorted.length - 1, Math.ceil(quantile * sorted.length) - 1)];
}

function statistics(values) {
  return {
    min: Math.min(...values),
    mean: values.reduce((sum, value) => sum + value, 0) / values.length,
    p50: percentile(values, 0.5),
    p95: percentile(values, 0.95),
    max: Math.max(...values),
  };
}

const lines = readFileSync(argument("--input"), "utf8").trim().split(/\r?\n/u);
const header = lines[0].split("\t");
const rows = lines.slice(1).map((line) => Object.fromEntries(line.split("\t").map((value, index) => [header[index], value])));
const scanner = rows.filter((row) => row.command === "experiment-scanner");
const existing = rows.filter((row) => row.command === "existing-link-checker");
const report = {
  schema: "ci-idea/runtime-benchmark/v1",
  environment: {
    node: process.version,
    cpu: readFileSync("/proc/cpuinfo", "utf8").match(/^model name\s*:\s*(.+)$/mu)?.[1],
    memoryKiB: Number(readFileSync("/proc/meminfo", "utf8").match(/^MemTotal:\s+(\d+) kB$/mu)?.[1]),
  },
  method: "Separate Node process per run on a warm local filesystem. External wall time uses date +%s%N; scanner self-time starts after ESM dependency loading. This is not a CI runner benchmark.",
  experimentScanner: {
    runs: scanner.length,
    externalWallMilliseconds: statistics(scanner.map((row) => Number(row.external_wall_ms))),
    postImportInternalMilliseconds: statistics(scanner.map((row) => Number(row.internal_ms))),
    maxRssKiB: statistics(scanner.map((row) => Number(row.max_rss_kib))),
    outputBytes: statistics(scanner.map((row) => Number(row.output_bytes))),
  },
  existingLinkChecker: {
    runs: existing.length,
    externalWallMilliseconds: statistics(existing.map((row) => Number(row.external_wall_ms))),
    reportedFilesPerRun: 90,
  },
};
writeFileSync(argument("--out"), `${JSON.stringify(report, null, 2)}\n`);
