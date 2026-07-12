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

function git(args) {
  const result = spawnSync("git", args, { cwd: root, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 });
  if (result.status !== 0) throw new Error(`git ${args.join(" ")} failed: ${result.stderr}`);
  return result.stdout.trim();
}

function eventBlock(lines) {
  const start = lines.findIndex((line) => /^on:\s*/u.test(line));
  if (start < 0) return [];
  const first = lines[start];
  if (!/^on:\s*$/u.test(first)) return [first];
  const block = [first];
  for (let index = start + 1; index < lines.length; index += 1) {
    if (/^[^\s#]/u.test(lines[index]) && lines[index].trim() !== "") break;
    block.push(lines[index]);
  }
  return block;
}

function eventsFromBlock(block) {
  if (block.length === 0) return [];
  const inline = /^on:\s*\[([^\]]+)\]/u.exec(block[0]);
  if (inline) return inline[1].split(",").map((value) => value.trim());
  const scalar = /^on:\s*([^\s#]+)\s*$/u.exec(block[0]);
  if (scalar) return [scalar[1]];
  return block
    .map((line) => /^ {2}([A-Za-z_]+):/u.exec(line)?.[1])
    .filter(Boolean);
}

function actionPin(reference) {
  if (reference.startsWith("./")) return "local";
  const at = reference.lastIndexOf("@");
  if (at < 0) return "unpinned";
  const pin = reference.slice(at + 1);
  if (/^[0-9a-f]{40,64}$/u.test(pin)) return "full-commit-sha";
  if (/^sha256:[0-9a-f]{64}$/u.test(pin)) return "digest";
  return "mutable-ref-or-short-sha";
}

function auditWorkflow(path) {
  const source = readFileSync(resolve(root, path), "utf8");
  const lines = source.split(/\r?\n/u);
  const block = eventBlock(lines);
  const checkout = [];
  const actions = [];
  for (let index = 0; index < lines.length; index += 1) {
    const match = /\buses:\s*([^\s#]+)/u.exec(lines[index]);
    if (!match) continue;
    const reference = match[1];
    actions.push({ line: index + 1, reference, pinClass: actionPin(reference) });
    if (!reference.startsWith("actions/checkout@")) continue;
    let fetchDepth;
    let persistCredentials;
    for (let next = index + 1; next < lines.length; next += 1) {
      if (/^\s*-\s+(?:name:|uses:|run:)/u.test(lines[next])) break;
      const depth = /fetch-depth:\s*([^\s#]+)/u.exec(lines[next]);
      if (depth) fetchDepth = depth[1];
      const persist = /persist-credentials:\s*([^\s#]+)/u.exec(lines[next]);
      if (persist) persistCredentials = persist[1];
    }
    checkout.push({ line: index + 1, reference, pinClass: actionPin(reference), fetchDepth: fetchDepth ?? "provider-default", persistCredentials: persistCredentials ?? "provider-default" });
  }
  const permissionsLine = lines.findIndex((line) => /^permissions:/u.test(line));
  let topLevelContents = "unspecified";
  if (permissionsLine >= 0) {
    if (/permissions:\s*\{\s*\}/u.test(lines[permissionsLine])) topLevelContents = "none-explicit";
    for (let index = permissionsLine + 1; index < lines.length; index += 1) {
      if (/^[^\s#]/u.test(lines[index]) && lines[index].trim() !== "") break;
      const contents = /^ {2}contents:\s*([^\s#]+)/u.exec(lines[index]);
      if (contents) topLevelContents = contents[1];
    }
  }
  return {
    path,
    events: eventsFromBlock(block),
    eventBlock: block,
    hasPathFilter: block.some((line) => /^ {4}paths(?:-ignore)?:/u.test(line)),
    topLevelContentsPermission: topLevelContents,
    checkout,
    actions,
  };
}

const workflowPaths = git(["ls-files", ".github/workflows/*.yml", ".github/workflows/*.yaml"]).split("\n").filter(Boolean);
const workflows = workflowPaths.map(auditWorkflow);
const checkoutSteps = workflows.flatMap((workflow) => workflow.checkout.map((step) => ({ workflow: workflow.path, ...step })));
const actionSteps = workflows.flatMap((workflow) => workflow.actions.map((step) => ({ workflow: workflow.path, ...step })));
const eventCounts = {};
for (const event of workflows.flatMap((workflow) => workflow.events)) eventCounts[event] = (eventCounts[event] ?? 0) + 1;
let parentAvailable = true;
try {
  git(["cat-file", "-e", "HEAD^"]);
} catch {
  parentAvailable = false;
}

const report = {
  schema: "ci-idea/workflow-audit/v1",
  repository: {
    head: git(["rev-parse", "HEAD"]),
    branch: git(["branch", "--show-current"]),
    shallow: git(["rev-parse", "--is-shallow-repository"]) === "true",
    headParentAvailable: parentAvailable,
    mergeQueueConfiguration: "unknown-from-checkout",
  },
  summary: {
    workflows: workflows.length,
    eventCounts: Object.fromEntries(Object.entries(eventCounts).sort(([a], [b]) => a.localeCompare(b))),
    workflowsWithPathFilters: workflows.filter((workflow) => workflow.hasPathFilter).length,
    workflowsWithMergeGroup: workflows.filter((workflow) => workflow.events.includes("merge_group")).length,
    workflowsWithPullRequest: workflows.filter((workflow) => workflow.events.includes("pull_request")).length,
    checkoutSteps: checkoutSteps.length,
    checkoutDefaultDepthSteps: checkoutSteps.filter((step) => step.fetchDepth === "provider-default").length,
    checkoutFullHistorySteps: checkoutSteps.filter((step) => step.fetchDepth === "0").length,
    checkoutExplicitDepthTwoSteps: checkoutSteps.filter((step) => step.fetchDepth === "2").length,
    checkoutPersistCredentialsDisabledSteps: checkoutSteps.filter((step) => step.persistCredentials === "false").length,
    checkoutImmutablePins: checkoutSteps.filter((step) => step.pinClass === "full-commit-sha" || step.pinClass === "digest").length,
    externalActionMutablePins: actionSteps.filter((step) => step.pinClass === "mutable-ref-or-short-sha" || step.pinClass === "unpinned").length,
    topLevelContentsRead: workflows.filter((workflow) => workflow.topLevelContentsPermission === "read").length,
    topLevelContentsWrite: workflows.filter((workflow) => workflow.topLevelContentsPermission === "write").length,
  },
  readiness: {
    existingAlwaysRunPullRequestAndMergeGroupLane: workflows.some((workflow) => workflow.events.includes("pull_request") && workflow.events.includes("merge_group") && !workflow.hasPathFilter),
    candidateBaseCanBeReadInThisLocalCheckout: parentAvailable,
    exactProviderBaseAndCandidateContractPresent: false,
    requiredNewWorkflowProperties: [
      "pull_request, merge_group, and main push triggers",
      "no path filters",
      "contents: read and no write permissions",
      "explicit provider base and candidate SHAs",
      "base fetch or a documented unattributed mode",
      "persist-credentials: false",
      "immutable action and binary pins",
      "bounded timeout and output",
    ],
  },
  linksWorkflow: workflows.find((workflow) => workflow.path === ".github/workflows/links.yml"),
  checkoutSteps,
  workflows,
};
report.readiness.candidateBaseCanBeReadInThisLocalCheckout = report.repository.headParentAvailable && !report.repository.shallow;
const serialized = `${JSON.stringify(report, null, 2)}\n`;
if (outputPath) writeFileSync(outputPath, serialized);
else process.stdout.write(serialized);
