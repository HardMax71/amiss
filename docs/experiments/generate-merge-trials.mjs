import { mkdirSync } from "node:fs";
import { writeFileSync } from "node:fs";
import { resolve } from "node:path";

const directoryArg = process.argv.indexOf("--dir");
const trialsArg = process.argv.indexOf("--trials");
if (directoryArg < 0) throw new Error("--dir is required");
const outputDirectory = resolve(process.argv[directoryArg + 1]);
const trials = trialsArg >= 0 ? Number(process.argv[trialsArg + 1]) : 50;
const seedText = "assure-physical-layout-2026-07-11";
const claimCount = 250;
const workloads = [1, 5, 20];

function claimId(index) {
  return `claim-${String(index).padStart(4, "0")}`;
}

function claimLine(index, state) {
  return `${JSON.stringify({
    claim_id: claimId(index),
    predecessor: state === "base" ? null : "sha256:base",
    projection: `sha256:${state.padEnd(64, String(index % 10)).slice(0, 64)}`,
    state,
  })}\n`;
}

function seedFromText(value) {
  let seed = 2166136261;
  for (const character of value) {
    seed ^= character.codePointAt(0);
    seed = Math.imul(seed, 16777619);
  }
  return seed >>> 0;
}

function randomGenerator(seed) {
  let state = seed || 1;
  return () => {
    state ^= state << 13;
    state ^= state >>> 17;
    state ^= state << 5;
    return (state >>> 0) / 0x1_0000_0000;
  };
}

function disjointSelections(random, count) {
  const indices = Array.from({ length: claimCount }, (_, index) => index);
  for (let index = indices.length - 1; index > 0; index -= 1) {
    const other = Math.floor(random() * (index + 1));
    [indices[index], indices[other]] = [indices[other], indices[index]];
  }
  return { a: indices.slice(0, count), b: indices.slice(count, count * 2) };
}

mkdirSync(outputDirectory, { recursive: true });
const candidatesDirectory = resolve(outputDirectory, "single-candidates");
mkdirSync(candidatesDirectory, { recursive: true });
const baseLines = Array.from({ length: claimCount }, (_, index) => claimLine(index, "base"));
const basePath = resolve(outputDirectory, "base.jsonl");
writeFileSync(basePath, baseLines.join(""));
const random = randomGenerator(seedFromText(seedText));
const manifest = ["updates_per_branch\ttrial\ta_path\tbase_path\tb_path"];
const representative = {};

for (const updatesPerBranch of workloads) {
  for (let trial = 0; trial < trials; trial += 1) {
    const selected = disjointSelections(random, updatesPerBranch);
    representative[updatesPerBranch] ??= selected;
    const aLines = [...baseLines];
    const bLines = [...baseLines];
    for (const index of selected.a) aLines[index] = claimLine(index, "accepted-a");
    for (const index of selected.b) bLines[index] = claimLine(index, "accepted-b");
    const aPath = resolve(candidatesDirectory, `${updatesPerBranch}-${trial}-a.jsonl`);
    const bPath = resolve(candidatesDirectory, `${updatesPerBranch}-${trial}-b.jsonl`);
    writeFileSync(aPath, aLines.join(""));
    writeFileSync(bPath, bLines.join(""));
    manifest.push(`${updatesPerBranch}\t${trial}\t${aPath}\t${basePath}\t${bPath}`);
  }
}
writeFileSync(resolve(outputDirectory, "manifest.tsv"), `${manifest.join("\n")}\n`);

for (const updatesPerBranch of workloads) {
  const repo = resolve(outputDirectory, `sharded-${updatesPerBranch}`);
  const claims = resolve(repo, "claims");
  const updatesA = resolve(repo, "updates-a");
  const updatesB = resolve(repo, "updates-b");
  mkdirSync(claims, { recursive: true });
  mkdirSync(updatesA, { recursive: true });
  mkdirSync(updatesB, { recursive: true });
  for (let index = 0; index < claimCount; index += 1) {
    writeFileSync(resolve(claims, `${claimId(index)}.json`), `${JSON.stringify({ claim_id: claimId(index), state: "base" })}\n`);
  }
  for (const index of representative[updatesPerBranch].a) {
    writeFileSync(resolve(updatesA, `${claimId(index)}.json`), `${JSON.stringify({ claim_id: claimId(index), state: "accepted-a" })}\n`);
  }
  for (const index of representative[updatesPerBranch].b) {
    writeFileSync(resolve(updatesB, `${claimId(index)}.json`), `${JSON.stringify({ claim_id: claimId(index), state: "accepted-b" })}\n`);
  }
}

writeFileSync(resolve(outputDirectory, "metadata.json"), `${JSON.stringify({
  seed: seedText,
  seedInteger: seedFromText(seedText),
  claimCount,
  trialsPerWorkload: trials,
  workloads,
  selection: "Uniform shuffle without replacement; A and B ClaimId sets are disjoint.",
  representative,
}, null, 2)}\n`);
