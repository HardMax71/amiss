import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { writeFileSync } from "node:fs";
import { dirname } from "node:path";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const experimentDir = dirname(fileURLToPath(import.meta.url));
const root = resolve(experimentDir, "../..");
const outputArg = process.argv.indexOf("--out");
const outputPath = outputArg >= 0 ? resolve(root, process.argv[outputArg + 1]) : undefined;
const since = "2026-01-01";

function git(args, allowFailure = false) {
  const result = spawnSync("git", args, { cwd: root, encoding: "utf8", maxBuffer: 256 * 1024 * 1024 });
  if (result.status !== 0 && !allowFailure) throw new Error(`git ${args.join(" ")} failed: ${result.stderr}`);
  return result.status === 0 ? result.stdout : undefined;
}

function sha256(value) {
  return createHash("sha256").update(value).digest("hex");
}

function isDocumentPath(path) {
  const name = path.split("/").at(-1) ?? "";
  const extension = /\.[^.]+$/u.exec(name)?.[0].toLowerCase();
  return extension === ".md" || extension === ".mdx" || name === ".cursorrules" || name === "llms.txt" || (!path.includes("/") && /^(?:README|CONTRIBUTING|CHANGELOG)(?:[._-].*)?$/iu.test(name));
}

function parseFirstParentLog() {
  const raw = git([
    "log",
    "--first-parent",
    "--reverse",
    `--since=${since}`,
    "--date=iso-strict",
    "--format=@@%x09%H%x09%P%x09%aI%x09%s",
    "--name-status",
    "-M",
    "HEAD",
  ]);
  const commits = [];
  let current;
  for (const line of raw.split(/\r?\n/u)) {
    if (line.startsWith("@@\t")) {
      const [, hash, parents, authoredAt, ...subject] = line.split("\t");
      current = { hash, parents: parents.split(" ").filter(Boolean), authoredAt, subject: subject.join("\t"), changes: [] };
      commits.push(current);
      continue;
    }
    if (!current || line.trim() === "") continue;
    const fields = line.split("\t");
    const status = fields[0];
    if (/^[RC]/u.test(status)) {
      current.changes.push({ status, oldPath: fields[1], path: fields[2] });
    } else {
      current.changes.push({ status, path: fields[1] });
    }
  }
  return commits;
}

function percentile(values, quantile) {
  if (values.length === 0) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.min(sorted.length - 1, Math.ceil(quantile * sorted.length) - 1)];
}

function blocksContaining(source, needle) {
  const normalized = source.replaceAll("\r\n", "\n").replaceAll("\r", "\n");
  const lines = normalized.split("\n");
  const blocks = [];
  let start = 0;
  for (let index = 0; index <= lines.length; index += 1) {
    if (index < lines.length && lines[index].trim() !== "") continue;
    const block = lines.slice(start, index).join("\n");
    if (block.includes(needle)) blocks.push(block);
    start = index + 1;
  }
  return blocks;
}

function snapshot(commit, referenceCase) {
  const source = git(["show", `${commit}:${referenceCase.document}`], true);
  const docOid = git(["rev-parse", `${commit}:${referenceCase.document}`], true)?.trim();
  const targetExists = git(["cat-file", "-e", `${commit}:${referenceCase.target}`], true) !== undefined;
  if (source === undefined) {
    return { commit, documentExists: false, referencePresent: false, targetExists };
  }
  const blocks = blocksContaining(source, referenceCase.needle);
  return {
    commit,
    documentExists: true,
    documentOid: docOid,
    referencePresent: source.includes(referenceCase.needle),
    containingBlockDigest: blocks.length === 0 ? undefined : `sha256:${sha256(blocks.join("\n\n"))}`,
    containingBlocks: blocks.length,
    targetExists,
    broken: blocks.length > 0 && !targetExists,
  };
}

function replayKnownCase(referenceCase, commitMetadata) {
  const commits = git([
    "log",
    "--first-parent",
    "--reverse",
    "--format=%H",
    "--",
    referenceCase.document,
    referenceCase.target,
  ]).split(/\r?\n/u).filter(Boolean);
  const head = git(["rev-parse", "HEAD"]).trim();
  if (!commits.includes(head)) commits.push(head);
  const firstParent = commits.length > 0 ? git(["rev-parse", `${commits[0]}^`], true)?.trim() : undefined;
  if (firstParent) commits.unshift(firstParent);
  const states = [...new Set(commits)].map((commit) => ({ ...snapshot(commit, referenceCase), ...commitMetadata.get(commit) }));
  const transitions = [];
  for (let index = 1; index < states.length; index += 1) {
    const before = states[index - 1];
    const after = states[index];
    const kinds = [];
    if (before.targetExists && !after.targetExists && after.referencePresent) kinds.push("target-disappeared-while-reference-remained");
    if (!before.referencePresent && after.referencePresent && !after.targetExists) kinds.push("broken-reference-added");
    if (before.broken && after.broken && before.documentOid !== after.documentOid) kinds.push("document-file-edited-while-broken");
    if (before.broken && after.broken && before.containingBlockDigest !== after.containingBlockDigest) kinds.push("containing-block-edited-while-broken");
    if (before.broken && !after.broken) kinds.push("broken-state-cleared");
    if (kinds.length > 0) transitions.push({ from: before.commit, to: after.commit, kinds, afterAuthoredAt: after.authoredAt, afterSubject: after.subject });
  }
  return {
    ...referenceCase,
    states,
    transitions,
    summary: {
      relevantSnapshots: states.length,
      targetDisappearanceEvents: transitions.filter((transition) => transition.kinds.includes("target-disappeared-while-reference-remained")).length,
      brokenReferenceAddedEvents: transitions.filter((transition) => transition.kinds.includes("broken-reference-added")).length,
      documentFileEditsWhileBroken: transitions.filter((transition) => transition.kinds.includes("document-file-edited-while-broken")).length,
      containingBlockEditsWhileBroken: transitions.filter((transition) => transition.kinds.includes("containing-block-edited-while-broken")).length,
      currentlyBroken: states.at(-1)?.broken ?? false,
    },
  };
}

const commits = parseFirstParentLog();
const commitMetadata = new Map(commits.map((commit) => [commit.hash, { authoredAt: commit.authoredAt, subject: commit.subject }]));
const commitByHash = new Map(commits.map((commit) => [commit.hash, commit]));
const changedPathCounts = new Map();
for (const commit of commits) {
  for (const change of commit.changes) {
    changedPathCounts.set(change.path, (changedPathCounts.get(change.path) ?? 0) + 1);
    if (change.oldPath) changedPathCounts.set(change.oldPath, (changedPathCounts.get(change.oldPath) ?? 0) + 1);
  }
}
const docCommits = commits.filter((commit) => commit.changes.some((change) => isDocumentPath(change.path) || (change.oldPath && isDocumentPath(change.oldPath))));
const codeCommits = commits.filter((commit) => commit.changes.some((change) => !isDocumentPath(change.path) && !(change.oldPath && isDocumentPath(change.oldPath))));
const bothCommits = commits.filter((commit) => docCommits.includes(commit) && codeCommits.includes(commit));

const scan = JSON.parse(readFileSync(resolve(experimentDir, "current-scan.json"), "utf8"));
const currentRelations = scan.references.records
  .filter((record) => record.resolution.status === "resolved")
  .filter((record) => ["same-repo-github", "site-root-local", "document-relative-local", "repository-rooted-fence"].includes(record.resolution.classification))
  .filter((record) => typeof record.resolution.matches?.[0] === "string")
  .map((record, index) => ({
    id: `${record.document}:${record.position?.start.line ?? 0}:${index}`,
    document: record.document,
    target: record.resolution.matches[0],
    targetIsDocument: isDocumentPath(record.resolution.matches[0]),
    blockDigest: record.block?.digest,
  }));
const relationsByTarget = new Map();
for (const relation of currentRelations) {
  const existing = relationsByTarget.get(relation.target) ?? [];
  existing.push(relation);
  relationsByTarget.set(relation.target, existing);
}
const impactEvents = [];
for (const commit of commits) {
  const changed = new Set(commit.changes.flatMap((change) => [change.path, change.oldPath].filter(Boolean)));
  const impacted = [];
  for (const [target, relations] of relationsByTarget) {
    const targetChanged = changed.has(target) || [...changed].some((path) => target.endsWith("/") && path.startsWith(target));
    if (!targetChanged) continue;
    for (const relation of relations) {
      impacted.push({ relationId: relation.id, target, targetIsDocument: relation.targetIsDocument, document: relation.document, documentFileAlsoChanged: changed.has(relation.document) });
    }
  }
  if (impacted.length > 0) {
    impactEvents.push({
      commit: commit.hash,
      authoredAt: commit.authoredAt,
      relationsImpacted: impacted.length,
      targetOnly: impacted.filter((impact) => !impact.documentFileAlsoChanged).length,
      documentFileCochange: impacted.filter((impact) => impact.documentFileAlsoChanged).length,
      impacts: impacted,
    });
  }
}

const knownCases = [
  {
    id: "native-generator-link",
    document: "docs/content/docs/design/convention-engine.mdx",
    target: "modules/convention/src/main/scala/specrest/convention/dafny/Generator.scala",
    needle: "modules/convention/src/main/scala/specrest/convention/dafny/Generator.scala",
    referenceClass: "native-same-repository-github-link",
  },
  {
    id: "native-naming-link",
    document: "docs/content/docs/design/convention-engine.mdx",
    target: "modules/convention/src/main/scala/specrest/convention/Naming.scala",
    needle: "modules/convention/src/main/scala/specrest/convention/Naming.scala",
    referenceClass: "native-same-repository-github-link",
  },
  {
    id: "inline-route-kind",
    document: "docs/content/docs/targets/python/fastapi/postgres.mdx",
    target: "modules/codegen/src/main/scala/specrest/codegen/RouteKind.scala",
    needle: "modules/codegen/src/main/scala/specrest/codegen/RouteKind.scala",
    referenceClass: "repository-rooted-inline-path",
  },
  {
    id: "inline-pr-template-ir",
    document: ".github/PULL_REQUEST_TEMPLATE.md",
    target: "proofs/isabelle/SpecRest/IR.thy",
    needle: "proofs/isabelle/SpecRest/IR.thy",
    referenceClass: "repository-rooted-inline-path",
  },
  {
    id: "inline-pr-template-soundness",
    document: ".github/PULL_REQUEST_TEMPLATE.md",
    target: "proofs/isabelle/SpecRest/Soundness.thy",
    needle: "proofs/isabelle/SpecRest/Soundness.thy",
    referenceClass: "repository-rooted-inline-path",
  },
];
const knownCaseReplays = knownCases.map((referenceCase) => replayKnownCase(referenceCase, commitMetadata));

const perCommitFanout = impactEvents.map((event) => event.relationsImpacted);
const implementationImpactEvents = impactEvents
  .map((event) => {
    const impacts = event.impacts.filter((impact) => !impact.targetIsDocument);
    return {
      ...event,
      relationsImpacted: impacts.length,
      targetOnly: impacts.filter((impact) => !impact.documentFileAlsoChanged).length,
      documentFileCochange: impacts.filter((impact) => impact.documentFileAlsoChanged).length,
      impacts,
    };
  })
  .filter((event) => event.relationsImpacted > 0);
const implementationFanout = implementationImpactEvents.map((event) => event.relationsImpacted);
const report = {
  schema: "ci-idea/history-replay/v1",
  repository: {
    head: git(["rev-parse", "HEAD"]).trim(),
    shallow: git(["rev-parse", "--is-shallow-repository"]).trim() === "true",
    traversal: "first-parent main history",
  },
  period: { since, through: commits.at(-1)?.authoredAt },
  churn: {
    commits: commits.length,
    documentationTouchingCommits: docCommits.length,
    nonDocumentationTouchingCommits: codeCommits.length,
    bothTouchingCommits: bothCommits.length,
    documentationOnlyCommits: docCommits.length - bothCommits.length,
    nonDocumentationOnlyCommits: codeCommits.length - bothCommits.length,
    mergeCommits: commits.filter((commit) => commit.parents.length > 1).length,
    renameOrCopyRecords: commits.flatMap((commit) => commit.changes).filter((change) => /^[RC]/u.test(change.status)).length,
  },
  survivingCurrentGraphReplay: {
    method: "Replay current resolved explicit relations over first-parent changed-path lists. This is a lower-bound workload estimate with survivorship bias, not historical precision or recall.",
    currentRelations: currentRelations.length,
    uniqueCurrentTargets: relationsByTarget.size,
    commitsWithImpacts: impactEvents.length,
    relationImpactEvents: impactEvents.reduce((sum, event) => sum + event.relationsImpacted, 0),
    unchangedDocumentFileImpactEvents: impactEvents.reduce((sum, event) => sum + event.targetOnly, 0),
    documentFileCochangeEvents: impactEvents.reduce((sum, event) => sum + event.documentFileCochange, 0),
    fanout: {
      p50: percentile(perCommitFanout, 0.5),
      p95: percentile(perCommitFanout, 0.95),
      max: Math.max(0, ...perCommitFanout),
    },
    events: impactEvents,
  },
  survivingCurrentImplementationGraphReplay: {
    method: "Same surviving-current-graph replay, excluding relations whose resolved target is itself a discovered document.",
    currentRelations: currentRelations.filter((relation) => !relation.targetIsDocument).length,
    uniqueCurrentTargets: new Set(currentRelations.filter((relation) => !relation.targetIsDocument).map((relation) => relation.target)).size,
    commitsWithImpacts: implementationImpactEvents.length,
    relationImpactEvents: implementationImpactEvents.reduce((sum, event) => sum + event.relationsImpacted, 0),
    unchangedDocumentFileImpactEvents: implementationImpactEvents.reduce((sum, event) => sum + event.targetOnly, 0),
    documentFileCochangeEvents: implementationImpactEvents.reduce((sum, event) => sum + event.documentFileCochange, 0),
    fanout: {
      p50: percentile(implementationFanout, 0.5),
      p95: percentile(implementationFanout, 0.95),
      max: Math.max(0, ...implementationFanout),
    },
    events: implementationImpactEvents,
  },
  knownBrokenCaseReplay: {
    method: "Replay only commits touching each currently broken document or its old target path, plus HEAD. A blank-line block digest approximates block identity for these known prose/list cases.",
    cases: knownCaseReplays,
    totals: {
      cases: knownCaseReplays.length,
      targetDisappearanceEvents: knownCaseReplays.reduce((sum, item) => sum + item.summary.targetDisappearanceEvents, 0),
      brokenReferenceAddedEvents: knownCaseReplays.reduce((sum, item) => sum + item.summary.brokenReferenceAddedEvents, 0),
      documentFileEditsWhileBroken: knownCaseReplays.reduce((sum, item) => sum + item.summary.documentFileEditsWhileBroken, 0),
      containingBlockEditsWhileBroken: knownCaseReplays.reduce((sum, item) => sum + item.summary.containingBlockEditsWhileBroken, 0),
    },
  },
  commitMetadataCoverage: {
    commitsParsed: commitByHash.size,
    pathsWithChanges: changedPathCounts.size,
  },
};
const serialized = `${JSON.stringify(report, null, 2)}\n`;
if (outputPath) writeFileSync(outputPath, serialized);
else process.stdout.write(serialized);
