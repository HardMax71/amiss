import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, extname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

// Standalone re-implementation of github-slugger's algorithm (lowercase, strip
// punctuation, spaces to hyphens, duplicate slugs suffixed -1, -2, ...) so the
// dossier validates without the original host repository's node_modules.
class GithubSlugger {
  constructor() {
    this.occurrences = new Map();
  }
  slug(value) {
    let result = String(value)
      .toLowerCase()
      .replace(/[^\p{L}\p{N}\p{M}‌‍ _-]/gu, "")
      .replace(/ /gu, "-");
    const original = result;
    while (this.occurrences.has(result)) {
      this.occurrences.set(original, (this.occurrences.get(original) ?? 0) + 1);
      result = `${original}-${this.occurrences.get(original)}`;
    }
    this.occurrences.set(result, 0);
    return result;
  }
}

const dossier = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const failures = [];

const files = [];
const markdownAnchors = new Map();
const visit = (directory) => {
  for (const name of readdirSync(directory).sort()) {
    const path = join(directory, name);
    if (statSync(path).isDirectory()) visit(path);
    else files.push(path);
  }
};
visit(dossier);

const relativeName = (path) => path.slice(dossier.length + 1);

for (const path of files.filter((candidate) => extname(candidate) === ".md")) {
  const slugger = new GithubSlugger();
  const anchors = new Set();
  let fence = null;
  for (const line of readFileSync(path, "utf8").split(/\n/u)) {
    const marker = line.match(/^ {0,3}(`{3,}|~{3,})(.*)$/u);
    if (fence === null && marker) {
      fence = { marker: marker[1][0], length: marker[1].length };
      continue;
    }
    if (fence !== null) {
      const closing = line.match(/^ {0,3}(`{3,}|~{3,})\s*$/u);
      if (closing && closing[1][0] === fence.marker && closing[1].length >= fence.length) fence = null;
      continue;
    }
    const heading = line.match(/^ {0,3}#{1,6}\s+(.+?)\s*#*\s*$/u);
    if (heading) anchors.add(slugger.slug(heading[1]));
  }
  markdownAnchors.set(resolve(path), anchors);
}

const checkTarget = (source, raw, line) => {
  let target = raw.trim();
  if (target.startsWith("<") && target.endsWith(">")) target = target.slice(1, -1);
  const hash = target.indexOf("#");
  const rawFragment = hash >= 0 ? target.slice(hash + 1) : null;
  const pathAndQuery = hash >= 0 ? target.slice(0, hash) : target;
  const rawPath = pathAndQuery.split("?", 1)[0];
  if (rawPath !== "" && !rawPath.startsWith("./") && !rawPath.startsWith("../")) return;
  if (rawPath === "" && rawFragment === null) return;
  let resolved = resolve(source);
  try {
    if (rawPath !== "") resolved = resolve(dirname(source), decodeURIComponent(rawPath));
  } catch {
    failures.push(`${relativeName(source)}:${line}: invalid percent encoding in ${raw}`);
    return;
  }
  if (!existsSync(resolved)) {
    failures.push(`${relativeName(source)}:${line}: missing relative target ${raw}`);
    return;
  }
  if (rawFragment !== null && extname(resolved) === ".md") {
    let fragment;
    try {
      fragment = decodeURIComponent(rawFragment);
    } catch {
      failures.push(`${relativeName(source)}:${line}: invalid fragment encoding in ${raw}`);
      return;
    }
    if (fragment !== "" && !markdownAnchors.get(resolved)?.has(fragment)) {
      failures.push(`${relativeName(source)}:${line}: missing Markdown anchor ${raw}`);
    }
  }
};

for (const path of files) {
  const extension = extname(path);
  const content = readFileSync(path, "utf8");

  if (extension === ".json") {
    try {
      JSON.parse(content);
    } catch (error) {
      failures.push(`${relativeName(path)}: invalid JSON: ${error.message}`);
    }
  }

  if (extension === ".jsonl") {
    content.split(/\r?\n/u).forEach((line, index) => {
      if (line === "") return;
      try {
        JSON.parse(line);
      } catch (error) {
        failures.push(`${relativeName(path)}:${index + 1}: invalid JSONL: ${error.message}`);
      }
    });
  }

  if (extension !== ".md") continue;

  const lines = content.split(/\n/u);
  let fence = null;
  const prose = [];

  lines.forEach((line, index) => {
    const number = index + 1;
    if (/[ \t]\r?$/u.test(line)) failures.push(`${relativeName(path)}:${number}: trailing whitespace`);

    const opening = line.match(/^ {0,3}(`{3,}|~{3,})(.*)$/u);
    if (fence === null && opening) {
      if (opening[2].trim() === "") failures.push(`${relativeName(path)}:${number}: untyped code fence`);
      fence = { marker: opening[1][0], length: opening[1].length, line: number };
      prose.push("");
      return;
    }

    if (fence !== null) {
      const closing = line.match(/^ {0,3}(`{3,}|~{3,})\s*$/u);
      if (closing && closing[1][0] === fence.marker && closing[1].length >= fence.length) fence = null;
      prose.push("");
      return;
    }

    prose.push(line);
  });

  if (fence !== null) failures.push(`${relativeName(path)}:${fence.line}: unclosed code fence`);

  const visible = prose.join("\n");
  const inline = /\]\((<[^>]+>|[^)\s]+)(?:\s+[^)]*)?\)/gu;
  const definition = /^\s*\[[^\]]+\]:\s*(<[^>]+>|\S+)/gmu;

  for (const matcher of [inline, definition]) {
    for (const match of visible.matchAll(matcher)) {
      const line = visible.slice(0, match.index).split("\n").length;
      checkTarget(path, match[1], line);
    }
  }
}

if (failures.length > 0) {
  process.stderr.write(`${failures.join("\n")}\n`);
  process.exitCode = 1;
} else {
  process.stdout.write(`validated ${files.length} dossier files\n`);
}
