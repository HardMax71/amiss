import { spawnSync } from "node:child_process";
import {
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const experiments = dirname(fileURLToPath(import.meta.url));
const root = join(experiments, "..");
const fixture = JSON.parse(
  readFileSync(join(root, "spec", "examples", "gitignore-v1-vectors.json"), "utf8"),
);

const environment = {
  PATH: process.env.PATH,
  HOME: join(experiments, ".empty-home"),
  LC_ALL: "C",
  LANG: "C",
  GIT_CONFIG_NOSYSTEM: "1",
  GIT_CONFIG_SYSTEM: "/dev/null",
  GIT_CONFIG_GLOBAL: "/dev/null",
  GIT_TERMINAL_PROMPT: "0",
};

const git = (cwd, args, expected = [0]) => {
  const result = spawnSync(
    "git",
    ["-c", "core.ignoreCase=false", "-c", "core.precomposeUnicode=false", ...args],
    { cwd, env: environment, encoding: "utf8" },
  );
  if (!expected.includes(result.status)) {
    throw new Error(
      `git ${args.join(" ")} exited ${result.status}: ${result.stderr || result.stdout}`,
    );
  }
  return result;
};

const materialize = (work, item, suffix) => {
  const path = join(work, item.path);
  mkdirSync(dirname(path), { recursive: true });
  if (item.kind === "directory") {
    mkdirSync(path, { recursive: true });
  } else if (item.kind === "regular") {
    writeFileSync(path, item.content_utf8 ?? "", "utf8");
  } else if (item.kind === "symlink") {
    const targetName = `.assure-vector-target-${suffix}`;
    writeFileSync(join(dirname(path), targetName), item.content_utf8 ?? "", "utf8");
    symlinkSync(targetName, path);
  } else {
    throw new Error(`unknown fixture kind ${item.kind}`);
  }
};

let checked = 0;
for (const vector of fixture.cases) {
  for (const [entryIndex, entry] of vector.entries.entries()) {
    const work = mkdtempSync(join(tmpdir(), "assure-gitignore-vector-"));
    try {
      git(work, ["init", "-q"]);
      vector.files.forEach((file, fileIndex) => materialize(work, file, `source-${fileIndex}`));
      for (const file of vector.files.filter((item) => item.tracked)) {
        git(work, ["add", "-f", "--", file.path]);
      }

      const queryPath = join(work, entry.path);
      if (relative(work, queryPath).startsWith("..")) {
        throw new Error(`${vector.id}: entry escaped fixture root`);
      }
      if (!vector.files.some((file) => file.path === entry.path)) {
        materialize(work, entry, `entry-${entryIndex}`);
      }

      let actual;
      if (entry.tracked) {
        git(work, ["add", "-f", "--", entry.path]);
        const result = git(work, ["check-ignore", "-q", "--", entry.path], [0, 1]);
        actual = result.status === 0;
      } else {
        const result = git(
          work,
          ["check-ignore", "--no-index", "-q", "--", entry.path],
          [0, 1],
        );
        actual = result.status === 0;
      }

      if (actual !== entry.ignored) {
        throw new Error(
          `${vector.id} ${JSON.stringify(entry.path)}: expected ignored=${entry.ignored}, got ${actual}`,
        );
      }
      checked += 1;
    } finally {
      rmSync(work, { recursive: true, force: true });
    }
  }
}

const version = spawnSync("git", ["--version"], {
  env: environment,
  encoding: "utf8",
});
if (version.status !== 0) throw new Error("could not read Git version");
process.stdout.write(
  `differential-checked ${fixture.cases.length} gitignore-v1 cases / ${checked} entries with ${version.stdout.trim()}\n`,
);
