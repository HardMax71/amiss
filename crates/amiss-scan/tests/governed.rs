#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "integration assertions over a repository-owned semantic corpus"
)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use amiss_scan::report::{CandidateBlock, RequestDigests, Setup, SnapshotIdentity, construct};
use amiss_scan::{
    Classification, DocumentRecord, DocumentStatus, ScanLimits, ScanResources, SnapshotDiscovery,
    scan_document,
};
use amiss_wire::controls::GitMode;
use amiss_wire::digest::{Digest, RAW_EVIDENCE_DOMAIN, hb};
use amiss_wire::json::parse;
use amiss_wire::model::{Adapter, ObjectFormat, Oid, RepoPath};
use amiss_wire::report::EngineProvenance;
use serde_json::{Map, Value};

mod support;

use support::assert_report as assert_schema_clean;

const DOCUMENT_PATH: &str = "docs/governed.md";
const EXPECTED_IDS: [&str; 8] = [
    "GD-001-canonical-candidate",
    "GD-002-decoded-colon",
    "GD-003-uppercase-not-reserved",
    "GD-004-duplicate-source-multiplicity",
    "GD-005-base-only-does-not-emit",
    "GD-006-losing-reserved-does-not-suppress",
    "GD-007-backslash-decoded-colon",
    "GD-008-distinct-sources-sort-by-digest",
];

fn corpus() -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/examples/governed-definition-vectors.json");
    let bytes = fs::read(&path).expect("the governed-definition corpus is readable");
    parse(&bytes).expect("the corpus clears the strict JSON reader");
    serde_json::from_slice(&bytes).expect("strict JSON is available to the test harness")
}

fn object<'a>(value: &'a Value, context: &str) -> &'a Map<String, Value> {
    value
        .as_object()
        .unwrap_or_else(|| panic!("{context} must be an object, found {value:?}"))
}

fn array<'a>(value: &'a Value, context: &str) -> &'a [Value] {
    value.as_array().map_or_else(
        || panic!("{context} must be an array, found {value:?}"),
        Vec::as_slice,
    )
}

fn member<'a>(members: &'a Map<String, Value>, name: &str, context: &str) -> &'a Value {
    members
        .get(name)
        .unwrap_or_else(|| panic!("{context} lacks {name}"))
}

fn text<'a>(members: &'a Map<String, Value>, name: &str, context: &str) -> &'a str {
    member(members, name, context)
        .as_str()
        .unwrap_or_else(|| panic!("{context}.{name} must be a string"))
}

fn unsigned(members: &Map<String, Value>, name: &str, context: &str) -> u64 {
    member(members, name, context)
        .as_u64()
        .unwrap_or_else(|| panic!("{context}.{name} must be an unsigned integer"))
}

fn exact_keys(members: &Map<String, Value>, required: &[&str], optional: &[&str], context: &str) {
    for name in required {
        assert!(members.contains_key(*name), "{context} lacks {name}");
    }
    let mut expected: Vec<&str> = required.iter().chain(optional).copied().collect();
    expected.sort_unstable();
    let mut actual: Vec<&str> = members.keys().map(String::as_str).collect();
    actual.sort_unstable();
    for name in &actual {
        assert!(
            expected.binary_search(name).is_ok(),
            "{context} has unknown field {name}"
        );
    }
}

fn definitions<'a>(case: &'a Map<String, Value>, side: &str, id: &str) -> &'a [Value] {
    let context = format!("{id}.{side}");
    let rows = array(member(case, side, id), &context);
    for (index, row) in rows.iter().enumerate() {
        let row_context = format!("{context}[{index}]");
        let definition = object(row, &row_context);
        exact_keys(definition, &["source"], &[], &row_context);
        assert!(
            !text(definition, "source", &row_context).is_empty(),
            "{row_context}.source must not be empty"
        );
    }
    rows
}

fn labels<'a>(case: &'a Map<String, Value>, id: &str) -> Vec<&'a str> {
    let Some(raw) = case.get("consuming_normalized_labels") else {
        assert!(
            !case.contains_key("expected_ordinary_reference_count"),
            "{id} cannot expect consumers without naming them"
        );
        return Vec::new();
    };
    assert!(
        case.contains_key("expected_ordinary_reference_count"),
        "{id} names consumers without their expected count"
    );
    let context = format!("{id}.consuming_normalized_labels");
    let labels: Vec<&str> = array(raw, &context)
        .iter()
        .enumerate()
        .map(|(index, value)| {
            value
                .as_str()
                .unwrap_or_else(|| panic!("{context}[{index}] must be a string"))
        })
        .collect();
    let unique: BTreeSet<&str> = labels.iter().copied().collect();
    assert_eq!(unique.len(), labels.len(), "{id} repeats a consumer label");
    labels
}

fn source(definitions: &[Value], labels: &[&str], context: &str) -> String {
    let mut lines: Vec<String> = labels
        .iter()
        .enumerate()
        .map(|(index, label)| format!("[consumer-{index}][{label}]"))
        .collect();
    if !lines.is_empty() && !definitions.is_empty() {
        lines.push(String::new());
    }
    lines.extend(definitions.iter().enumerate().map(|(index, value)| {
        text(
            object(value, &format!("{context}[{index}]")),
            "source",
            context,
        )
        .to_owned()
    }));
    if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    }
}

fn scanned(source: &str) -> amiss_scan::Scanned {
    scan_document(
        &mut ScanResources::new(ScanLimits::CONTRACT),
        Adapter::Markdown,
        source.as_bytes(),
    )
    .expect("the vector source scans")
}

fn discovery(scanned: amiss_scan::Scanned, source: &str, oid_digit: char) -> SnapshotDiscovery {
    let oid = Oid::new(ObjectFormat::Sha1, oid_digit.to_string().repeat(40))
        .expect("the synthetic blob identity is valid");
    let path = RepoPath::new(DOCUMENT_PATH.to_owned()).expect("the fixture path is valid");
    let mut entries = BTreeMap::new();
    entries.insert(path.clone(), (GitMode::RegularFile, oid.clone()));
    SnapshotDiscovery {
        documents: vec![DocumentRecord {
            path,
            classification: Classification::StructuredMarkdown,
            status: DocumentStatus::Scanned(scanned),
            oid,
            mode: GitMode::RegularFile,
            byte_count: u64::try_from(source.len()).expect("the fixture length fits u64"),
            raw_digest: Some(hb(RAW_EVIDENCE_DOMAIN, source.as_bytes())),
        }],
        outside_document_set: 0,
        tree_entries: 1,
        path_defects: Vec::new(),
        entries,
    }
}

fn setup() -> Setup {
    let base = SnapshotIdentity {
        object_format: "sha1",
        commit_oid: "a".repeat(40),
        tree_oid: "b".repeat(40),
    };
    let candidate = SnapshotIdentity {
        object_format: "sha1",
        commit_oid: "c".repeat(40),
        tree_oid: "d".repeat(40),
    };
    Setup {
        engine: EngineProvenance {
            version: "0.0.0-test".to_owned(),
            digest: hb("amiss/scanner-engine", b"governed corpus test"),
        },
        enforce: false,
        repository: None,
        forge: None,
        candidate_ref: None,
        default_branch_ref: None,
        base,
        candidate: CandidateBlock::Commit(candidate),
        policy: amiss_scan::Effects::default(),
        controls_unavailable: None,
        requests: RequestDigests::default(),
    }
}

fn expected_sources(case: &Map<String, Value>, id: &str) -> Vec<(String, u64)> {
    let context = format!("{id}.expected_sources");
    let mut sources: Vec<(String, u64)> = array(member(case, "expected_sources", id), &context)
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let row_context = format!("{context}[{index}]");
            let row = object(value, &row_context);
            exact_keys(row, &["digest", "multiplicity"], &[], &row_context);
            let digest = text(row, "digest", &row_context);
            assert!(
                Digest::from_wire(digest).is_some(),
                "{row_context}.digest is not a canonical digest"
            );
            let multiplicity = unsigned(row, "multiplicity", &row_context);
            assert!(
                multiplicity > 0,
                "{row_context}.multiplicity must be positive"
            );
            (digest.to_owned(), multiplicity)
        })
        .collect();
    let before = sources.clone();
    sources.sort();
    assert_eq!(before, sources, "{id} expected sources are not sorted");
    sources
}

fn actual_sources(finding: &Value, id: &str) -> Vec<(String, u64)> {
    let sources = finding
        .pointer("/candidate_fact/evidence/candidate_control_state/sources")
        .unwrap_or_else(|| panic!("{id} governed finding lacks candidate sources"));
    array(sources, &format!("{id}.emitted_sources"))
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let context = format!("{id}.emitted_sources[{index}]");
            let row = object(value, &context);
            (
                text(row, "digest", &context).to_owned(),
                unsigned(row, "multiplicity", &context),
            )
        })
        .collect()
}

fn assert_governed_boundary(
    id: &str,
    result: &Value,
    errors: &[Value],
    finding: &Value,
    expected_member_count: u64,
    expected_sources: &[(String, u64)],
) {
    assert_eq!(result.get("complete"), Some(&Value::Bool(false)), "{id}");
    assert_eq!(errors.len(), 1, "{id} emits one governed analysis error");
    let error = errors.first().expect("one governed analysis error exists");
    assert_eq!(
        error.get("code").and_then(Value::as_str),
        Some("UNSUPPORTED_CAPABILITY")
    );
    assert_eq!(
        error.get("path").and_then(Value::as_str),
        Some(DOCUMENT_PATH)
    );
    assert_eq!(error.get("phase").and_then(Value::as_str), Some("policy"));
    assert_eq!(
        finding
            .pointer("/key_input/scope/kind")
            .and_then(Value::as_str),
        Some("control")
    );
    assert_eq!(
        finding
            .pointer("/key_input/scope/control_path")
            .and_then(Value::as_str),
        Some(DOCUMENT_PATH)
    );
    let state = finding
        .pointer("/candidate_fact/evidence/candidate_control_state")
        .expect("the governed finding carries candidate control state");
    assert_eq!(
        state.get("schema").and_then(Value::as_str),
        Some("amiss/scanner-control-state")
    );
    assert_eq!(
        state.get("rule_id").and_then(Value::as_str),
        Some("unsupported/governed-claim")
    );
    assert_eq!(
        state.get("path").and_then(Value::as_str),
        Some(DOCUMENT_PATH)
    );
    assert_eq!(
        state.get("state").and_then(Value::as_str),
        Some("unsupported")
    );
    assert_eq!(
        finding
            .pointer("/aggregation/member_count")
            .and_then(Value::as_u64),
        Some(expected_member_count),
        "{id} report member count"
    );
    assert_eq!(actual_sources(finding, id), expected_sources, "{id}");
}

fn assert_report(
    case: &Map<String, Value>,
    id: &str,
    base_scanned: amiss_scan::Scanned,
    candidate_scanned: amiss_scan::Scanned,
    base_source: &str,
    candidate_source: &str,
    expected_member_count: u64,
) {
    let expected_sources = expected_sources(case, id);
    assert_eq!(
        expected_sources
            .iter()
            .map(|(_, multiplicity)| *multiplicity)
            .sum::<u64>(),
        expected_member_count,
        "{id} source multiplicities must cover every governed member"
    );
    let base = discovery(base_scanned, base_source, '1');
    let candidate = discovery(candidate_scanned, candidate_source, '2');
    let built = construct(&setup(), &base, &candidate, &[]);
    let wire = built.wire();
    parse(&wire).expect("the emitted report clears the strict JSON reader");
    let envelope: Value = serde_json::from_slice(&wire).expect("the emitted report is JSON");
    assert_schema_clean(&envelope, id);
    let result = envelope
        .pointer("/payload/result")
        .expect("the report payload carries a result");
    let errors = envelope
        .pointer("/payload/errors")
        .and_then(Value::as_array)
        .expect("the report payload carries errors");
    let findings = envelope
        .pointer("/payload/findings")
        .expect("the report payload carries findings")
        .as_array()
        .expect("the report carries findings");
    let governed: Vec<&Value> = findings
        .iter()
        .filter(|finding| {
            finding.get("kind").and_then(Value::as_str) == Some("unsupported-capability")
                && finding
                    .pointer("/key_input/scope/rule_id")
                    .and_then(Value::as_str)
                    == Some("unsupported/governed-claim")
        })
        .collect();

    if expected_member_count == 0 {
        assert!(governed.is_empty(), "{id} emitted a governed boundary");
        assert_eq!(result.get("complete"), Some(&Value::Bool(true)), "{id}");
        assert!(errors.is_empty(), "{id} emitted a governed analysis error");
        assert_eq!(built.status, "pass", "{id} must remain complete");
        assert_eq!(built.exit_code, 0, "{id} must remain complete");
        return;
    }
    assert_eq!(governed.len(), 1, "{id} must emit one path-scoped boundary");
    let finding = governed.first().expect("one governed finding exists");
    assert_governed_boundary(
        id,
        result,
        errors,
        finding,
        expected_member_count,
        &expected_sources,
    );
    assert_eq!(built.status, "incomplete", "{id} boundary status");
    assert_eq!(built.exit_code, 2, "{id} boundary exit class");
}

fn run_case(index: usize, value: &Value) {
    let context = format!("cases[{index}]");
    let case = object(value, &context);
    exact_keys(
        case,
        &[
            "id",
            "base_definitions",
            "candidate_definitions",
            "expected_member_count",
            "expected_sources",
        ],
        &[
            "consuming_normalized_labels",
            "expected_ordinary_reference_count",
        ],
        &context,
    );
    let id = text(case, "id", &context);
    let base_definitions = definitions(case, "base_definitions", id);
    let candidate_definitions = definitions(case, "candidate_definitions", id);
    let consuming_labels = labels(case, id);
    let base_source = source(base_definitions, &[], &format!("{id}.base_definitions"));
    let candidate_source = source(
        candidate_definitions,
        &consuming_labels,
        &format!("{id}.candidate_definitions"),
    );
    let base_scanned = scanned(&base_source);
    let candidate_scanned = scanned(&candidate_source);
    let expected_member_count = unsigned(case, "expected_member_count", id);
    assert_eq!(
        u64::try_from(candidate_scanned.governed.len()).expect("count fits u64"),
        expected_member_count,
        "{id} governed parser count"
    );
    let expected_ordinary = case
        .get("expected_ordinary_reference_count")
        .map_or(0, |value| {
            value
                .as_u64()
                .unwrap_or_else(|| panic!("{id}.expected_ordinary_reference_count is invalid"))
        });
    assert_eq!(
        u64::try_from(candidate_scanned.occurrences.len()).expect("count fits u64"),
        expected_ordinary,
        "{id} ordinary consumer count"
    );
    if id == "GD-005-base-only-does-not-emit" {
        assert_eq!(
            base_scanned.governed.len(),
            1,
            "the base-only proof must contain a real governed node"
        );
        assert!(candidate_scanned.governed.is_empty());
    }
    assert_report(
        case,
        id,
        base_scanned,
        candidate_scanned,
        &base_source,
        &candidate_source,
        expected_member_count,
    );
}

#[test]
fn the_governed_definition_corpus_reproduces_the_runtime_boundary() {
    let fixture = corpus();
    let root = object(&fixture, "governed-definition corpus");
    exact_keys(
        root,
        &["schema", "contract", "cases"],
        &[],
        "governed-definition corpus",
    );
    assert_eq!(
        text(root, "schema", "governed-definition corpus"),
        "amiss/governed-definition-vectors"
    );
    assert_eq!(
        text(root, "contract", "governed-definition corpus"),
        "governed-definition-source"
    );
    let cases = array(member(root, "cases", "governed-definition corpus"), "cases");
    let ids: Vec<&str> = cases
        .iter()
        .enumerate()
        .map(|(index, value)| text(object(value, &format!("cases[{index}]")), "id", "case"))
        .collect();
    assert_eq!(ids, EXPECTED_IDS, "the corpus case set or order drifted");
    let unique: BTreeSet<&str> = ids.iter().copied().collect();
    assert_eq!(unique.len(), ids.len(), "the corpus repeats a case ID");
    for (index, value) in cases.iter().enumerate() {
        run_case(index, value);
    }
}
