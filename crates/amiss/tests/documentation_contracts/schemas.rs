#![expect(
    clippy::expect_used,
    reason = "integration assertions over repository-owned documentation and fixtures"
)]

use std::fs;
use std::path::{Path, PathBuf};

use amiss_wire::controls::{
    DOCUMENT_SUFFIX_BYTES, DebtSnapshot, ExecutionConstraintDescriptor, OrganizationFloor,
    SOURCE_MARKER_BYTES, ScannerPolicy, TrustedTimeStatement, WaiverBundle,
};
use amiss_wire::manifest::ReleaseManifest;
use amiss_wire::model::BranchRef;
use amiss_wire::report::{AnalysisErrorCode, ENVELOPE_SCHEMA, FindingKind, PAYLOAD_SCHEMA};
use amiss_wire::requests::{ControlsRequest, EvaluationRequest, SnapshotRequest};
use amiss_wire::semantic::RECORD_KEY_BYTES;

use crate::support::{report_schema, repository_root};

const BRANCH_REF_SCHEMA_PATTERN: &str = r"^refs/heads/(?!/)(?![\s\S]*\.\.)(?![\s\S]*@\{)(?![\s\S]*[~^:?*\[\\\u0000-\u001f\u007f ])(?!\.)(?![\s\S]*/\.)(?![^/]*\.lock(?:/|$))(?![\s\S]*/[^/]*\.lock(?:/|$))(?![\s\S]*//)(?![\s\S]*/$)(?![\s\S]*\.$).+$";

fn public_schema_examples() -> Vec<(String, PathBuf, PathBuf)> {
    let specification_directory = repository_root().join("spec");
    let examples_directory = specification_directory.join("examples");
    let mut pairs = Vec::new();

    for entry in
        fs::read_dir(&specification_directory).expect("specification directory is readable")
    {
        let schema_path = entry.expect("specification entry is readable").path();
        if !schema_path.is_file() {
            continue;
        }

        let file_name = schema_path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("public schema names are UTF-8");
        let Some(contract_name) = file_name.strip_suffix(".schema.json") else {
            continue;
        };
        let example_path = examples_directory.join(format!("{contract_name}.json"));
        assert!(
            example_path.is_file(),
            "{} has no matching public example at {}",
            schema_path.display(),
            example_path.display(),
        );
        pairs.push((contract_name.to_owned(), schema_path, example_path));
    }

    pairs.sort();
    assert!(!pairs.is_empty(), "no public JSON Schema contracts found");
    pairs
}

fn parse_defect<T, E: std::fmt::Debug>(result: Result<T, E>) -> Option<String> {
    result.err().map(|error| format!("{error:?}"))
}

fn example_reader_defect(contract_name: &str, bytes: &[u8]) -> Option<String> {
    match contract_name {
        "debt-snapshot" => parse_defect(DebtSnapshot::parse(bytes)),
        "organization-floor" => parse_defect(OrganizationFloor::parse(bytes)),
        "scanner-controls-request" => parse_defect(ControlsRequest::parse(bytes)),
        "scanner-evaluation-request" => parse_defect(EvaluationRequest::parse(bytes)),
        "scanner-execution-constraint" => parse_defect(ExecutionConstraintDescriptor::parse(bytes)),
        "scanner-external-assessment"
        | "scanner-external-evidence"
        | "scanner-external-plan"
        | "scanner-report" => parse_defect(amiss_wire::json::parse(bytes)),
        "scanner-semantic-evidence" => parse_defect(amiss_wire::semantic::parse(bytes)),
        "scanner-semantic-template" => parse_defect(amiss_wire::semantic::parse_template(bytes)),
        "scanner-policy" => parse_defect(ScannerPolicy::parse(bytes)),
        "scanner-release-manifest" => parse_defect(ReleaseManifest::parse(bytes)),
        "scanner-snapshot-request" => parse_defect(SnapshotRequest::parse(bytes)),
        "scanner-trusted-time-statement" => parse_defect(TrustedTimeStatement::parse(bytes)),
        "waiver-bundle" => parse_defect(WaiverBundle::parse(bytes)),
        _ => Some("no authoritative example reader is registered".to_owned()),
    }
}

fn check_schema_bounds(
    value: &serde_json::Value,
    schema: &Path,
    branch_count: &mut usize,
    attempt_count: &mut usize,
) {
    match value {
        serde_json::Value::Object(members) => {
            if let Some(pattern) = members.get("pattern").and_then(serde_json::Value::as_str)
                && pattern.starts_with("^refs/heads/")
            {
                assert_eq!(
                    pattern,
                    BRANCH_REF_SCHEMA_PATTERN,
                    "{} has a weaker branch-ref grammar",
                    schema.display(),
                );
                assert_eq!(
                    members.get("minLength").and_then(serde_json::Value::as_u64),
                    Some(12),
                    "{} has the wrong branch-ref minimum",
                    schema.display(),
                );
                assert_eq!(
                    members.get("maxLength").and_then(serde_json::Value::as_u64),
                    Some(266),
                    "{} has the wrong branch-ref maximum",
                    schema.display(),
                );
                *branch_count = (*branch_count).saturating_add(1);
            }
            if let Some(attempt) = members.get("provider_run_attempt")
                && attempt.get("type").and_then(serde_json::Value::as_str) == Some("integer")
            {
                let safe_max = u64::try_from(amiss_wire::json::MAX_SAFE_INTEGER)
                    .expect("the JSON safe-integer maximum is positive");
                assert_eq!(
                    attempt.get("maximum").and_then(serde_json::Value::as_u64),
                    Some(safe_max),
                    "{} advertises a provider attempt its strict JSON reader rejects",
                    schema.display(),
                );
                *attempt_count = (*attempt_count).saturating_add(1);
            }
            for member in members.values() {
                check_schema_bounds(member, schema, branch_count, attempt_count);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                check_schema_bounds(item, schema, branch_count, attempt_count);
            }
        }
        serde_json::Value::Null
        | serde_json::Value::Bool(_)
        | serde_json::Value::Number(_)
        | serde_json::Value::String(_) => {}
    }
}

#[test]
fn public_schemas_share_the_branch_ref_and_integer_bounds() {
    let mut branch_patterns = 0_usize;
    let mut provider_attempts = 0_usize;
    for (_name, schema_path, _example_path) in public_schema_examples() {
        let schema: serde_json::Value =
            serde_json::from_slice(&fs::read(&schema_path).expect("public schema is readable"))
                .expect("public schema is JSON");
        check_schema_bounds(
            &schema,
            &schema_path,
            &mut branch_patterns,
            &mut provider_attempts,
        );
    }
    assert!(
        branch_patterns >= 12,
        "every branch-ref schema projection is checked; found {branch_patterns}"
    );
    assert!(
        provider_attempts >= 4,
        "every provider-run-attempt schema projection is checked; found {provider_attempts}"
    );

    let branch_schema = serde_json::json!({
        "type": "string",
        "minLength": 12,
        "maxLength": 266,
        "pattern": BRANCH_REF_SCHEMA_PATTERN,
    });
    let branch_validator =
        jsonschema::validator_for(&branch_schema).expect("branch-ref schema compiles");
    for reference in ["refs/heads/main", "refs/heads/topic/docs"] {
        assert!(
            BranchRef::new(reference.to_owned()).is_some()
                && branch_validator.is_valid(&serde_json::json!(reference)),
            "the Rust and schema branch-ref grammars must accept {reference}",
        );
    }
    for reference in [
        "refs/heads//topic",
        "refs/heads/topic/",
        "refs/heads/.topic",
        "refs/heads/topic.lock",
        "refs/heads/topic..next",
    ] {
        assert!(
            BranchRef::new(reference.to_owned()).is_none()
                && !branch_validator.is_valid(&serde_json::json!(reference)),
            "the Rust and schema branch-ref grammars must reject {reference}",
        );
    }
}

#[test]
fn active_report_schema_ids_match_the_writer_contract() {
    let schema = report_schema();
    assert_eq!(
        amiss_scan::report::ENVELOPE_SCHEMA,
        ENVELOPE_SCHEMA,
        "the scan and wire envelope writers disagree on the active identity"
    );
    assert_eq!(
        schema
            .pointer("/properties/schema/const")
            .and_then(serde_json::Value::as_str),
        Some(ENVELOPE_SCHEMA),
        "the active schema and writer disagree on the envelope identity"
    );
    assert_eq!(
        schema
            .pointer("/$defs/ReportPayload/properties/schema/const")
            .and_then(serde_json::Value::as_str),
        Some(PAYLOAD_SCHEMA),
        "the active schema and writer disagree on the payload identity"
    );
    assert_eq!(
        schema
            .pointer("/$defs/ReportPayload/properties/compatibility/const")
            .and_then(serde_json::Value::as_str),
        Some(amiss_wire::report::COMPATIBILITY),
        "the active schema and writer disagree on the wire major"
    );
}

#[test]
fn the_policy_schema_tracks_the_reader_bounds() {
    let path = repository_root().join("spec/scanner-policy.schema.json");
    let schema: serde_json::Value =
        serde_json::from_slice(&fs::read(path).expect("scanner policy schema is readable"))
            .expect("scanner policy schema is JSON");
    assert_eq!(
        schema
            .pointer("/$defs/ExactSuffix/maxLength")
            .and_then(serde_json::Value::as_u64),
        u64::try_from(DOCUMENT_SUFFIX_BYTES).ok(),
        "the schema and strict reader must publish one suffix ceiling"
    );
    assert_eq!(
        schema
            .pointer("/$defs/SourceMarker/maxLength")
            .and_then(serde_json::Value::as_u64),
        u64::try_from(SOURCE_MARKER_BYTES).ok(),
        "the schema and strict reader must publish one source-marker ceiling"
    );
    assert_eq!(
        schema
            .pointer("/$defs/RecordKey/maxLength")
            .and_then(serde_json::Value::as_u64),
        u64::try_from(RECORD_KEY_BYTES).ok(),
        "the schema and strict reader must publish one record-key ceiling"
    );
}

#[test]
fn the_semantic_evidence_schema_tracks_the_reader_contract() {
    let path = repository_root().join("spec/scanner-semantic-evidence.schema.json");
    let schema: serde_json::Value =
        serde_json::from_slice(&fs::read(path).expect("semantic evidence schema is readable"))
            .expect("semantic evidence schema is JSON");
    assert_eq!(
        schema
            .pointer("/properties/schema/const")
            .and_then(serde_json::Value::as_str),
        Some(amiss_wire::semantic::ENVELOPE_SCHEMA)
    );
    assert_eq!(
        schema
            .pointer("/$defs/Payload/properties/schema/const")
            .and_then(serde_json::Value::as_str),
        Some(amiss_wire::semantic::PAYLOAD_SCHEMA)
    );
    assert_eq!(
        schema
            .pointer("/$defs/Payload/properties/observations/maxItems")
            .and_then(serde_json::Value::as_u64),
        u64::try_from(amiss_wire::semantic::SEMANTIC_OBSERVATIONS_LIMIT).ok()
    );
    assert_eq!(
        schema
            .pointer("/$defs/Producer/properties/version/maxLength")
            .and_then(serde_json::Value::as_u64),
        u64::try_from(amiss_wire::semantic::PRODUCER_VERSION_BYTES).ok()
    );

    let template_path = repository_root().join("spec/scanner-semantic-template.schema.json");
    let template: serde_json::Value = serde_json::from_slice(
        &fs::read(template_path).expect("semantic template schema is readable"),
    )
    .expect("semantic template schema is JSON");
    assert_eq!(
        template
            .pointer("/properties/schema/const")
            .and_then(serde_json::Value::as_str),
        Some(amiss_wire::semantic::TEMPLATE_SCHEMA)
    );
    assert_eq!(
        template
            .pointer("/properties/observations/maxItems")
            .and_then(serde_json::Value::as_u64),
        u64::try_from(amiss_wire::semantic::SEMANTIC_OBSERVATIONS_LIMIT).ok()
    );
    assert_eq!(
        template
            .pointer("/$defs/Producer/properties/version/maxLength")
            .and_then(serde_json::Value::as_u64),
        u64::try_from(amiss_wire::semantic::PRODUCER_VERSION_BYTES).ok()
    );

    let request_path = repository_root().join("spec/scanner-controls-request.schema.json");
    let request: serde_json::Value = serde_json::from_slice(
        &fs::read(request_path).expect("controls request schema is readable"),
    )
    .expect("controls request schema is JSON");
    let limit = u64::try_from(amiss_wire::requests::SEMANTIC_EVIDENCE_REQUEST_LIMIT).ok();
    assert_eq!(
        request
            .pointer("/properties/semantic_evidence/maxItems")
            .and_then(serde_json::Value::as_u64),
        limit
    );
    assert_eq!(
        report_schema()
            .pointer("/$defs/ResolvedControls/properties/semantic_evidence/maxItems")
            .and_then(serde_json::Value::as_u64),
        limit
    );
}

#[test]
fn all_public_contract_examples_clear_their_schema_and_registered_reader() {
    let mut defects = Vec::new();

    for (contract_name, schema_path, example_path) in public_schema_examples() {
        let schema_bytes = fs::read(&schema_path).expect("public schema is readable");
        let example_bytes = fs::read(&example_path).expect("public example is readable");
        let schema: serde_json::Value = serde_json::from_slice(&schema_bytes)
            .unwrap_or_else(|error| panic!("{} is not JSON: {error}", schema_path.display()));
        let example: serde_json::Value = serde_json::from_slice(&example_bytes)
            .unwrap_or_else(|error| panic!("{} is not JSON: {error}", example_path.display()));
        let validator = jsonschema::validator_for(&schema)
            .unwrap_or_else(|error| panic!("{} does not compile: {error}", schema_path.display()));

        defects.extend(validator.iter_errors(&example).map(|error| {
            format!(
                "{} against {} at {}: {error}",
                example_path.display(),
                schema_path.display(),
                error.instance_path(),
            )
        }));

        if let Some(error) = example_reader_defect(&contract_name, &example_bytes) {
            defects.push(format!(
                "{} was rejected by the {contract_name} example reader: {error}",
                example_path.display(),
            ));
        }
    }

    assert!(
        defects.is_empty(),
        "public contract examples violate their schemas or registered readers:\n{}",
        defects.join("\n"),
    );
}

/// The example the last release shipped, refreshed by the release workflow,
/// must keep clearing the rolling schema and reader: additions leave it
/// clean, so a failure here is a payload reshape, which the frozen major
/// forbids. The one lawful mismatch is the founding window: the last
/// release before the freeze wrote `experimental`, the reader must still
/// accept it, and the next release refresh restores the full check.
#[test]
fn the_last_released_example_still_clears_the_rolling_contract() {
    let root = repository_root();
    let example_bytes = fs::read(root.join("spec/examples/scanner-report.last-released.json"))
        .expect("the last released example is readable");
    let example: serde_json::Value =
        serde_json::from_slice(&example_bytes).expect("the last released example is JSON");
    let mut defects = Vec::new();
    match example
        .pointer("/payload/compatibility")
        .and_then(serde_json::Value::as_str)
    {
        Some(released) if released == amiss_wire::report::COMPATIBILITY => {
            let schema = report_schema();
            let validator = jsonschema::validator_for(&schema).expect("schema compiles");
            defects.extend(
                validator
                    .iter_errors(&example)
                    .map(|error| format!("at {}: {error}", error.instance_path())),
            );
        }
        Some("experimental") => assert_eq!(
            amiss_wire::report::COMPATIBILITY,
            "1",
            "only the founding freeze may follow an experimental release",
        ),
        released => {
            panic!("the last released example carries no lawful wire version: {released:?}")
        }
    }
    if let Some(error) = example_reader_defect("scanner-report", &example_bytes) {
        defects.push(format!("rejected by the report reader: {error}"));
    }
    assert!(
        defects.is_empty(),
        "the last released example no longer clears the rolling contract; \
         this is a payload reshape, which the frozen major forbids:\n{}",
        defects.join("\n"),
    );
}

/// The first frozen example, retained permanently at the moment the wire
/// froze: every later schema in the major must still validate it, and the
/// bytes themselves never change. Reshaping past this fixture mints `2`,
/// and that release is a major one.
#[test]
fn the_first_frozen_example_binds_the_major() {
    let root = repository_root();
    let bytes = fs::read(root.join("spec/examples/scanner-report.frozen-1.json"))
        .expect("the frozen example is readable");
    let mut retained = String::with_capacity(64);
    for byte in <sha2::Sha256 as sha2::Digest>::digest(&bytes) {
        use std::fmt::Write as _;
        write!(&mut retained, "{byte:02x}").expect("writing to a string is infallible");
    }
    assert_eq!(
        retained, "3fff8892cabc5bf6a9aae730ed11ac37f6c96ecd1efbc3d04786367d36f39d7a",
        "the frozen fixture is permanent; a new major mints a new fixture instead",
    );
    let example: serde_json::Value =
        serde_json::from_slice(&bytes).expect("the frozen example is JSON");
    assert_eq!(
        example
            .pointer("/payload/compatibility")
            .and_then(serde_json::Value::as_str),
        Some("1"),
        "the frozen fixture opens the major it binds",
    );
    let schema = report_schema();
    let validator = jsonschema::validator_for(&schema).expect("schema compiles");
    let mut defects: Vec<String> = validator
        .iter_errors(&example)
        .map(|error| format!("at {}: {error}", error.instance_path()))
        .collect();
    if let Some(error) = example_reader_defect("scanner-report", &bytes) {
        defects.push(format!("rejected by the report reader: {error}"));
    }
    assert!(
        defects.is_empty(),
        "the rolling contract no longer accepts the first frozen example; \
         this reshape mints wire major 2 and a major release:\n{}",
        defects.join("\n"),
    );
}

/// The plan example is not authored, it is derived: feeding the report
/// example through the real derivation, with the engine values the plan
/// example itself carries, must reproduce it byte-equal at the value level.
/// A drift in either example, the derivation, or the digest fails here.
#[test]
fn the_external_plan_example_derives_from_the_report_example() {
    let root = repository_root();
    let report_bytes = fs::read(root.join("spec/examples/scanner-report.json"))
        .expect("the report example is readable");
    let report = amiss_wire::json::parse(&report_bytes).expect("the report example is strict JSON");
    let plan_bytes = fs::read(root.join("spec/examples/scanner-external-plan.json"))
        .expect("the plan example is readable");
    let plan = amiss_wire::json::parse(&plan_bytes).expect("the plan example is strict JSON");
    let example: serde_json::Value =
        serde_json::from_slice(&plan_bytes).expect("the plan example is JSON");
    let version = example
        .pointer("/payload/engine/engine_version")
        .and_then(serde_json::Value::as_str)
        .expect("the plan example names an engine version");
    let digest = example
        .pointer("/payload/engine/engine_digest")
        .and_then(serde_json::Value::as_str)
        .expect("the plan example names an engine digest");
    let derived = amiss_wire::external::plan(&report, version, digest)
        .expect("the report example yields a plan");
    assert_eq!(
        derived, plan,
        "the plan example drifted from its own derivation"
    );
}

/// The assessment example is not authored either: judging the plan example
/// against the evidence example through the real code path, with the engine
/// values the assessment example itself carries, must reproduce it exactly.
#[test]
fn the_assessment_example_derives_from_the_plan_and_evidence_examples() {
    let root = repository_root();
    let plan_bytes = fs::read(root.join("spec/examples/scanner-external-plan.json"))
        .expect("the plan example is readable");
    let plan = amiss_wire::json::parse(&plan_bytes).expect("the plan example is strict JSON");
    let evidence_bytes = fs::read(root.join("spec/examples/scanner-external-evidence.json"))
        .expect("the evidence example is readable");
    let evidence =
        amiss_wire::json::parse(&evidence_bytes).expect("the evidence example is strict JSON");
    let assessment_bytes = fs::read(root.join("spec/examples/scanner-external-assessment.json"))
        .expect("the assessment example is readable");
    let assessment =
        amiss_wire::json::parse(&assessment_bytes).expect("the assessment example is strict JSON");
    let example: serde_json::Value =
        serde_json::from_slice(&assessment_bytes).expect("the assessment example is JSON");
    let version = example
        .pointer("/payload/engine/engine_version")
        .and_then(serde_json::Value::as_str)
        .expect("the assessment example names an engine version");
    let digest = example
        .pointer("/payload/engine/engine_digest")
        .and_then(serde_json::Value::as_str)
        .expect("the assessment example names an engine digest");
    let derived = amiss_wire::external::assess(&plan, &evidence, version, digest)
        .expect("the plan and evidence examples yield an assessment");
    assert_eq!(
        derived, assessment,
        "the assessment example drifted from its own derivation"
    );
}

#[test]
fn the_semantic_evidence_example_matches_its_checked_writer() {
    let root = repository_root();
    let bytes = fs::read(root.join("spec/examples/scanner-semantic-evidence.json"))
        .expect("the semantic evidence example is readable");
    let parsed = amiss_wire::semantic::parse(&bytes)
        .expect("the semantic evidence example clears the strict reader");
    let written = amiss_wire::semantic::envelope(parsed.payload)
        .expect("the semantic evidence example clears the checked writer");
    let example =
        amiss_wire::json::parse(&bytes).expect("the semantic evidence example is strict JSON");
    assert_eq!(
        amiss_wire::json::canonical(&written),
        amiss_wire::json::canonical(&example),
        "the semantic evidence example drifted from its writer"
    );
}

#[test]
fn report_example_is_schema_clean_and_matches_its_canonical_form() {
    let root = repository_root();
    let pretty = fs::read(root.join("spec/examples/scanner-report.json"))
        .expect("pretty report example is readable");
    let canonical_fixture = fs::read(root.join("spec/examples/scanner-report.canonical.json"))
        .expect("canonical report example is readable");

    let parsed = amiss_wire::json::parse(&pretty).expect("pretty example is strict JSON");
    let mut canonical = amiss_wire::json::canonical(&parsed);
    canonical.push(b'\n');
    assert_eq!(
        canonical, canonical_fixture,
        "pretty and canonical report examples drifted"
    );

    let schema = report_schema();
    let example: serde_json::Value =
        serde_json::from_slice(&pretty).expect("report example is JSON");
    let validator = jsonschema::validator_for(&schema).expect("report schema compiles");
    let defects: Vec<String> = validator
        .iter_errors(&example)
        .map(|error| format!("{}: {error}", error.instance_path()))
        .collect();
    assert_eq!(
        defects,
        Vec::<String>::new(),
        "report example violates its schema"
    );

    let payload = &example["payload"];
    for row in payload["errors"].as_array().expect("errors is an array") {
        let code = row["code"].as_str().expect("an error row names its code");
        let meaning = AnalysisErrorCode::all()
            .find(|candidate| candidate.as_ref() == code)
            .expect("the example uses schema error codes")
            .meaning();
        assert_eq!(
            row["description"], meaning,
            "the example description for {code} drifted from the engine text"
        );
    }
    for row in payload["findings"]
        .as_array()
        .expect("findings is an array")
    {
        let kind = row["kind"].as_str().expect("a finding row names its kind");
        let meaning = FindingKind::all()
            .find(|candidate| candidate.as_ref() == kind)
            .expect("the example uses schema finding kinds")
            .meaning();
        assert_eq!(
            row["description"], meaning,
            "the example description for {kind} drifted from the engine text"
        );
    }
}
