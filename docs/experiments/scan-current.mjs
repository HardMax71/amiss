import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { writeFileSync } from "node:fs";
import { dirname } from "node:path";
import { extname } from "node:path";
import { relative } from "node:path";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { TextDecoder } from "node:util";
import { unified } from "../../docs/node_modules/unified/index.js";
import remarkGfm from "../../docs/node_modules/remark-gfm/index.js";
import remarkMath from "../../docs/node_modules/remark-math/index.js";
import remarkMdx from "../../docs/node_modules/remark-mdx/index.js";
import remarkParse from "../../docs/node_modules/remark-parse/index.js";

const experimentDir = dirname(fileURLToPath(import.meta.url));
const root = resolve(experimentDir, "../..");
const outputArg = process.argv.indexOf("--out");
const outputPath = outputArg >= 0 ? resolve(root, process.argv[outputArg + 1]) : undefined;
const decoder = new TextDecoder("utf-8", { fatal: true });
const proseExtensions = new Set([".md", ".mdx", ".markdown", ".rst", ".adoc", ".txt", ".org"]);
const excludedSegments = new Set(["node_modules", "vendor", "third_party", "dist", "build"]);
const knownRepoRoots = new Set([
  ".github",
  "deploy",
  "docs",
  "fixtures",
  "modules",
  "project",
  "proofs",
]);
const candidateFileExtensions = new Set([
  ".adoc",
  ".als",
  ".conf",
  ".csv",
  ".g4",
  ".go",
  ".hbs",
  ".html",
  ".java",
  ".js",
  ".json",
  ".md",
  ".mdx",
  ".mjs",
  ".org",
  ".py",
  ".rs",
  ".rst",
  ".scala",
  ".sh",
  ".smt2",
  ".spec",
  ".svg",
  ".thy",
  ".toml",
  ".ts",
  ".tsx",
  ".txt",
  ".yaml",
  ".yml",
]);
const blockTypes = new Set(["paragraph", "listItem", "table", "code", "html"]);

function git(args, options = {}) {
  const result = spawnSync("git", args, {
    cwd: root,
    encoding: options.encoding ?? "utf8",
    maxBuffer: 128 * 1024 * 1024,
  });
  if (result.status !== 0) {
    throw new Error(`git ${args.join(" ")} failed: ${String(result.stderr)}`);
  }
  return result.stdout;
}

function sha256(value) {
  return createHash("sha256").update(value).digest("hex");
}

function normalizePath(value) {
  const parts = value.replaceAll("\\", "/").split("/");
  const normalized = [];
  for (const part of parts) {
    if (part === "" || part === ".") continue;
    if (part === "..") {
      if (normalized.length === 0) return undefined;
      normalized.pop();
    } else {
      normalized.push(part);
    }
  }
  return normalized.join("/");
}

function stripTargetSuffix(value) {
  const query = value.indexOf("?");
  const fragment = value.indexOf("#");
  const cut = [query, fragment].filter((index) => index >= 0).sort((a, b) => a - b)[0];
  return cut === undefined ? value : value.slice(0, cut);
}

function stripLineSuffix(value) {
  return value.replace(/:(?:L)?\d+(?:-(?:L)?\d+)?$/u, "");
}

function isExcluded(path) {
  const segments = path.split("/");
  return segments.some((segment) => excludedSegments.has(segment)) || path === "assure.lock";
}

function isDocNamed(path) {
  const name = path.split("/").at(-1) ?? "";
  return /^(?:README|CONTRIBUTING|CHANGELOG)(?:[._-].*)?$/iu.test(name);
}

function broadDiscoveryReason(path) {
  if (isExcluded(path)) return "built-in-exclusion";
  const extension = extname(path).toLowerCase();
  if (proseExtensions.has(extension)) return `prose-extension:${extension}`;
  if (isDocNamed(path)) return "doc-name";
  const name = path.split("/").at(-1)?.toLowerCase();
  if ([".cursorrules", "llms.txt", "agents.md", "claude.md"].includes(name)) return "agent-file";
  return "not-discovered";
}

function recommendedDiscoveryReason(path) {
  if (isExcluded(path)) return "built-in-exclusion";
  const extension = extname(path).toLowerCase();
  if (extension === ".md" || extension === ".mdx") return `structured:${extension}`;
  const name = path.split("/").at(-1)?.toLowerCase();
  if (name === ".cursorrules" || name === "llms.txt") return "explicit-plain-agent-file";
  if (!path.includes("/") && isDocNamed(path)) return "root-doc-name";
  return "not-discovered";
}

function parseTracked() {
  const raw = git(["ls-files", "-z", "--stage"], { encoding: "buffer" });
  const records = raw.toString("utf8").split("\0").filter(Boolean).map((record) => {
    const match = /^(\d+) ([0-9a-f]+) (\d)\t(.*)$/u.exec(record);
    if (!match) throw new Error(`cannot parse git index record: ${record}`);
    return { mode: match[1], oid: match[2], stage: Number(match[3]), path: match[4] };
  });
  if (records.some((record) => record.stage !== 0)) throw new Error("unmerged index stages present");
  return records;
}

function directorySet(paths) {
  const directories = new Set();
  for (const path of paths) {
    const parts = path.split("/");
    for (let length = 1; length < parts.length; length += 1) {
      directories.add(parts.slice(0, length).join("/"));
    }
  }
  return directories;
}

function basenameIndex(paths) {
  const index = new Map();
  for (const path of paths) {
    const name = path.split("/").at(-1);
    const existing = index.get(name) ?? [];
    existing.push(path);
    index.set(name, existing);
  }
  return index;
}

function decodeUtf8(path) {
  const bytes = readFileSync(resolve(root, path));
  try {
    return { bytes, text: decoder.decode(bytes), valid: true };
  } catch {
    return { bytes, valid: false };
  }
}

function position(node) {
  if (!node.position) return undefined;
  return {
    start: {
      line: node.position.start.line,
      column: node.position.start.column,
      offset: node.position.start.offset,
    },
    end: {
      line: node.position.end.line,
      column: node.position.end.column,
      offset: node.position.end.offset,
    },
  };
}

function blockFor(ancestors, node, text) {
  const block = [...ancestors, node].reverse().find((candidate) => blockTypes.has(candidate.type)) ?? node;
  const start = block.position?.start.offset;
  const end = block.position?.end.offset;
  const raw = start === undefined || end === undefined ? "" : text.slice(start, end).replaceAll("\r\n", "\n").replaceAll("\r", "\n");
  return {
    type: block.type,
    lineStart: block.position?.start.line,
    lineEnd: block.position?.end.line,
    digest: `sha256:${sha256(raw)}`,
  };
}

function walk(node, ancestors, visit) {
  visit(node, ancestors);
  if (!Array.isArray(node.children)) return;
  for (const child of node.children) walk(child, [...ancestors, node], visit);
}

function parseMarkdown(path, text) {
  const processor = unified().use(remarkParse).use(remarkMath).use(remarkGfm);
  if (path.toLowerCase().endsWith(".mdx")) processor.use(remarkMdx);
  return processor.parse(text);
}

function extractFenceMetadata(meta) {
  if (!meta) return [];
  const results = [];
  const pattern = /(?:^|\s)(file|src)=(?:"([^"]+)"|'([^']+)'|([^\s]+))/gu;
  for (const match of meta.matchAll(pattern)) {
    results.push({ attribute: match[1], target: match[2] ?? match[3] ?? match[4] });
  }
  return results;
}

function extractHtmlAttributes(value) {
  const results = [];
  const pattern = /\b(href|src)\s*=\s*(?:"([^"]+)"|'([^']+)')/giu;
  for (const match of value.matchAll(pattern)) {
    results.push({ attribute: match[1].toLowerCase(), target: match[2] ?? match[3] });
  }
  return results;
}

function extractMdxAttributes(node) {
  if (node.type !== "mdxJsxFlowElement" && node.type !== "mdxJsxTextElement") return [];
  return (node.attributes ?? [])
    .filter((attribute) => attribute.type === "mdxJsxAttribute")
    .filter((attribute) => attribute.name === "href" || attribute.name === "src")
    .map((attribute) => ({
      attribute: attribute.name,
      target: typeof attribute.value === "string" ? attribute.value : undefined,
      expression: typeof attribute.value === "object",
    }));
}

function contextLine(text, line) {
  if (!line) return "";
  return text.split(/\r?\n/u)[line - 1]?.trim() ?? "";
}

function extractDocument(path, text) {
  const tree = parseMarkdown(path, text);
  const explicit = [];
  const inlineCode = [];
  walk(tree, [], (node, ancestors) => {
    const common = {
      document: path,
      position: position(node),
      block: blockFor(ancestors, node, text),
      context: contextLine(text, node.position?.start.line),
    };
    if (node.type === "link" || node.type === "image" || node.type === "definition") {
      explicit.push({ ...common, sourceKind: node.type, target: node.url });
    }
    if (node.type === "html") {
      for (const found of extractHtmlAttributes(node.value)) {
        explicit.push({ ...common, sourceKind: `html-${found.attribute}`, target: found.target });
      }
    }
    if (node.type === "code") {
      for (const found of extractFenceMetadata(node.meta)) {
        explicit.push({ ...common, sourceKind: `fence-${found.attribute}`, target: found.target });
      }
    }
    for (const found of extractMdxAttributes(node)) {
      explicit.push({
        ...common,
        sourceKind: `mdx-${found.attribute}`,
        target: found.target,
        expression: found.expression,
      });
    }
    if (node.type === "inlineCode") inlineCode.push({ ...common, value: node.value });
  });
  return { explicit, inlineCode };
}

function candidatePathsForSiteRoute(raw) {
  const route = raw.replace(/^\/+/, "").replace(/\/$/u, "");
  if (route === "") return ["docs/content/docs/index.mdx", "docs/content/docs/index.md"];
  return [
    `docs/public/${route}`,
    `docs/content/docs/${route}.mdx`,
    `docs/content/docs/${route}.md`,
    `docs/content/docs/${route}/index.mdx`,
    `docs/content/docs/${route}/index.md`,
  ];
}

function candidatePathsForRelative(document, raw) {
  const decoded = decodeURIComponent(raw);
  const relativeCandidate = normalizePath(`${dirname(document)}/${decoded}`);
  const results = relativeCandidate ? [relativeCandidate] : [];
  for (const suffix of [".mdx", ".md", "/index.mdx", "/index.md"]) {
    if (relativeCandidate) results.push(`${relativeCandidate}${suffix}`);
  }
  return [...new Set(results)];
}

function resolveCandidates(candidates, trackedPaths, directories) {
  const matches = candidates.filter((candidate) => trackedPaths.has(candidate) || directories.has(candidate));
  return [...new Set(matches)];
}

function githubReference(rawTarget, trackedPaths, directories) {
  let url;
  try {
    url = new URL(rawTarget);
  } catch {
    return undefined;
  }
  if (url.hostname.toLowerCase() !== "github.com") return undefined;
  const parts = url.pathname.split("/").filter(Boolean).map((part) => decodeURIComponent(part));
  if (parts.length < 5) return undefined;
  const [owner, repository, kind, ref, ...pathParts] = parts;
  if (owner.toLowerCase() !== "hardmax71" || repository.toLowerCase() !== "spec_to_rest") return undefined;
  if (kind !== "blob" && kind !== "tree") return undefined;
  const path = normalizePath(pathParts.join("/"));
  const currentScope = ref === "main" || ref === "HEAD";
  if (!path) return { classification: "same-repo-github", status: "invalid", ref, kind };
  if (currentScope) {
    const matches = resolveCandidates([path], trackedPaths, directories);
    return {
      classification: "same-repo-github",
      status: matches.length === 1 ? "resolved" : "missing",
      ref,
      kind,
      normalizedPath: path,
      matches,
    };
  }
  let exists = false;
  try {
    git(["cat-file", "-e", `${ref}:${path}`]);
    exists = true;
  } catch {
    exists = false;
  }
  return {
    classification: "same-repo-github-pinned-or-foreign-ref",
    status: exists ? "resolved" : "scope-unavailable-or-missing",
    ref,
    kind,
    normalizedPath: path,
    matches: exists ? [`${ref}:${path}`] : [],
  };
}

function resolveExplicit(record, trackedPaths, directories) {
  if (record.expression) return { classification: "mdx-expression", status: "unsupported" };
  if (typeof record.target !== "string" || record.target.length === 0) {
    return { classification: "empty-target", status: "invalid" };
  }
  const sameRepo = githubReference(record.target, trackedPaths, directories);
  if (sameRepo) return sameRepo;
  if (record.target.startsWith("#")) return { classification: "same-document-anchor", status: "not-file-reference" };
  if (/^[A-Za-z][A-Za-z0-9+.-]*:/u.test(record.target) || record.target.startsWith("//")) {
    return { classification: "external", status: "not-evaluated" };
  }
  let stripped;
  try {
    stripped = stripTargetSuffix(record.target);
    if (stripped.startsWith("<") && stripped.endsWith(">")) stripped = stripped.slice(1, -1);
    stripped = stripLineSuffix(stripped);
    let candidates;
    if (stripped.startsWith("/")) {
      candidates = candidatePathsForSiteRoute(stripped);
    } else if (record.sourceKind === "fence-file" || record.sourceKind === "fence-src") {
      const repositoryPath = normalizePath(decodeURIComponent(stripped));
      candidates = repositoryPath ? [repositoryPath] : [];
    } else {
      candidates = candidatePathsForRelative(record.document, stripped);
    }
    const matches = resolveCandidates(candidates, trackedPaths, directories);
    const exactMatch = candidates[0] && (trackedPaths.has(candidates[0]) || directories.has(candidates[0])) ? [candidates[0]] : [];
    const effectiveMatches = exactMatch.length > 0 ? exactMatch : matches;
    return {
      classification: stripped.startsWith("/")
        ? "site-root-local"
        : record.sourceKind === "fence-file" || record.sourceKind === "fence-src"
          ? "repository-rooted-fence"
          : "document-relative-local",
      status: effectiveMatches.length === 0 ? "missing" : effectiveMatches.length === 1 ? "resolved" : "ambiguous",
      normalizedPath: normalizePath(stripped),
      matches: effectiveMatches,
      attempted: candidates,
    };
  } catch {
    return { classification: "invalid-local-target", status: "invalid" };
  }
}

function extractPathTokens(value) {
  const matches = new Set();
  const pattern = /(?:^|[\s"'`(])((?:(?:\.github|deploy|docs|fixtures|modules|project|proofs)\/|\.\.?\/)[A-Za-z0-9_./{}<>*$…-]+(?:#[^\s,;)]+|:(?:L)?\d+(?:-(?:L)?\d+)?)?)/gu;
  for (const match of value.matchAll(pattern)) matches.add(match[1]);
  if (matches.size === 0 && /^[A-Za-z0-9_.-]+\.[A-Za-z0-9]+$/u.test(value) && candidateFileExtensions.has(extname(value).toLowerCase())) matches.add(value);
  return [...matches];
}

function resolveInline(document, raw, trackedPaths, directories, byBasename) {
  const placeholder = /[<>{}*$…]|\.\.\./u.test(raw);
  let value = stripLineSuffix(stripTargetSuffix(raw.replaceAll("\\", "/")));
  value = value.replace(/^\.\//u, "");
  const first = value.split("/")[0];
  const repoRooted = knownRepoRoots.has(first);
  const basenameMatches = value.includes("/") ? [] : (byBasename.get(value) ?? []);
  const candidates = repoRooted
    ? [normalizePath(value)].filter(Boolean)
    : basenameMatches.length > 0
      ? basenameMatches
      : [normalizePath(`${dirname(document)}/${value}`), normalizePath(value)].filter(Boolean);
  const matches = resolveCandidates(candidates, trackedPaths, directories);
  return {
    classification: placeholder ? "placeholder" : repoRooted ? "repository-rooted-inline" : "ambiguous-inline",
    status: placeholder ? "non-binding" : matches.length === 0 ? "missing" : matches.length === 1 ? "resolved" : "ambiguous",
    normalizedPath: normalizePath(value),
    attempted: candidates,
    matches,
  };
}

function countBy(items, key) {
  const result = {};
  for (const item of items) {
    const value = typeof key === "function" ? key(item) : item[key];
    result[value] = (result[value] ?? 0) + 1;
  }
  return Object.fromEntries(Object.entries(result).sort(([a], [b]) => a.localeCompare(b)));
}

const started = process.hrtime.bigint();
const tracked = parseTracked();
const trackedPaths = new Set(tracked.map((record) => record.path));
const directories = directorySet(trackedPaths);
const byBasename = basenameIndex(trackedPaths);
const broad = tracked.map((record) => ({ ...record, reason: broadDiscoveryReason(record.path) })).filter((record) => record.reason !== "not-discovered" && record.reason !== "built-in-exclusion");
const recommended = tracked.map((record) => ({ ...record, reason: recommendedDiscoveryReason(record.path) })).filter((record) => record.reason !== "not-discovered" && record.reason !== "built-in-exclusion");
const inventoryFiles = [];
const explicit = [];
const inline = [];

for (const record of broad) {
  const decoded = decodeUtf8(record.path);
  inventoryFiles.push({
    path: record.path,
    mode: record.mode,
    oid: record.oid,
    discoveryReason: record.reason,
    recommendedReason: recommendedDiscoveryReason(record.path),
    bytes: decoded.bytes.length,
    lines: decoded.valid ? decoded.text.split(/\r?\n/u).length : undefined,
    validUtf8: decoded.valid,
  });
}

for (const record of recommended) {
  const decoded = decodeUtf8(record.path);
  if (!decoded.valid) continue;
  if (!record.path.toLowerCase().endsWith(".md") && !record.path.toLowerCase().endsWith(".mdx")) continue;
  let extracted;
  try {
    extracted = extractDocument(record.path, decoded.text);
  } catch (error) {
    explicit.push({
      document: record.path,
      sourceKind: "parser-error",
      resolution: { classification: "parser-error", status: "error", message: String(error) },
    });
    continue;
  }
  for (const reference of extracted.explicit) {
    explicit.push({ ...reference, resolution: resolveExplicit(reference, trackedPaths, directories) });
  }
  for (const code of extracted.inlineCode) {
    for (const token of extractPathTokens(code.value)) {
      inline.push({ ...code, token, resolution: resolveInline(record.path, token, trackedPaths, directories, byBasename) });
    }
  }
}

explicit.sort((a, b) => a.document.localeCompare(b.document) || (a.position?.start.line ?? 0) - (b.position?.start.line ?? 0) || String(a.target).localeCompare(String(b.target)));
inline.sort((a, b) => a.document.localeCompare(b.document) || (a.position?.start.line ?? 0) - (b.position?.start.line ?? 0) || a.token.localeCompare(b.token));
inventoryFiles.sort((a, b) => a.path.localeCompare(b.path));
const explicitFileReferences = explicit.filter((item) => ["same-repo-github", "same-repo-github-pinned-or-foreign-ref", "site-root-local", "document-relative-local", "repository-rooted-fence"].includes(item.resolution.classification));
const inlineBindingCandidates = inline.filter((item) => item.resolution.status !== "non-binding");
const recommendedPaths = new Set(recommended.map((record) => record.path));
const documentsWithExplicit = new Set(explicitFileReferences.map((item) => item.document));
const end = process.hrtime.bigint();
const report = {
  schema: "ci-idea/current-scan/v1",
  repository: {
    head: git(["rev-parse", "HEAD"]).trim(),
    branch: git(["branch", "--show-current"]).trim(),
    objectFormat: git(["rev-parse", "--show-object-format"]).trim(),
    shallow: git(["rev-parse", "--is-shallow-repository"]).trim() === "true",
    trackedEntries: tracked.length,
  },
  discovery: {
    broadDossierScope: {
      count: broad.length,
      bytes: inventoryFiles.reduce((sum, file) => sum + file.bytes, 0),
      lines: inventoryFiles.reduce((sum, file) => sum + (file.lines ?? 0), 0),
      reasons: countBy(broad, "reason"),
    },
    recommendedScannerScope: {
      count: recommended.length,
      reasons: countBy(recommended, "reason"),
    },
    broadOnly: inventoryFiles.filter((file) => !recommendedPaths.has(file.path)).map((file) => file.path),
    invalidUtf8: inventoryFiles.filter((file) => !file.validUtf8).map((file) => file.path),
    files: inventoryFiles,
  },
  references: {
    allExtractedCount: explicit.length,
    fileReferenceCount: explicitFileReferences.length,
    bySourceKind: countBy(explicit, "sourceKind"),
    byClassification: countBy(explicit, (item) => item.resolution.classification),
    byStatus: countBy(explicit, (item) => item.resolution.status),
    sameRepoGithubCount: explicit.filter((item) => item.resolution.classification.startsWith("same-repo-github")).length,
    missingExplicitCount: explicitFileReferences.filter((item) => item.resolution.status === "missing").length,
    documentsWithExplicit: documentsWithExplicit.size,
    documentsWithoutExplicit: recommended.length - documentsWithExplicit.size,
    records: explicit,
  },
  inlinePaths: {
    extractedTokenCount: inline.length,
    bindingCandidateCount: inlineBindingCandidates.length,
    byClassification: countBy(inline, (item) => item.resolution.classification),
    byStatus: countBy(inline, (item) => item.resolution.status),
    missingCount: inlineBindingCandidates.filter((item) => item.resolution.status === "missing").length,
    records: inline,
  },
  measurement: {
    elapsedMilliseconds: Number(end - started) / 1_000_000,
    maxRssKiB: process.resourceUsage().maxRSS,
  },
};
const serialized = `${JSON.stringify(report, null, 2)}\n`;
if (outputPath) writeFileSync(outputPath, serialized);
else process.stdout.write(serialized);
