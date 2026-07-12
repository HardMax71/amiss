import { readFileSync } from "node:fs";
import { writeFileSync } from "node:fs";
import { dirname } from "node:path";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { compile } from "../../docs/node_modules/@mdx-js/mdx/index.js";
import { toHast } from "../../docs/node_modules/mdast-util-to-hast/index.js";
import { unified } from "../../docs/node_modules/unified/index.js";
import remarkGfm from "../../docs/node_modules/remark-gfm/index.js";
import remarkMath from "../../docs/node_modules/remark-math/index.js";
import remarkMdx from "../../docs/node_modules/remark-mdx/index.js";
import remarkParse from "../../docs/node_modules/remark-parse/index.js";

const experimentDir = dirname(fileURLToPath(import.meta.url));
const root = resolve(experimentDir, "../..");
const outputArg = process.argv.indexOf("--out");
const outputPath = outputArg >= 0 ? resolve(root, process.argv[outputArg + 1]) : undefined;

const fixtures = {
  repeatedDefinitions: `# First

[assure]: modules/a/Foo.scala#Foo

[first][assure]

# Second

[assure]: modules/b/Bar.scala#Bar

[second][assure]
`,
  uniqueDefinitions: `# First

[assure:first]: assure:v1?target=modules%2Fa%2FFoo.scala%23Foo

[first][assure:first]

# Second

[assure:second]: assure:v1?target=modules%2Fb%2FBar.scala%23Bar

[second][assure:second]
`,
  frontmatter: `---
title: Matrix fixture
description: Frontmatter must not become a claim block.
---

# Section

[assure:section]: assure:v1?target=modules%2Fa%2FFoo.scala
`,
  jsxAndEsm: `import Component from './component.js'

export const marker = (globalThis.__ASSURE_SOURCE_WAS_EVALUATED__ = true)

# JSX

<Component href="modules/a/Foo.scala" src={globalThis.__ASSURE_JSX_WAS_EVALUATED__ = true}>
  [assure:not-a-definition]: modules/inside-jsx/Fake.scala
</Component>

[assure:jsx]: assure:v1?target=modules%2Fa%2FFoo.scala
`,
  mathBraces: `---
title: Math
---

# Formula

$$
\\underbrace{2N}_{\\text{enabled}} + \\underbrace{T}_{\\text{temporal}}
$$

[assure:math]: assure:v1?target=modules%2Fverify%2FConfig.scala
`,
  crlfUnicode: "# Café\r\n\r\n[assure:café]: assure:v1?target=docs%2Fcaf%C3%A9.md\r\n",
};

const profiles = {
  commonmark(processor) {
    return processor.use(remarkParse);
  },
  gfm(processor) {
    return processor.use(remarkParse).use(remarkGfm);
  },
  mdxBasic(processor) {
    return processor.use(remarkParse).use(remarkGfm).use(remarkMdx);
  },
  mdxWithSiteMath(processor) {
    return processor.use(remarkParse).use(remarkMath).use(remarkGfm).use(remarkMdx);
  },
};

function walk(node, visit) {
  visit(node);
  if (!Array.isArray(node.children)) return;
  for (const child of node.children) walk(child, visit);
}

function summarizeTree(tree) {
  const nodes = [];
  walk(tree, (node) => {
    const summary = {
      type: node.type,
      line: node.position?.start.line,
    };
    if (node.type === "definition") {
      summary.identifier = node.identifier;
      summary.label = node.label;
      summary.url = node.url;
    }
    if (node.type === "linkReference") {
      summary.identifier = node.identifier;
      summary.label = node.label;
      summary.referenceType = node.referenceType;
    }
    if (node.type === "mdxjsEsm") summary.value = node.value;
    if (node.type === "mdxJsxFlowElement" || node.type === "mdxJsxTextElement") {
      summary.name = node.name;
      summary.attributes = (node.attributes ?? []).map((attribute) => ({
        type: attribute.type,
        name: attribute.name,
        valueKind: typeof attribute.value === "string" ? "literal" : attribute.value === null ? "boolean" : "expression",
        value: typeof attribute.value === "string" ? attribute.value : undefined,
      }));
    }
    nodes.push(summary);
  });
  return {
    rootChildren: tree.children.map((node) => ({ type: node.type, line: node.position?.start.line })),
    definitions: nodes.filter((node) => node.type === "definition"),
    references: nodes.filter((node) => node.type === "linkReference"),
    esm: nodes.filter((node) => node.type === "mdxjsEsm"),
    jsx: nodes.filter((node) => node.type === "mdxJsxFlowElement" || node.type === "mdxJsxTextElement"),
    nodeTypeCounts: Object.fromEntries(
      [...new Set(nodes.map((node) => node.type))]
        .sort()
        .map((type) => [type, nodes.filter((node) => node.type === type).length]),
    ),
  };
}

function renderedHrefs(tree) {
  const hast = toHast(tree);
  const hrefs = [];
  walk(hast, (node) => {
    if (node.type === "element" && node.tagName === "a") hrefs.push(node.properties?.href);
  });
  return hrefs;
}

function parseFixture(profileName, source) {
  try {
    const tree = profiles[profileName](unified()).parse(source);
    const summary = summarizeTree(tree);
    if (profileName !== "mdxBasic" && profileName !== "mdxWithSiteMath") {
      summary.renderedHrefs = renderedHrefs(tree);
    } else if (!summary.esm.length && !summary.jsx.length) {
      summary.renderedHrefs = renderedHrefs(tree);
    }
    return { status: "parsed", ...summary };
  } catch (error) {
    return { status: "parse-error", message: String(error) };
  }
}

async function compileFixture(format, source) {
  delete globalThis.__ASSURE_SOURCE_WAS_EVALUATED__;
  delete globalThis.__ASSURE_JSX_WAS_EVALUATED__;
  try {
    const compiled = await compile(source, {
      format,
      remarkPlugins: [remarkMath, remarkGfm],
    });
    return {
      status: "compiled-not-evaluated",
      outputBytes: Buffer.byteLength(String(compiled)),
      sourceMarkerExecuted: globalThis.__ASSURE_SOURCE_WAS_EVALUATED__ === true,
      jsxMarkerExecuted: globalThis.__ASSURE_JSX_WAS_EVALUATED__ === true,
      outputContainsImport: String(compiled).includes("import Component"),
    };
  } catch (error) {
    return {
      status: "compile-error",
      message: String(error),
      sourceMarkerExecuted: globalThis.__ASSURE_SOURCE_WAS_EVALUATED__ === true,
      jsxMarkerExecuted: globalThis.__ASSURE_JSX_WAS_EVALUATED__ === true,
    };
  }
}

function corpusMatrix() {
  const scan = JSON.parse(readFileSync(resolve(experimentDir, "current-scan.json"), "utf8"));
  const documents = scan.discovery.files
    .filter((file) => file.recommendedReason.startsWith("structured:"))
    .map((file) => file.path);
  const result = {};
  for (const profileName of Object.keys(profiles)) {
    const errors = [];
    let definitions = 0;
    let repeatedDefinitionDocuments = 0;
    for (const document of documents) {
      const source = readFileSync(resolve(root, document), "utf8");
      const parsed = parseFixture(profileName, source);
      if (parsed.status !== "parsed") {
        errors.push({ document, message: parsed.message });
        continue;
      }
      definitions += parsed.definitions.length;
      const identifiers = parsed.definitions.map((definition) => definition.identifier);
      if (new Set(identifiers).size !== identifiers.length) repeatedDefinitionDocuments += 1;
    }
    result[profileName] = {
      documents: documents.length,
      parsed: documents.length - errors.length,
      errors,
      definitions,
      repeatedDefinitionDocuments,
    };
  }
  return result;
}

const fixtureMatrix = {};
for (const [fixtureName, source] of Object.entries(fixtures)) {
  fixtureMatrix[fixtureName] = {};
  for (const profileName of Object.keys(profiles)) {
    fixtureMatrix[fixtureName][profileName] = parseFixture(profileName, source);
  }
  fixtureMatrix[fixtureName].mdxCompile = await compileFixture("mdx", source);
  fixtureMatrix[fixtureName].markdownCompile = await compileFixture("md", source);
}

const report = {
  schema: "ci-idea/directive-matrix/v1",
  dependencies: {
    node: process.version,
    mdx: JSON.parse(readFileSync(resolve(root, "docs/node_modules/@mdx-js/mdx/package.json"), "utf8")).version,
    remarkParse: JSON.parse(readFileSync(resolve(root, "docs/node_modules/remark-parse/package.json"), "utf8")).version,
    remarkMdx: JSON.parse(readFileSync(resolve(root, "docs/node_modules/remark-mdx/package.json"), "utf8")).version,
    remarkGfm: JSON.parse(readFileSync(resolve(root, "docs/node_modules/remark-gfm/package.json"), "utf8")).version,
    remarkMath: JSON.parse(readFileSync(resolve(root, "docs/node_modules/remark-math/package.json"), "utf8")).version,
  },
  executionGuard: {
    sourceMarkerExecuted: globalThis.__ASSURE_SOURCE_WAS_EVALUATED__ === true,
    jsxMarkerExecuted: globalThis.__ASSURE_JSX_WAS_EVALUATED__ === true,
  },
  fixtureMatrix,
  currentCorpus: corpusMatrix(),
};
const serialized = `${JSON.stringify(report, null, 2)}\n`;
if (outputPath) writeFileSync(outputPath, serialized);
else process.stdout.write(serialized);
