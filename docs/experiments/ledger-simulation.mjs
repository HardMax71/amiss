import { createHash } from "node:crypto";
import { gzipSync } from "node:zlib";
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
const history = JSON.parse(readFileSync(resolve(experimentDir, "history-replay.json"), "utf8"));

function sha256(value) {
  return `sha256:${createHash("sha256").update(value).digest("hex")}`;
}

function stableSelectorId(target) {
  return sha256(`selector\0${target}`);
}

function isFileReference(record) {
  return [
    "same-repo-github",
    "same-repo-github-pinned-or-foreign-ref",
    "site-root-local",
    "document-relative-local",
    "repository-rooted-fence",
  ].includes(record.resolution.classification);
}

function targetFor(record) {
  return record.resolution.matches?.[0] ?? record.resolution.normalizedPath ?? record.target;
}

const referenceRecords = scan.references.records.filter(isFileReference);
const groups = new Map();
for (const record of referenceRecords) {
  const key = `${record.document}\0${record.block?.digest ?? `line:${record.position?.start.line ?? 0}`}`;
  const group = groups.get(key) ?? {
    document: record.document,
    blockDigest: record.block?.digest,
    blockType: record.block?.type,
    lineStart: record.block?.lineStart,
    lineEnd: record.block?.lineEnd,
    context: record.context,
    dependencies: [],
  };
  group.dependencies.push({
    target: targetFor(record),
    status: record.resolution.status,
    selectorKind: record.sourceKind.startsWith("fence-") ? "source-bearing-fence" : "path-exists",
    origin: record.resolution.classification,
  });
  groups.set(key, group);
}

const hyperedges = [...groups.values()]
  .map((group) => ({
    ...group,
    dependencies: [...new Map(group.dependencies.map((dependency) => [`${dependency.selectorKind}\0${dependency.target}`, dependency])).values()]
      .sort((a, b) => a.target.localeCompare(b.target)),
  }))
  .sort((a, b) => a.document.localeCompare(b.document) || (a.lineStart ?? 0) - (b.lineStart ?? 0));

const minimalRecords = hyperedges.map((edge) => ({
  id: sha256(`observation\0${edge.document}\0${edge.blockDigest}\0${edge.dependencies.map((dependency) => dependency.target).join("\0")}`),
  document: edge.document,
  block: edge.blockDigest,
  dependencies: edge.dependencies.map((dependency) => ({ target: dependency.target, status: dependency.status })),
}));

const detailedRecords = hyperedges.map((edge) => {
  const observationId = sha256(`experiment-observation\0${edge.document}\0${edge.blockDigest}\0${edge.dependencies.map((dependency) => dependency.target).join("\0")}`);
  return {
    record_type: "automatic-observation",
    schema: 1,
    observation_id: observationId,
    origin: "inferred-explicit-reference",
    declaration: {
      document: edge.document,
      block_locator: { line_start: edge.lineStart, line_end: edge.lineEnd, block_type: edge.blockType },
      declaration_digest: sha256(`declaration\0${edge.document}\0${edge.dependencies.map((dependency) => dependency.target).join("\0")}`),
    },
    subject: {
      selector_id: stableSelectorId(`${edge.document}#${edge.lineStart}`),
      resolution: "resolved",
      raw_digest: edge.blockDigest,
      projection_digest: edge.blockDigest,
      selector_engine: "experiment-mdast@1",
      projection_schema: 1,
      display_summary: edge.context,
    },
    dependencies: edge.dependencies.map((dependency) => ({
      selector_id: stableSelectorId(dependency.target),
      selector_kind: dependency.selectorKind,
      target: dependency.target,
      resolution: dependency.status,
      projection_digest: dependency.status === "resolved" ? sha256(`experiment-target\0${dependency.target}`) : null,
      selector_engine: "experiment-path-resolver@1",
      projection_schema: 1,
    })),
    attestation: { status: "unattested", trust: "automatic" },
    lifecycle: "ephemeral",
  };
});

function jsonLines(records) {
  return `${records.map((record) => JSON.stringify(record)).join("\n")}\n`;
}

function bytes(value) {
  return Buffer.byteLength(value);
}

function sizeStats(serialized, count) {
  const uncompressed = bytes(serialized);
  const compressed = gzipSync(serialized, { level: 9 }).length;
  return {
    records: count,
    uncompressedBytes: uncompressed,
    gzipBytes: compressed,
    meanBytesPerRecord: count === 0 ? 0 : uncompressed / count,
    compressionRatio: uncompressed === 0 ? 0 : compressed / uncompressed,
  };
}

function extrapolate(meanBytesPerRecord) {
  return [1_000, 10_000, 100_000, 1_000_000].map((records) => ({
    records,
    estimatedBytes: Math.ceil(records * meanBytesPerRecord),
  }));
}

function isoWeek(dateString) {
  const date = new Date(`${dateString.slice(0, 10)}T00:00:00Z`);
  const day = date.getUTCDay() || 7;
  date.setUTCDate(date.getUTCDate() + 4 - day);
  const yearStart = new Date(Date.UTC(date.getUTCFullYear(), 0, 1));
  const week = Math.ceil((((date - yearStart) / 86_400_000) + 1) / 7);
  return `${date.getUTCFullYear()}-W${String(week).padStart(2, "0")}`;
}

const minimalJsonl = jsonLines(minimalRecords);
const detailedJsonl = jsonLines(detailedRecords);
writeFileSync(resolve(experimentDir, "simulated-minimal.jsonl"), minimalJsonl);
writeFileSync(resolve(experimentDir, "simulated-detailed.jsonl"), detailedJsonl);
const minimalStats = sizeStats(minimalJsonl, minimalRecords.length);
const detailedStats = sizeStats(detailedJsonl, detailedRecords.length);
const dependencyCount = hyperedges.reduce((sum, edge) => sum + edge.dependencies.length, 0);
const uniqueTargets = new Set(hyperedges.flatMap((edge) => edge.dependencies.map((dependency) => dependency.target))).size;
const subjectBodyAssumption = 500;
const dependencyBodyAssumption = 2_048;
const perRelationBodyStore = detailedStats.uncompressedBytes + hyperedges.length * subjectBodyAssumption + dependencyCount * dependencyBodyAssumption;
const deduplicatedBodyStore = detailedStats.uncompressedBytes + hyperedges.length * subjectBodyAssumption + uniqueTargets * dependencyBodyAssumption;
const impactEvents = history.survivingCurrentImplementationGraphReplay.events;
const impactDays = new Set(impactEvents.map((event) => event.authoredAt.slice(0, 10)));
const impactWeeks = new Set(impactEvents.map((event) => isoWeek(event.authoredAt)));

const report = {
  schema: "ci-idea/ledger-simulation/v1",
  warning: "Synthetic sizing only. No public ledger, observation, claim, or acceptance schema is implied.",
  cardinality: {
    discoveredDocuments: scan.discovery.recommendedScannerScope.count,
    explicitFileReferenceOccurrences: referenceRecords.length,
    groupedBlockHyperedges: hyperedges.length,
    dependencyEndpointsAfterWithinBlockDeduplication: dependencyCount,
    uniqueTargets,
    resolvedInlineBindingCandidates: scan.inlinePaths.records.filter((record) => record.resolution.status === "resolved").length,
    allInlineBindingCandidatesUpperBound: scan.inlinePaths.bindingCandidateCount,
    upperBoundIfEveryInlineCandidateBecameAnotherEndpoint: dependencyCount + scan.inlinePaths.bindingCandidateCount,
  },
  serializedSpecimens: {
    minimal: { file: "simulated-minimal.jsonl", ...minimalStats, extrapolation: extrapolate(minimalStats.meanBytesPerRecord) },
    detailedAutomaticObservation: { file: "simulated-detailed.jsonl", ...detailedStats, extrapolation: extrapolate(detailedStats.meanBytesPerRecord) },
  },
  projectionBodyStoreScenario: {
    assumptions: {
      meanSubjectBodyBytes: subjectBodyAssumption,
      meanDependencyProjectionBodyBytes: dependencyBodyAssumption,
      compressionAndObjectStoreOverhead: "excluded",
    },
    perRelationBodiesBytes: perRelationBodyStore,
    contentAddressedTargetDeduplicationBytes: deduplicatedBodyStore,
    savingFractionFromTargetDeduplication: perRelationBodyStore === 0 ? 0 : 1 - deduplicatedBodyStore / perRelationBodyStore,
  },
  churnScenario: {
    basis: "Surviving current implementation-target graph replay from 2026-01-01; it excludes historical relations and does not label actionability.",
    historyCommits: history.churn.commits,
    commitsWithAtLeastOneCurrentImplementationRelationImpact: impactEvents.length,
    relationEndpointImpactEvents: history.survivingCurrentImplementationGraphReplay.relationImpactEvents,
    possiblePerMergeObservationWriterCommits: impactEvents.length,
    possibleDailyBatchedWriterCommits: impactDays.size,
    possibleWeeklyBatchedWriterCommits: impactWeeks.size,
    approximateTwoLineJsonlChurnBytesIfEveryEndpointImpactRewroteOneDetailedRecord: Math.ceil(2 * detailedStats.meanBytesPerRecord * history.survivingCurrentImplementationGraphReplay.relationImpactEvents),
    knownBrokenCasesContainingBlockEditsWhileStillBroken: history.knownBrokenCaseReplay.totals.containingBlockEditsWhileBroken,
    statelessScannerRepositoryWrites: 0,
    caveats: [
      "Impact does not mean an acceptance or write should occur.",
      "Current-graph replay has survivorship bias and overstates whole-file selector impact where a narrower projection would be used.",
      "The known-case block-edit count is a five-case lower bound, not a corpus rate.",
      "Merge conflicts are measured separately with synthetic Git merges.",
    ],
  },
};
const serialized = `${JSON.stringify(report, null, 2)}\n`;
if (outputPath) writeFileSync(outputPath, serialized);
else process.stdout.write(serialized);
