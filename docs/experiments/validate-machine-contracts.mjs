import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

const contracts = [
  ["scanner-policy-v1.schema.json", "scanner-policy-v1.json"],
  ["organization-floor-v1.schema.json", "organization-floor-v1.json"],
  ["debt-snapshot-v1.schema.json", "debt-snapshot-v1.json"],
  ["waiver-bundle-v1.schema.json", "waiver-bundle-v1.json"],
  ["scanner-report-v1.schema.json", "scanner-report-v1.json"],
];

const parse = (path) => JSON.parse(readFileSync(path, "utf8"));

const canonical = (value) => {
  if (value === null || typeof value !== "object") return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(canonical).join(",")}]`;
  return `{${Object.keys(value)
    .sort()
    .map((key) => `${JSON.stringify(key)}:${canonical(value[key])}`)
    .join(",")}}`;
};

const digest = (domain, value) => {
  const hash = createHash("sha256");
  hash.update(Buffer.from(domain, "utf8"));
  hash.update(Buffer.from([0]));
  hash.update(Buffer.from(canonical(value), "utf8"));
  return `sha256:${hash.digest("hex")}`;
};

const digestBytes = (domain, bytes) => {
  const hash = createHash("sha256");
  hash.update(Buffer.from(domain, "utf8"));
  hash.update(Buffer.from([0]));
  hash.update(bytes);
  return `sha256:${hash.digest("hex")}`;
};

const same = (left, right) => canonical(left) === canonical(right);

const stripAnnotations = (value) => {
  if (value === null || typeof value !== "object") return value;
  if (Array.isArray(value)) return value.map(stripAnnotations);
  return Object.fromEntries(
    Object.entries(value)
      .filter(([key]) => !["description", "title", "$comment", "examples", "default"].includes(key))
      .map(([key, item]) => [key, stripAnnotations(item)]),
  );
};

const resolveRef = (schemaRoot, ref) => {
  if (!ref.startsWith("#/")) throw new Error(`unsupported non-local $ref ${ref}`);
  return ref
    .slice(2)
    .split("/")
    .map((part) => part.replaceAll("~1", "/").replaceAll("~0", "~"))
    .reduce((value, part) => value[part], schemaRoot);
};

const expandLocalRefs = (value, schemaRoot, stack = []) => {
  if (value === null || typeof value !== "object") return value;
  if (Array.isArray(value)) return value.map((item) => expandLocalRefs(item, schemaRoot, stack));
  if (value.$ref) {
    if (stack.includes(value.$ref)) throw new Error(`recursive schema ref ${value.$ref}`);
    return expandLocalRefs(resolveRef(schemaRoot, value.$ref), schemaRoot, [...stack, value.$ref]);
  }
  return Object.fromEntries(
    Object.entries(stripAnnotations(value)).map(([key, item]) => [
      key,
      expandLocalRefs(item, schemaRoot, stack),
    ]),
  );
};

const typeMatches = (type, value) => {
  if (type === "null") return value === null;
  if (type === "array") return Array.isArray(value);
  if (type === "object") return value !== null && typeof value === "object" && !Array.isArray(value);
  if (type === "integer") return Number.isSafeInteger(value) && !Object.is(value, -0);
  return typeof value === type;
};

const validate = (schema, value, schemaRoot, location = "$") => {
  if (schema.$ref) return validate(resolveRef(schemaRoot, schema.$ref), value, schemaRoot, location);

  for (const candidate of schema.allOf ?? []) {
    validate(candidate, value, schemaRoot, location);
  }
  if (schema.if) {
    let conditionMatches = false;
    try {
      validate(schema.if, value, schemaRoot, location);
      conditionMatches = true;
    } catch {
      conditionMatches = false;
    }
    if (conditionMatches && schema.then) validate(schema.then, value, schemaRoot, location);
    if (!conditionMatches && schema.else) validate(schema.else, value, schemaRoot, location);
  }

  if (schema.oneOf) {
    const matches = schema.oneOf.filter((candidate) => {
      try {
        validate(candidate, value, schemaRoot, location);
        return true;
      } catch {
        return false;
      }
    });
    if (matches.length !== 1) throw new Error(`${location}: expected exactly one oneOf match`);
    return;
  }

  if (schema.const !== undefined && !same(schema.const, value)) {
    throw new Error(`${location}: const mismatch`);
  }
  if (schema.enum && !schema.enum.some((candidate) => same(candidate, value))) {
    throw new Error(`${location}: value is outside enum`);
  }

  const types = schema.type === undefined ? [] : Array.isArray(schema.type) ? schema.type : [schema.type];
  if (types.length > 0 && !types.some((type) => typeMatches(type, value))) {
    throw new Error(`${location}: expected type ${types.join("|")}`);
  }

  if (typeof value === "string") {
    if (schema.minLength !== undefined && [...value].length < schema.minLength) {
      throw new Error(`${location}: shorter than minLength`);
    }
    if (schema.maxLength !== undefined && [...value].length > schema.maxLength) {
      throw new Error(`${location}: longer than maxLength`);
    }
    if (schema.pattern && !new RegExp(schema.pattern, "u").test(value)) {
      throw new Error(`${location}: pattern mismatch`);
    }
  }

  if (Number.isInteger(value)) {
    if (!Number.isSafeInteger(value) || Object.is(value, -0)) {
      throw new Error(`${location}: unsafe or negative-zero integer`);
    }
    if (schema.minimum !== undefined && value < schema.minimum) {
      throw new Error(`${location}: below minimum`);
    }
    if (schema.maximum !== undefined && value > schema.maximum) {
      throw new Error(`${location}: above maximum`);
    }
  }

  if (Array.isArray(value)) {
    if (schema.minItems !== undefined && value.length < schema.minItems) {
      throw new Error(`${location}: fewer than minItems`);
    }
    if (schema.maxItems !== undefined && value.length > schema.maxItems) {
      throw new Error(`${location}: more than maxItems`);
    }
    if (schema.uniqueItems && new Set(value.map(canonical)).size !== value.length) {
      throw new Error(`${location}: duplicate array value`);
    }
    if (schema.items) {
      value.forEach((item, index) => validate(schema.items, item, schemaRoot, `${location}[${index}]`));
    }
  }

  if (value !== null && typeof value === "object" && !Array.isArray(value)) {
    const properties = schema.properties ?? {};
    for (const required of schema.required ?? []) {
      if (!Object.hasOwn(value, required)) throw new Error(`${location}: missing ${required}`);
    }
    if (schema.additionalProperties === false) {
      for (const key of Object.keys(value)) {
        if (!Object.hasOwn(properties, key)) throw new Error(`${location}: unknown ${key}`);
      }
    }
    for (const [key, item] of Object.entries(value)) {
      if (properties[key]) validate(properties[key], item, schemaRoot, `${location}.${key}`);
    }
  }
};

const auditSchema = (value, schemaRoot, location = "$") => {
  if (value === null || typeof value !== "object") return;
  if (Array.isArray(value)) {
    value.forEach((item, index) => auditSchema(item, schemaRoot, `${location}[${index}]`));
    return;
  }
  if (value.$ref) resolveRef(schemaRoot, value.$ref);
  if (value.pattern) new RegExp(value.pattern, "u");
  for (const [key, item] of Object.entries(value)) {
    auditSchema(item, schemaRoot, `${location}.${key}`);
  }
};

const loaded = contracts.map(([schemaName, exampleName]) => {
  const schema = parse(join(root, "spec", schemaName));
  const example = parse(join(root, "spec", "examples", exampleName));
  auditSchema(schema, schema);
  validate(schema, example, schema);
  return { schemaName, exampleName, schema, example };
});

const byExample = Object.fromEntries(loaded.map((item) => [item.exampleName, item.example]));
const schemaByName = Object.fromEntries(loaded.map((item) => [item.schemaName, item.schema]));
const reportSchemaResource = loaded.find(
  (item) => item.schemaName === "scanner-report-v1.schema.json",
).schema;
const policySchemaResource = schemaByName["scanner-policy-v1.schema.json"];
const floorSchemaResource = schemaByName["organization-floor-v1.schema.json"];
const debtSchemaResource = schemaByName["debt-snapshot-v1.schema.json"];
const waiverSchemaResource = schemaByName["waiver-bundle-v1.schema.json"];

const requireSameDefinitions = (label, entries, { expand = false } = {}) => {
  const values = entries.map(([schema, definition]) => {
    const value = schema.$defs[definition];
    return expand ? expandLocalRefs(value, schema) : stripAnnotations(value);
  });
  if (values.slice(1).some((value) => !same(values[0], value))) {
    throw new Error(`${label}: duplicated schema definitions drifted`);
  }
};
requireSameDefinitions("RepoPath", [
  [policySchemaResource, "RepoPath"],
  [floorSchemaResource, "RepoPath"],
  [debtSchemaResource, "RepoPath"],
  [waiverSchemaResource, "RepoPath"],
  [reportSchemaResource, "RepoPath"],
]);
requireSameDefinitions("RepositoryIdentity", [
  [floorSchemaResource, "RepositoryIdentity"],
  [debtSchemaResource, "RepositoryIdentity"],
  [waiverSchemaResource, "RepositoryIdentity"],
  [reportSchemaResource, "RepositoryIdentity"],
]);
requireSameDefinitions("TreeIdentity", [
  [debtSchemaResource, "TreeIdentity"],
  [waiverSchemaResource, "TreeIdentity"],
  [reportSchemaResource, "TreeIdentity"],
]);
requireSameDefinitions("SourceConstruct", [
  [debtSchemaResource, "SourceConstruct"],
  [waiverSchemaResource, "SourceConstruct"],
  [reportSchemaResource, "SourceConstruct"],
]);
for (const [debtDefinition, waiverDefinition] of [
  ["DebtOccurrence", "WaiverOccurrence"],
  ["DebtFindingScope", "WaiverFindingScope"],
  ["DebtFindingKeyInput", "WaiverFindingKeyInput"],
  ["StructuralResolution", "StructuralResolution"],
  ["StructuralFactEvidence", "StructuralFactEvidence"],
  ["StructuralFindingFactInput", "StructuralFindingFactInput"],
]) {
  requireSameDefinitions(`debt/waiver ${debtDefinition}`, [
    [debtSchemaResource, debtDefinition],
    [waiverSchemaResource, waiverDefinition],
  ], { expand: true });
}
requireSameDefinitions("report/control RepositoryTargetIntent", [
  [debtSchemaResource, "RepositoryTargetIntent"],
  [waiverSchemaResource, "RepositoryTargetIntent"],
  [reportSchemaResource, "RepositoryTargetIntent"],
], { expand: true });
requireSameDefinitions("report/control ReferenceFindingScope", [
  [debtSchemaResource, "DebtFindingScope"],
  [waiverSchemaResource, "WaiverFindingScope"],
  [reportSchemaResource, "ReferenceFindingScope"],
], { expand: true });
const fragmentExamples = [
  ["IndexProjectionInput", "index-projection-v1.json"],
  ["SyntheticSnapshotInput", "synthetic-snapshot-v1.json"],
  ["CandidateIdentityInput", "candidate-identity-v1.json"],
  ["CandidateIdentityInput", "candidate-identity-index-v1.json"],
].map(([definition, exampleName]) => {
  const example = parse(join(root, "spec", "examples", exampleName));
  validate(reportSchemaResource.$defs[definition], example, reportSchemaResource);
  return { definition, exampleName, example };
});
const byFragmentExample = Object.fromEntries(
  fragmentExamples.map((item) => [item.exampleName, item.example]),
);
const indexProjectionExample = byFragmentExample["index-projection-v1.json"];
const syntheticSnapshotExample = byFragmentExample["synthetic-snapshot-v1.json"];
const indexProjectionDigest = digest(
  "assure/scanner-index-projection/v1",
  indexProjectionExample,
);
const syntheticSnapshotDigest = digest(
  "assure/scanner-snapshot/v1",
  syntheticSnapshotExample,
);
if (
  indexProjectionDigest !==
    "sha256:744ba61a0f30ee6ecbc6b3e93323428ef523034a0636f9eb6c6215c3504c40bd" ||
  syntheticSnapshotExample.index_projection_digest !== indexProjectionDigest ||
  syntheticSnapshotDigest !==
    "sha256:762011b6032198b5ef43ce268c985d69e0e45e17da679bcb424a68298b13ebba" ||
  syntheticSnapshotExample.identity_scope !== "complete-logical-index"
) {
  throw new Error("synthetic snapshot example does not bind the complete logical-index example");
}
const floor = byExample["organization-floor-v1.json"];
const floorDigest = digest("assure/organization-floor/v1", floor);
const policySchema = loaded.find((item) => item.schemaName === "scanner-policy-v1.schema.json").schema;
const organizationSchema = loaded.find(
  (item) => item.schemaName === "organization-floor-v1.schema.json",
).schema;
const promotableKinds = [
  "explicit-target-missing",
  "explicit-target-type-mismatch",
  "invalid-reference",
];
if (
  !same(policySchema.$defs.FindingKind.enum, promotableKinds) ||
  !same(organizationSchema.$defs.FloorPromotableFindingKind.enum, promotableKinds)
) {
  throw new Error("repository/floor promotable finding kinds are not the closed structural set");
}

for (const name of ["debt-snapshot-v1.json", "waiver-bundle-v1.json"]) {
  const value = byExample[name];
  if (value.organization_floor_digest !== floorDigest) {
    throw new Error(`${name}: organization_floor_digest does not match the floor example`);
  }
  for (const item of value.items) {
    validate(
      reportSchemaResource.$defs.FindingKeyInput,
      item.key_input,
      reportSchemaResource,
      `${name}.${item.finding_key}.key_input`,
    );
    const findingKey = digest("assure/scanner-finding-key/v1", item.key_input);
    if (findingKey !== item.finding_key) throw new Error(`${name}: finding_key mismatch`);
    const fact = item.accepted_fact ?? item.authorized_fact;
    const factDigest = item.accepted_fact_digest ?? item.authorized_fact_digest;
    if (!same(fact.key_input, item.key_input) || fact.finding_kind !== item.finding_kind) {
      throw new Error(`${name}: embedded fact does not repeat its item key and kind`);
    }
    validate(
      reportSchemaResource.$defs.FindingFactInput,
      fact,
      reportSchemaResource,
      `${name}.${item.finding_key}.fact`,
    );
    if (digest("assure/scanner-fact/v1", fact) !== factDigest) {
      throw new Error(`${name}: embedded fact digest mismatch`);
    }
  }
}

const debt = byExample["debt-snapshot-v1.json"];
if (
  new Set(debt.items.map((item) => item.debt_id)).size !== debt.items.length ||
  new Set(debt.items.map((item) => item.finding_key)).size !== debt.items.length
) {
  throw new Error("debt IDs and finding keys must each be globally unique");
}
if (!same(debt.items.map((item) => item.debt_id), debt.items.map((item) => item.debt_id).sort())) {
  throw new Error("debt items are not sorted by debt_id");
}
for (const item of debt.items) {
  if (!(item.created_at <= debt.created_at && item.created_at < item.expires_at)) {
    throw new Error(`debt temporal ordering is invalid for ${item.debt_id}`);
  }
  if (!floor.authorized_debt_owners.includes(item.owner)) {
    throw new Error(`debt owner is not authorized by the floor example for ${item.debt_id}`);
  }
}

const waiver = byExample["waiver-bundle-v1.json"];
for (const item of waiver.items) {
  if (!(
    item.created_at <= waiver.created_at &&
    item.created_at <= item.not_before &&
    item.not_before < item.expires_at
  )) {
    throw new Error(`waiver temporal ordering is invalid for ${item.waiver_id}`);
  }
  if (!floor.authorized_waiver_issuers.includes(item.issuer)) {
    throw new Error(`waiver issuer is not authorized by the floor example for ${item.waiver_id}`);
  }
  if (!floor.waivable_finding_kinds.includes(item.finding_kind)) {
    throw new Error(`waiver kind is not authorized by the floor example for ${item.waiver_id}`);
  }
  if (item.owner === item.issuer) {
    throw new Error(`waiver owner and issuer must differ for ${item.waiver_id}`);
  }
}

const report = byExample["scanner-report-v1.json"];
const candidateIdentityExample = byFragmentExample["candidate-identity-v1.json"];
const evaluation = report.payload.evaluation;
const derivedCandidateIdentity = {
  schema: "assure/scanner-candidate-identity/v1",
  mode: evaluation.mode,
  event_kind: evaluation.event_kind,
  finality: evaluation.finality,
  repository: evaluation.repository,
  ref: evaluation.ref,
  default_branch_ref: evaluation.default_branch_ref,
  base: evaluation.base,
  candidate: evaluation.candidate,
  materialization: evaluation.materialization,
  skip_worktree_paths: evaluation.skip_worktree_paths,
  index_only_materialized_paths: evaluation.index_only_materialized_paths,
};
if (!same(candidateIdentityExample, derivedCandidateIdentity)) {
  throw new Error("candidate identity example does not reproduce the report evaluation");
}
if (
  digest("assure/scanner-candidate-identity/v1", candidateIdentityExample) !==
  "sha256:5b6f7ec573960a4a2f41f10504c96d3e692f4d910548ecd96309c120742e537b"
) {
  throw new Error("commit candidate identity golden mismatch");
}
const indexCandidateIdentityExample =
  byFragmentExample["candidate-identity-index-v1.json"];
if (
  indexCandidateIdentityExample.candidate.index_projection_digest !== indexProjectionDigest ||
  indexCandidateIdentityExample.candidate.snapshot_digest !== syntheticSnapshotDigest ||
  indexCandidateIdentityExample.candidate.entry_count !== indexProjectionExample.entries.length ||
  indexCandidateIdentityExample.skip_worktree_paths !==
    indexProjectionExample.entries.filter((item) => item.skip_worktree).length ||
  indexCandidateIdentityExample.index_only_materialized_paths !== 0 ||
  digest("assure/scanner-candidate-identity/v1", indexCandidateIdentityExample) !==
    "sha256:3cd8f228b0314d0ea76dc8b16b01bc32fe45e31575136cb2f4a453f588656670"
) {
  throw new Error("index candidate identity example/golden mismatch");
}
const reportSchema = loaded.find((item) => item.schemaName === "scanner-report-v1.schema.json").schema;
const floorSchema = loaded.find((item) => item.schemaName === "organization-floor-v1.schema.json").schema;
if (!same(reportSchema.$defs.ResourceName.enum, floorSchema.$defs.ResourceName.enum)) {
  throw new Error("scanner report and organization floor ResourceName enums differ");
}
if (
  policySchemaResource.properties.finding_dispositions.maxItems !== 3 ||
  floorSchemaResource.properties.minimum_dispositions.maxItems !== 3 ||
  floorSchemaResource.properties.resource_limits.maxItems !==
    floorSchemaResource.$defs.ResourceName.enum.length
) {
  throw new Error("closed policy/resource array ceilings drifted from their key enums");
}
for (const schema of [
  floorSchemaResource,
  debtSchemaResource,
  waiverSchemaResource,
  reportSchemaResource,
]) {
  validate(
    schema.$defs.RepositoryIdentity,
    { host: "github.com", owner: "citypaul", name: ".dotfiles" },
    schema,
    "RepositoryIdentity .dotfiles vector",
  );
}
if (
  !same(reportSchema.$defs.DocumentResult.properties.classification.enum, [
    "structured-markdown",
    "structured-mdx",
    "extensionless-markdown",
    "plain-advisory",
    "policy-included",
  ]) ||
  !reportSchema.$defs.DocumentSide.properties.status.enum.includes("excluded-built-in")
) {
  throw new Error("document classification and side-status levels drifted");
}
if (
  !same(reportSchema.$defs.IndexProjectionInput.required, ["schema", "entries"]) ||
  reportSchema.$defs.IndexProjectionInput.properties.entries.maxItems !== 1000000 ||
  !same(reportSchema.$defs.IndexProjectionInput.properties.entries["x-assure-order"], ["path"]) ||
  !same(reportSchema.$defs.IndexProjectionEntry.required, [
    "path",
    "entry_kind",
    "git_mode",
    "object_format",
    "object_oid",
    "skip_worktree",
  ]) ||
  !same(reportSchema.$defs.SyntheticSnapshotInput.required, [
    "schema",
    "kind",
    "identity_scope",
    "base_object_format",
    "base_commit_oid",
    "index_projection_digest",
  ]) ||
  !reportSchema.$defs.SyntheticSnapshot.required.includes("entry_count") ||
  !reportSchema.$defs.SyntheticSnapshot.required.includes("snapshot_digest")
) {
  throw new Error("complete logical-index/synthetic-snapshot schema contract drifted");
}
if (
  reportSchema.$defs.AnalysisErrorCode.enum.some((code) => code.startsWith("WORKTREE_")) ||
  reportSchema.$defs.ResolvedEvaluation.properties.mode.enum.includes("worktree") ||
  reportSchema.$defs.Resolution.properties.code.enum.includes("unsupported-target-kind") ||
  reportSchema.$defs.SyntheticSnapshot.properties.kind.const !== "index" ||
  reportSchema.$defs.SyntheticSnapshot.properties.identity_scope.const !== "complete-logical-index" ||
  reportSchema.$defs.SyntheticSnapshotEntry !== undefined ||
  reportSchema.$defs.SyntheticSnapshotInput.properties.entries !== undefined
) {
  throw new Error("removed worktree/resolution values remain reachable in the v0 report schema");
}
const machineJsonConditional = organizationSchema.$defs.ResourceLimit.allOf.find(
  (item) => item.if.properties.resource.const === "machine-json-bytes",
);
const typedErrorsConditional = organizationSchema.$defs.ResourceLimit.allOf.find(
  (item) => item.if.properties.resource.const === "typed-analysis-errors-retained",
);
if (
  machineJsonConditional.then.properties.maximum.type !== "integer" ||
  machineJsonConditional.then.properties.maximum.minimum !== 67108864 ||
  machineJsonConditional.then.properties.maximum.maximum !== 67108864
) {
  throw new Error("organization floor does not reserve the exact 64 MiB error envelope");
}
if (
  typedErrorsConditional.then.properties.maximum.type !== "integer" ||
  typedErrorsConditional.then.properties.maximum.minimum !== 1 ||
  typedErrorsConditional.then.properties.maximum.maximum !== 64
) {
  throw new Error("organization floor does not bound the retained typed-error limit to [1, 64]");
}
if (
  !same(floor.resource_limits, [
    { resource: "machine-json-bytes", maximum: 67108864 },
    { resource: "typed-analysis-errors-retained", maximum: 64 },
  ])
) {
  throw new Error("organization floor example does not exercise both hard-limit conditionals");
}
for (const [resource, maximum] of [
  ["machine-json-bytes", 67108863],
  ["typed-analysis-errors-retained", 65],
]) {
  let rejected = false;
  try {
    validate(
      organizationSchema.$defs.ResourceLimit,
      { resource, maximum },
      organizationSchema,
      `negative ResourceLimit ${resource}`,
    );
  } catch {
    rejected = true;
  }
  if (!rejected) throw new Error(`organization floor accepted invalid ${resource} maximum`);
}
const wrongAddressInput = structuredClone(
  report.payload.observations[0].base.observation_id_input,
);
wrongAddressInput.structural_address.address_kind = "mdx-ast-node-path";
let rejectedWrongAddress = false;
try {
  validate(
    reportSchema.$defs.ObservationIdInput,
    wrongAddressInput,
    reportSchema,
    "negative ObservationIdInput adapter/address",
  );
} catch {
  rejectedWrongAddress = true;
}
if (!rejectedWrongAddress) {
  throw new Error("scanner report schema accepted mismatched adapter/address kind");
}
const prettyReportPath = join(root, "spec", "examples", "scanner-report-v1.json");
const canonicalReportPath = join(
  root,
  "spec",
  "examples",
  "scanner-report-v1.canonical.json",
);
const prettyReportBytes = readFileSync(prettyReportPath, "utf8");
const canonicalReportBytes = readFileSync(canonicalReportPath, "utf8");
const expectedCanonicalReport = `${canonical(report)}\n`;
if (canonicalReportBytes !== expectedCanonicalReport) {
  throw new Error("scanner report canonical wire fixture is not JCS(envelope) || LF");
}
if (!same(JSON.parse(canonicalReportBytes), report)) {
  throw new Error("scanner report canonical and indented fixtures differ semantically");
}
if (prettyReportBytes === canonicalReportBytes) {
  throw new Error("scanner report parsed-value fixture unexpectedly equals canonical wire bytes");
}

const gitignoreVectors = parse(
  join(root, "spec", "examples", "gitignore-v1-vectors.json"),
);
if (
  gitignoreVectors.schema !== "assure/gitignore-vectors/v1" ||
  gitignoreVectors.contract !== "gitignore-v1" ||
  gitignoreVectors.reference.git_version !== "2.42.0" ||
  !same(gitignoreVectors.reference.modes, [
    {
      entry_state: "untracked",
      setup: null,
      command: "git check-ignore --no-index -q -- <path>",
    },
    {
      entry_state: "tracked",
      setup: "git add -f -- <path>",
      command: "git check-ignore -q -- <path>",
    },
  ]) ||
  gitignoreVectors.reference.locale !== "C"
) {
  throw new Error("gitignore-v1 vector header mismatch");
}

const lfsPointerVectors = parse(
  join(root, "spec", "examples", "lfs-pointer-v1-vectors.json"),
);
const referenceConstructorVectors = parse(
  join(root, "spec", "examples", "reference-constructor-v1-vectors.json"),
);
const correlationIntentVectors = parse(
  join(root, "spec", "examples", "correlation-intent-v1-vectors.json"),
);
const frontmatterVectors = parse(
  join(root, "spec", "examples", "frontmatter-v1-vectors.json"),
);
const governedDefinitionVectors = parse(
  join(root, "spec", "examples", "governed-definition-v1-vectors.json"),
);
const recognizesLfsPointer = (bytes) => {
  if (bytes.length === 0 || bytes.length >= 1024) return false;
  if (bytes.length >= 3 && bytes[0] === 0xef && bytes[1] === 0xbb && bytes[2] === 0xbf) {
    return false;
  }
  let value;
  try {
    value = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    return false;
  }
  const lineEnding = value.includes("\r\n") ? "\r\n" : "\n";
  if (!value.endsWith(lineEnding)) return false;
  if (value.split(lineEnding).join("").includes("\r") || value.split(lineEnding).join("").includes("\n")) {
    return false;
  }
  const lines = value.slice(0, -lineEnding.length).split(lineEnding);
  if (lines.length < 3) return false;
  const pairs = [];
  for (const line of lines) {
    const separator = line.indexOf(" ");
    if (separator <= 0) return false;
    const key = line.slice(0, separator);
    if (!/^[a-z0-9.-]+$/u.test(key) || line[separator + 1] === " ") return false;
    pairs.push([key, line.slice(separator + 1)]);
  }
  if (
    pairs[0][0] !== "version" ||
    ![
      "https://git-lfs.github.com/spec/v1",
      "https://hawser.github.com/spec/v1",
    ].includes(pairs[0][1])
  ) {
    return false;
  }
  const tailKeys = pairs.slice(1).map(([key]) => key);
  if (
    tailKeys.includes("version") ||
    tailKeys.some((key, index) => index > 0 && tailKeys[index - 1] >= key)
  ) {
    return false;
  }
  const byKey = new Map(pairs.slice(1));
  return (
    /^sha256:[0-9a-f]{64}$/u.test(byKey.get("oid") ?? "") &&
    /^(0|[1-9][0-9]{0,18})$/u.test(byKey.get("size") ?? "") &&
    BigInt(byKey.get("size") ?? "0") <= 9223372036854775807n
  );
};
const expectedLfsPointerIds = [
  "current-basic-lf",
  "current-sorted-extensions",
  "legacy-hawser",
  "defensive-crlf",
  "unsorted-extension",
  "duplicate-key",
  "missing-final-newline",
  "mixed-line-endings",
  "uppercase-hash",
  "leading-zero-size",
  "blank-line",
  "double-separator-space",
  "second-version-key",
  "maximum-signed-size",
  "above-maximum-signed-size",
];
if (
  lfsPointerVectors.schema !== "assure/lfs-pointer-vectors/v1" ||
  lfsPointerVectors.contract !== "lfs-pointer-v1-conservative" ||
  !same(lfsPointerVectors.cases.map((item) => item.id), expectedLfsPointerIds) ||
  lfsPointerVectors.cases.some(
    (item) => typeof item.input !== "string" || typeof item.recognized !== "boolean",
  )
) {
  throw new Error("lfs-pointer-v1 vector header or IDs are invalid");
}
for (const item of lfsPointerVectors.cases) {
  if (recognizesLfsPointer(Buffer.from(item.input, "utf8")) !== item.recognized) {
    throw new Error(`lfs-pointer-v1 classification mismatch for ${item.id}`);
  }
}
const lfsBoundaryPrefix =
  "version https://git-lfs.github.com/spec/v1\n" +
  "oid sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\n" +
  "size 42\n" +
  "z.future ";
const lfsAt1023 = Buffer.from(`${lfsBoundaryPrefix}${"x".repeat(1023 - lfsBoundaryPrefix.length - 1)}\n`);
const lfsAt1024 = Buffer.from(`${lfsBoundaryPrefix}${"x".repeat(1024 - lfsBoundaryPrefix.length - 1)}\n`);
const lfsBomShort = Buffer.concat([
  Buffer.from([0xef, 0xbb, 0xbf]),
  Buffer.from(lfsPointerVectors.cases[0].input, "utf8"),
]);
if (
  lfsAt1023.length !== 1023 ||
  !recognizesLfsPointer(lfsAt1023) ||
  lfsAt1024.length !== 1024 ||
  recognizesLfsPointer(lfsAt1024) ||
  lfsBomShort.length >= 1024 ||
  recognizesLfsPointer(lfsBomShort) ||
  recognizesLfsPointer(Buffer.from([0xff]))
) {
  throw new Error("lfs-pointer-v1 byte-boundary/encoding checks failed");
}
const targetKindFor = (construct, githubForm, trailingSlash = false) => {
  if (githubForm === "blob" || githubForm === "tree") return githubForm;
  if (trailingSlash && !construct.endsWith("-image")) return "tree";
  return construct.endsWith("-image") ? "blob" : "either";
};
const isGithubLineFragment = (value) => {
  const match = /^L([1-9][0-9]{0,15})(?:-L([1-9][0-9]{0,15}))?$/u.exec(value);
  if (!match) return false;
  const start = BigInt(match[1]);
  const end = BigInt(match[2] ?? match[1]);
  return start <= 9007199254740991n && end <= 9007199254740991n && end >= start;
};
const githubRefSplit = (encodedSuffix, candidateRef, defaultRef, githubForm) => {
  const decoded = [];
  try {
    const encodedSegments = encodedSuffix.split("/");
    if (encodedSegments.at(-1) === "") {
      if (githubForm !== "tree" || encodedSegments.length < 2) {
        return { status: "invalid", path: null };
      }
      encodedSegments.pop();
    }
    for (const segment of encodedSegments) {
      const value = decodeURIComponent(segment);
      if (!value || /[\/\\\x00-\x1f\x7f]/u.test(value)) return { status: "invalid", path: null };
      decoded.push(value);
    }
  } catch {
    return { status: "invalid", path: null };
  }
  const matchRef = (ref) => {
    const parts = ref.slice("refs/heads/".length).split("/");
    if (parts.some((part, index) => decoded[index] !== part)) return { kind: "none" };
    if (decoded.length === parts.length) return { kind: "empty" };
    if (decoded.length < parts.length) return { kind: "none" };
    const remainder = decoded.slice(parts.length);
    const path = remainder.join("/");
    if (
      remainder.some((part) => part === "." || part === "..") ||
      Buffer.byteLength(path, "utf8") > 4096
    ) {
      return { kind: "invalid" };
    }
    return { kind: "path", path };
  };
  const candidate = matchRef(candidateRef);
  const defaultMatch = matchRef(defaultRef);
  if (
    [candidate.kind, defaultMatch.kind].some((kind) => ["empty", "invalid"].includes(kind))
  ) {
    return { status: "invalid", path: null };
  }
  if (candidate.kind === "path" && defaultMatch.kind === "path" && candidateRef !== defaultRef) {
    return { status: "unsupported-version-scope", path: null };
  }
  if (candidate.kind === "path") return { status: "candidate", path: candidate.path };
  if (defaultMatch.kind === "path") {
    return { status: "unsupported-version-scope", path: defaultMatch.path };
  }
  return { status: "unsupported-version-scope", path: null };
};
const semanticAutolink = (form, token) => {
  if (["commonmark-uri", "gfm-protocol"].includes(form)) return token;
  if (["commonmark-email", "gfm-email"].includes(form)) return `mailto:${token}`;
  if (form === "gfm-www") return `http://${token}`;
  throw new Error(`unknown autolink form ${form}`);
};
const uriComponents = (value) => {
  const fragmentAt = value.indexOf("#");
  const prefix = fragmentAt < 0 ? value : value.slice(0, fragmentAt);
  const queryAt = prefix.indexOf("?");
  return {
    path: queryAt < 0 ? prefix : prefix.slice(0, queryAt),
    query: queryAt < 0 ? null : prefix.slice(queryAt + 1),
    fragment: fragmentAt < 0 ? null : value.slice(fragmentAt + 1),
  };
};
const githubIdentityMatches = (item) => {
  const literal = /^[A-Za-z0-9._-]+$/u;
  return (
    item.host === "github.com" &&
    literal.test(item.url_owner) &&
    literal.test(item.url_repository) &&
    item.url_owner.toLowerCase() === item.identity_owner &&
    item.url_repository.toLowerCase() === item.identity_repository
  );
};
const resolutionBoundary = (item) => {
  if (item.query_present && item.target_class !== "document") {
    return "unsupported-query-semantics";
  }
  if (!item.fragment_present) return "exact-path";
  if (item.github_line_fragment || item.target_class === "code") {
    return "code-fragment-unevaluated";
  }
  return "unsupported-fragment-semantics";
};
const expectedReferenceConstructorIds = Array.from(
  { length: 38 },
  (_, index) => `RI-${String(index + 1).padStart(3, "0")}-${[
    "native-link-kind",
    "native-image-kind",
    "github-blob-kind",
    "github-tree-kind",
    "line-single",
    "line-range",
    "line-zero",
    "line-leading-zero",
    "line-reversed",
    "line-too-large",
    "line-lowercase",
    "unicode-ref",
    "percent-once",
    "encoded-slash-ref",
    "only-default-ref",
    "two-trusted-splits",
    "empty-native-destination",
    "uppercase-external-scheme",
    "network-path",
    "commonmark-uri-autolink",
    "commonmark-email-autolink",
    "gfm-protocol-autolink",
    "gfm-www-autolink",
    "gfm-email-autolink",
    "multiple-question-components",
    "question-inside-fragment",
    "mixed-case-github-identity",
    "encoded-github-identity",
    "current-ref-without-path",
    "document-query-and-fragment",
    "code-query-and-fragment",
    "github-line-is-boundary",
    "uppercase-github-host-is-foreign",
    "default-only-invalid-path",
    "native-link-directory-hint",
    "native-image-terminal-slash-invalid",
    "github-tree-terminal-slash",
    "github-blob-terminal-slash-invalid",
  ][index]}`,
);
if (
  referenceConstructorVectors.schema !== "assure/reference-constructor-vectors/v1" ||
  referenceConstructorVectors.contract !== "reference-constructor-v1" ||
  !same(referenceConstructorVectors.cases.map((item) => item.id), expectedReferenceConstructorIds)
) {
  throw new Error("reference-constructor-v1 vector header or IDs are invalid");
}
for (const item of referenceConstructorVectors.cases) {
  let actual;
  if (item.operation === "target-kind") {
    actual = targetKindFor(item.construct, item.github_form, item.trailing_slash);
  } else if (item.operation === "github-line-fragment") {
    actual = isGithubLineFragment(item.value);
  } else if (item.operation === "github-ref-split") {
    actual = githubRefSplit(
      item.encoded_suffix,
      item.candidate_ref,
      item.default_ref,
      item.github_form,
    );
  } else if (item.operation === "native-trailing-slash") {
    actual = item.construct.endsWith("-image") ? "invalid-reference" : "tree";
  } else if (item.operation === "empty-native-destination") {
    actual = item.source_document;
  } else if (item.operation === "external-scheme") {
    actual = item.value.toLowerCase();
  } else if (item.operation === "network-path") {
    actual = item.value.startsWith("//") ? "network-path-unsupported" : null;
  } else if (item.operation === "semantic-autolink") {
    actual = semanticAutolink(item.form, item.token);
  } else if (item.operation === "uri-components") {
    actual = uriComponents(item.value);
  } else if (item.operation === "github-identity") {
    actual = githubIdentityMatches(item);
  } else if (item.operation === "resolution-boundary") {
    actual = resolutionBoundary(item);
  } else {
    throw new Error(`unknown reference-constructor operation ${item.operation}`);
  }
  if (!same(actual, item.expected)) {
    throw new Error(`reference-constructor-v1 mismatch for ${item.id}`);
  }
}
const correlationIntent = (intent) => {
  if (["repository-path", "same-repository-github"].includes(intent.kind)) {
    return {
      class: "repository",
      path: intent.repository_path,
      target_kind: intent.target_kind,
      query_digest: intent.query_digest,
      fragment_digest: intent.fragment_digest,
    };
  }
  if (intent.kind === "external-url") {
    return {
      class: "external-url",
      raw_destination_digest: intent.raw_destination_digest,
      external_scheme: intent.external_scheme,
      query_digest: intent.query_digest,
      fragment_digest: intent.fragment_digest,
    };
  }
  return {
    class: intent.kind,
    raw_destination_digest: intent.raw_destination_digest,
    query_digest: intent.query_digest,
    fragment_digest: intent.fragment_digest,
  };
};
const expectedCorrelationIds = Array.from(
  { length: 8 },
  (_, index) => `CI-${String(index + 1).padStart(3, "0")}-${[
    "native-github-equivalent",
    "repository-path-changed",
    "target-kind-changed",
    "query-presence-changed",
    "external-identical",
    "external-raw-spelling-changed",
    "site-route-identical",
    "unsupported-raw-changed",
  ][index]}`,
);
if (
  correlationIntentVectors.schema !== "assure/correlation-intent-vectors/v1" ||
  correlationIntentVectors.contract !== "correlation-intent-v1" ||
  !same(correlationIntentVectors.cases.map((item) => item.id), expectedCorrelationIds)
) {
  throw new Error("correlation-intent-v1 vector header or IDs are invalid");
}
for (const item of correlationIntentVectors.cases) {
  validate(reportSchema.$defs.TargetIntent, item.left, reportSchema);
  validate(reportSchema.$defs.TargetIntent, item.right, reportSchema);
  const actual = same(correlationIntent(item.left), correlationIntent(item.right));
  if (actual !== item.expected_equal) {
    throw new Error(`correlation-intent-v1 mismatch for ${item.id}`);
  }
}

const makeFrontmatterVector = (item) => {
  const parts = [];
  const newline = item.newline === "crlf" ? "\r\n" : item.newline === "cr" ? "\r" : "\n";
  if (item.bom) parts.push(Buffer.from([0xef, 0xbb, 0xbf]));
  parts.push(
    Buffer.from(`${item.opener}${newline}${"a".repeat(item.payload_bytes)}${newline}`, "utf8"),
  );
  if (item.closer !== null) {
    parts.push(Buffer.from(`${item.closer}${item.closer_at_eof ? "" : newline}`, "utf8"));
  }
  return Buffer.concat(parts);
};
const frontmatterLine = (bytes, start) => {
  let contentEnd = start;
  while (contentEnd < bytes.length && bytes[contentEnd] !== 0x0a && bytes[contentEnd] !== 0x0d) {
    contentEnd += 1;
  }
  let exclusiveEnd = contentEnd;
  if (exclusiveEnd < bytes.length) {
    exclusiveEnd +=
      bytes[exclusiveEnd] === 0x0d && bytes[exclusiveEnd + 1] === 0x0a ? 2 : 1;
  }
  return {
    contentEnd,
    exclusiveEnd,
    hasEnding: contentEnd < bytes.length,
  };
};
const recognizeFrontmatter = (bytes) => {
  const bom = bytes.length >= 3 && bytes[0] === 0xef && bytes[1] === 0xbb && bytes[2] === 0xbf;
  const start = bom ? 3 : 0;
  const body = bytes.subarray(start);
  const first = frontmatterLine(body, 0);
  if (!first.hasEnding) return null;
  const opener = body.subarray(0, first.contentEnd).toString("utf8");
  if (!['---', '+++'].includes(opener)) return null;
  let cursor = first.exclusiveEnd;
  while (cursor <= body.length) {
    const current = frontmatterLine(body, cursor);
    const line = body.subarray(cursor, current.contentEnd).toString("utf8");
    const closes = line === opener || (opener === "---" && line === "...");
    if (closes) return current.exclusiveEnd <= 65536 ? current.exclusiveEnd : null;
    if (!current.hasEnding || current.exclusiveEnd > 65536) return null;
    cursor = current.exclusiveEnd;
  }
  return null;
};
const expectedFrontmatterIds = Array.from(
  { length: 9 },
  (_, index) => `FM-${String(index + 1).padStart(3, "0")}-${[
    "no-bom-exact-bound",
    "no-bom-over-bound",
    "bom-exact-bound",
    "bom-over-bound",
    "plus-matched",
    "mismatched-closer",
    "no-closer",
    "crlf-matched",
    "bare-cr-matched-at-eof",
  ][index]}`,
);
if (
  frontmatterVectors.schema !== "assure/frontmatter-vectors/v1" ||
  frontmatterVectors.contract !== "frontmatter-v1" ||
  !same(frontmatterVectors.cases.map((item) => item.id), expectedFrontmatterIds)
) {
  throw new Error("frontmatter-v1 vector header or IDs are invalid");
}
for (const item of frontmatterVectors.cases) {
  const actualBytes = recognizeFrontmatter(makeFrontmatterVector(item));
  if ((actualBytes !== null) !== item.expected || actualBytes !== item.expected_frontmatter_bytes) {
    throw new Error(`frontmatter-v1 mismatch for ${item.id}`);
  }
}

const expectedGovernedIds = Array.from(
  { length: 6 },
  (_, index) => `GD-${String(index + 1).padStart(3, "0")}-${[
    "canonical-candidate",
    "decoded-colon",
    "uppercase-not-reserved",
    "duplicate-source-multiplicity",
    "base-only-does-not-emit",
    "losing-reserved-does-not-suppress",
  ][index]}`,
);
if (
  governedDefinitionVectors.schema !== "assure/governed-definition-vectors/v1" ||
  governedDefinitionVectors.contract !== "governed-definition-source-v1" ||
  !same(governedDefinitionVectors.cases.map((item) => item.id), expectedGovernedIds)
) {
  throw new Error("governed-definition-source-v1 vector header or IDs are invalid");
}
for (const item of governedDefinitionVectors.cases) {
  const counts = new Map();
  const winners = new Map();
  let members = 0;
  for (const definition of item.candidate_definitions) {
    if (definition.normalized_label !== undefined && !winners.has(definition.normalized_label)) {
      winners.set(definition.normalized_label, definition);
    }
    if (!definition.decoded_label.startsWith("assure:")) continue;
    members += 1;
    const sourceDigest = digestBytes(
      "assure/scanner-governed-definition-source/v1",
      Buffer.from(definition.source, "utf8"),
    );
    counts.set(sourceDigest, (counts.get(sourceDigest) ?? 0) + 1);
  }
  const sources = [...counts]
    .sort(([left], [right]) => (left < right ? -1 : left > right ? 1 : 0))
    .map(([sourceDigest, multiplicity]) => ({ digest: sourceDigest, multiplicity }));
  if (members !== item.expected_member_count || !same(sources, item.expected_sources)) {
    throw new Error(`governed-definition-source-v1 mismatch for ${item.id}`);
  }
  if (item.consuming_normalized_labels !== undefined) {
    const ordinary = item.consuming_normalized_labels.filter((label) => {
      const winner = winners.get(label);
      return winner !== undefined && !winner.decoded_label.startsWith("assure:");
    }).length;
    if (ordinary !== item.expected_ordinary_reference_count) {
      throw new Error(`governed-definition consumer precedence mismatch for ${item.id}`);
    }
  }
}
const expectedGitignoreIds = Array.from(
  { length: 26 },
  (_, index) => `GI-${String(index + 1).padStart(3, "0")}`,
);
if (!same(gitignoreVectors.cases.map((item) => item.id), expectedGitignoreIds)) {
  throw new Error("gitignore-v1 case IDs/order mismatch");
}
const reportRepoPath = new RegExp(reportSchema.$defs.RepoPath.pattern, "u");
for (const vector of gitignoreVectors.cases) {
  if (!vector.description || vector.files.length === 0 && vector.consulted.length !== 0) {
    throw new Error(`${vector.id}: malformed description/files`);
  }
  const sourcePaths = new Set();
  for (const file of vector.files) {
    if (!reportRepoPath.test(file.path) || sourcePaths.has(file.path)) {
      throw new Error(`${vector.id}: invalid or duplicate source path ${file.path}`);
    }
    if (!["regular", "symlink"].includes(file.kind) || typeof file.tracked !== "boolean") {
      throw new Error(`${vector.id}: invalid source descriptor for ${file.path}`);
    }
    if (typeof file.content_utf8 !== "string") {
      throw new Error(`${vector.id}: source bytes are not represented by a string`);
    }
    sourcePaths.add(file.path);
  }
  for (const consulted of vector.consulted) {
    const source = vector.files.find((file) => file.path === consulted);
    if (!source || source.kind !== "regular" || !/(^|\/)\.gitignore$/.test(consulted)) {
      throw new Error(`${vector.id}: invalid consulted source ${consulted}`);
    }
  }
  if (new Set(vector.consulted).size !== vector.consulted.length) {
    throw new Error(`${vector.id}: duplicate consulted source`);
  }
  for (const entry of vector.entries) {
    if (
      !reportRepoPath.test(entry.path) ||
      !["regular", "directory", "symlink"].includes(entry.kind) ||
      typeof entry.tracked !== "boolean" ||
      typeof entry.ignored !== "boolean"
    ) {
      throw new Error(`${vector.id}: invalid entry descriptor for ${entry.path}`);
    }
    if (entry.tracked && entry.ignored) {
      throw new Error(`${vector.id}: tracked entry cannot be removed by ignore rules`);
    }
  }
}

const expectedAdapterIds = ["markdown-v1", "mdx-v1", "plain-advisory-v1"];
if (!same(report.payload.engine.adapters.map((adapter) => adapter.adapter_id), expectedAdapterIds)) {
  throw new Error("scanner report adapter set/order mismatch");
}
for (const adapter of report.payload.engine.adapters) {
  if (adapter.adapter_id !== adapter.contract_descriptor.adapter_id) {
    throw new Error(`scanner report adapter_id mismatch for ${adapter.adapter_id}`);
  }
  const actual = digest("assure/scanner-adapter-contract/v1", adapter.contract_descriptor);
  if (adapter.contract_digest !== actual) {
    throw new Error(`scanner report contract_digest mismatch for ${adapter.adapter_id}`);
  }
}
for (const comparison of report.payload.observations) {
  for (const occurrence of [comparison.base, comparison.candidate].filter((item) => item !== null)) {
    const input = occurrence.observation_id_input;
    if (occurrence.observation_id !== digest("assure/observation-id/v1", input)) {
      throw new Error("scanner report observation_id mismatch");
    }
    if (
      occurrence.adapter_id !== input.adapter_id ||
      occurrence.document !== input.document ||
      occurrence.source_construct !== input.source_construct ||
      occurrence.source_projection_digest !== input.source_projection_digest ||
      !same(occurrence.intent, input.extracted_intent)
    ) {
      throw new Error("scanner report occurrence does not repeat ObservationIdInput");
    }
    const adapter = report.payload.engine.adapters.find((item) => item.adapter_id === input.adapter_id);
    if (!adapter || adapter.contract_digest !== input.adapter_contract_digest) {
      throw new Error("scanner report occurrence adapter contract mismatch");
    }
    const expectedAddressKind =
      occurrence.adapter_id === "markdown-v1" ? "markdown-ast-node-path" : "mdx-ast-node-path";
    if (input.structural_address.address_kind !== expectedAddressKind) {
      throw new Error("scanner report occurrence adapter/address kind mismatch");
    }
    if (occurrence.intent.kind === "repository-path") {
      const allowedTargetKinds = occurrence.source_construct.endsWith("-image")
        ? ["blob"]
        : occurrence.source_construct === "markdown-autolink"
          ? []
          : ["either", "tree"];
      if (!allowedTargetKinds.includes(occurrence.intent.target_kind)) {
        throw new Error("scanner report native source construct/target kind is impossible");
      }
    } else if (
      occurrence.intent.kind === "same-repository-github" &&
      !["blob", "tree"].includes(occurrence.intent.target_kind)
    ) {
      throw new Error("scanner report GitHub target kind is impossible");
    }
    const resolution = occurrence.resolution;
    if (resolution.projection_digest !== null) {
      const expected = digest("assure/scanner-target-projection/v1", {
        git_mode: resolution.git_mode,
        raw_digest: resolution.raw_digest,
      });
      if (resolution.projection_digest !== expected) {
        throw new Error("scanner report target projection mismatch");
      }
    }
  }
}
for (const finding of report.payload.findings) {
  if (finding.finding_key !== digest("assure/scanner-finding-key/v1", finding.key_input)) {
    throw new Error("scanner report finding_key mismatch");
  }
  for (const side of ["base", "candidate"]) {
    const fact = finding[`${side}_fact`];
    const factDigest = finding[`${side}_fact_digest`];
    if ((fact === null) !== (factDigest === null)) {
      throw new Error(`scanner report ${side} fact presence mismatch`);
    }
    if (fact !== null) {
      if (
        fact.finding_kind !== finding.kind ||
        !same(fact.key_input, finding.key_input) ||
        factDigest !== digest("assure/scanner-fact/v1", fact)
      ) {
        throw new Error(`scanner report ${side} fact mismatch`);
      }
    }
  }
  finding.policy_trace.forEach((step, index) => {
    if (index > 0 && step.before !== finding.policy_trace[index - 1].after) {
      throw new Error("scanner report policy trace is not adjacent");
    }
  });
  if (finding.effective_disposition !== finding.policy_trace.at(-1).after) {
    throw new Error("scanner report effective disposition does not match policy trace");
  }
  if (finding.attribution === "resolved") {
    if (
      finding.base_fact === null ||
      finding.candidate_fact !== null ||
      finding.configured_disposition !== "record" ||
      finding.effective_disposition !== "record" ||
      finding.policy_trace.length !== 1 ||
      finding.policy_trace[0].source !== "resolved-projection"
    ) {
      throw new Error("scanner report resolved projection is invalid");
    }
  }
}
const sandbox = report.payload.controls.sandbox;
if (
  sandbox.descriptor_digest !==
  digest("assure/scanner-sandbox-profile/v1", sandbox.descriptor)
) {
  throw new Error("scanner report sandbox descriptor_digest mismatch");
}
if (report.payload_digest !== digest("assure/scanner-report-payload/v1", report.payload)) {
  throw new Error("scanner report payload_digest mismatch");
}
if (report.payload.result.finding_count !== report.payload.findings.length) {
  throw new Error("scanner report finding_count mismatch");
}
if (report.payload.result.error_count !== report.payload.errors.length) {
  throw new Error("scanner report error_count mismatch");
}
if (report.payload.summary.findings.total !== report.payload.findings.length) {
  throw new Error("scanner report summary total mismatch");
}
if (
  report.payload.summary.documents.discovered !== report.payload.documents.length ||
  report.payload.summary.documents.outside_document_set !== 1
) {
  throw new Error("scanner report example document-set partition mismatch");
}
const findingCounts = report.payload.summary.findings;
if (findingCounts.record + findingCounts.warn + findingCounts.fail !== findingCounts.total) {
  throw new Error("scanner report disposition totals mismatch");
}
if (
  findingCounts.introduced +
    findingCounts.pre_existing +
    findingCounts.resolved +
    findingCounts.unknown +
    findingCounts.not_applicable !==
  findingCounts.total
) {
  throw new Error("scanner report attribution totals mismatch");
}

for (const { schemaName, schema } of loaded) {
  if (!schema.$id.startsWith("urn:assure:schema:")) throw new Error(`${schemaName}: non-URN $id`);
  const repoPath = new RegExp(schema.$defs.RepoPath.pattern, "u");
  const pathVectors = [
    ["docs/example.md", true],
    ["a%2Fb", true],
    ["../escape", false],
    ["a/../escape", false],
    ["a\n/../escape", false],
    ["a\n/b\\c", false],
    ["a\n/b\0c", false],
    ["a//b", false],
    ["/absolute", false],
  ];
  for (const [value, expected] of pathVectors) {
    if (repoPath.test(value) !== expected) {
      throw new Error(`${schemaName}: RepoPath vector failed for ${JSON.stringify(value)}`);
    }
  }
}

const refFormatV1 = (value) => {
  const bytes = Buffer.from(value, "utf8");
  if (bytes.length === 0 || bytes.length > 266 || !value.startsWith("refs/heads/")) return false;
  const components = value.split("/");
  if (components.some((part) => part.length === 0 || part.startsWith(".") || part.endsWith(".lock"))) {
    return false;
  }
  if (
    value.includes("..") ||
    value.includes("@{") ||
    value.endsWith(".") ||
    value === "@" ||
    /[\x00-\x20\x7f~^:?*\[\\]/u.test(value)
  ) {
    return false;
  }
  return value.length > "refs/heads/".length;
};

const refVectors = [
  ["refs/heads/main", true],
  ["refs/heads/feature/a+b", true],
  ["refs/heads/é", true],
  ["refs/heads/@", true],
  ["refs/heads/-dash", true],
  ["refs/tags/main", false],
  ["refs/heads/", false],
  ["refs/heads//main", false],
  ["refs/heads/.hidden", false],
  ["refs/heads/main.lock", false],
  ["refs/heads/a..b", false],
  ["refs/heads/a b", false],
  ["refs/heads/a~b", false],
  ["refs/heads/a?b", false],
  ["refs/heads/a[b", false],
  ["refs/heads/a\\b", false],
  ["refs/heads/a@{b", false],
  ["refs/heads/a.", false],
  [`refs/heads/${"a".repeat(256)}`, false],
];
for (const [value, expected] of refVectors) {
  if (refFormatV1(value) !== expected) {
    throw new Error(`ref-format-v1 vector failed for ${JSON.stringify(value)}`);
  }
}
for (const value of [report.payload.evaluation.ref, report.payload.evaluation.default_branch_ref]) {
  if (!refFormatV1(value)) throw new Error(`scanner report contains invalid ref-format-v1 ${value}`);
}

const seedVectors = [
  [
    "GV-001",
    digest("assure/claim-key/v1", { claim_id: "docs.expr-precedence" }),
    "sha256:f6a22f480cab9ed6e0fc82bcbe67eba85d88f10103f5107008809dec44fb71b0",
  ],
  [
    "GV-002",
    digest("assure/path-set-projection/v1", { members: [] }),
    "sha256:6765a67e22b2efbaaf89509cd34a70682613f002cd82d0ff4e08332e26b76954",
  ],
  [
    "GV-003",
    digest("assure/test-json/v1", { z: "é", a: 1 }),
    "sha256:1a2aab8858a444002cd16e1fa53cc33fd12e5e6ac4568f85e06bef971a28425d",
  ],
  [
    "GV-004",
    digestBytes("assure/text-projection/v1", Buffer.from("a\nb\n", "utf8")),
    "sha256:bab154d44fb1340ee8c20af6a1e36b9a903a5e44c584f8ce524237f0289b88c6",
  ],
  [
    "GV-005",
    digestBytes("assure/raw-bytes/v1", Buffer.alloc(0)),
    "sha256:28031daa5fbb3a297dc947195957fe4a05c1bd2e58c56163013ee62be9368fac",
  ],
];

for (const [id, actual, expected] of seedVectors) {
  if (actual !== expected) throw new Error(`${id}: core seed-vector mismatch`);
}

process.stdout.write(
  `smoke-checked ${loaded.length} root schema/examples, ${fragmentExamples.length} fragment examples, canonical report wire, ${gitignoreVectors.cases.length} gitignore cases, ${lfsPointerVectors.cases.length} LFS-pointer cases, ${refVectors.length} ref-format cases, ${referenceConstructorVectors.cases.length} reference-constructor cases, ${correlationIntentVectors.cases.length} correlation-intent cases, ${frontmatterVectors.cases.length} frontmatter cases, ${governedDefinitionVectors.cases.length} governed-definition cases, selected semantic digests, and ${seedVectors.length} core vectors\n`,
);
