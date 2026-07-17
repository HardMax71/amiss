#![expect(
    clippy::panic,
    reason = "integration harness over asserted fixture shapes"
)]

use std::alloc::System;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use amiss_wire::digest::{hb, hj};
use amiss_wire::json::{Value, canonical_length};
use amiss_wire::report::{
    AnalysisErrorCode, EngineProvenance, FATAL_SCRATCH_BYTES, FatalSerializer, PAYLOAD_SCHEMA,
    unavailable_evaluation_wire,
};
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

const WIRE_CAP: usize = 67_108_864;

fn string(text: &str) -> Value {
    Value::String(text.to_owned())
}

fn object(members: Vec<(&str, Value)>) -> Value {
    Value::Object(
        members
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    )
}

fn set_member(value: &mut Value, key: &str, replacement: Value) {
    let Value::Object(members) = value else {
        panic!("expected an object at {key}");
    };
    let slot = members
        .iter_mut()
        .find(|(name, _)| name == key)
        .unwrap_or_else(|| panic!("missing member {key}"));
    slot.1 = replacement;
}

fn member_mut<'value>(value: &'value mut Value, key: &str) -> &'value mut Value {
    let Value::Object(members) = value else {
        panic!("expected an object at {key}");
    };
    &mut members
        .iter_mut()
        .find(|(name, _)| name == key)
        .unwrap_or_else(|| panic!("missing member {key}"))
        .1
}

/// A maximal schema-valid `RepoPath`: 4,096 characters dominated by quotes,
/// the densest escaping the path grammar can reach on the wire, prefixed for
/// uniqueness and byte order.
fn maximal_path(index: usize) -> String {
    format!("{index:04}{}", "\"".repeat(4_092))
}

const DIGEST: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn runtime_file(index: usize) -> Value {
    object(vec![
        ("path", string(&maximal_path(index))),
        ("role", string("dynamic-library")),
        ("git_mode", string("100755")),
        ("file_sha256", string(DIGEST)),
    ])
}

fn artifact(platform: &str) -> Value {
    let files: Vec<Value> = (0..256).map(runtime_file).collect();
    object(vec![
        ("platform", string(platform)),
        (
            "artifact_name",
            string(&format!("amiss-{platform}{}", ".x".repeat(60))),
        ),
        ("tree_path", string(&maximal_path(9_000))),
        ("binary_sha256", string(DIGEST)),
        ("engine_digest", string(DIGEST)),
        ("runtime_contract", string("manifest-closed")),
        ("environment_contract", string("scanner-process-env")),
        ("runtime_files", Value::Array(files)),
    ])
}

/// The schema-maximum forge-action provenance: six artifacts of 256 runtime
/// files each (the 1,536 runtime paths of the paper bound), 32 dependency
/// lock files, and every bounded field at its widest.
fn maximal_provenance() -> Value {
    let platforms = [
        "linux-aarch64",
        "linux-x86_64",
        "macos-aarch64",
        "macos-x86_64",
        "windows-aarch64",
        "windows-x86_64",
    ];
    let artifacts: Vec<Value> = platforms.iter().map(|name| artifact(name)).collect();
    let locks: Vec<Value> = (0..32)
        .map(|index| {
            object(vec![
                ("path", string(&maximal_path(index))),
                ("raw_digest", string(DIGEST)),
            ])
        })
        .collect();
    let manifest = object(vec![
        ("schema", string("amiss/scanner-release-manifest")),
        (
            "engine_version",
            string(&format!("100.200.300-{}", "a".repeat(52))),
        ),
        (
            "build_source",
            object(vec![
                (
                    "repository",
                    object(vec![
                        ("host", string("git.example.internal")),
                        (
                            "owner",
                            string(&format!("{}/{}", "o".repeat(100), "g".repeat(100))),
                        ),
                        ("name", string(&"n".repeat(100))),
                    ]),
                ),
                ("object_format", string("sha256")),
                ("commit_oid", string(&"a".repeat(64))),
            ]),
        ),
        (
            "dependency_lock",
            object(vec![
                ("schema", string("amiss/scanner-dependency-lock-input")),
                ("files", Value::Array(locks)),
            ]),
        ),
        ("dependency_lock_digest", string(DIGEST)),
        ("artifacts", Value::Array(artifacts)),
    ]);
    object(vec![
        ("kind", string("forge-action")),
        (
            "action_repository",
            object(vec![
                ("host", string("git.example.internal")),
                (
                    "owner",
                    string(&format!("{}/{}", "o".repeat(100), "g".repeat(100))),
                ),
                ("name", string(&"n".repeat(100))),
            ]),
        ),
        ("action_object_format", string("sha256")),
        ("action_commit_oid", string(&"a".repeat(64))),
        ("action_tree_oid", string(&"b".repeat(64))),
        ("dependency_lock_digest", string(DIGEST)),
        ("release_manifest", manifest),
        ("release_manifest_digest", string(DIGEST)),
        ("manifest_path", string(&maximal_path(9_001))),
        ("selected_platform", string("linux-x86_64")),
        (
            "selected_artifact_name",
            string(&format!("amiss-linux-x86_64{}", ".x".repeat(60))),
        ),
    ])
}

/// One maximal retained error row: the widest phase and code strings, a
/// 4,096-character quote-dense path, the full 8,192-character byte hex, the
/// longest resource name, and safe-integer limits.
fn maximal_error(index: usize) -> Value {
    object(vec![
        ("phase", string("configuration")),
        ("code", string("RESOURCE_LIMIT_EXCEEDED")),
        ("description", string(&"d".repeat(400))),
        ("path", string(&maximal_path(index))),
        ("path_bytes_hex", string(&"ab".repeat(4_096))),
        (
            "resource",
            string("aggregate-git-compressed-object-bytes-per-evaluation"),
        ),
        ("configured_limit", Value::Integer(9_007_199_254_740_991)),
        (
            "observed_lower_bound",
            Value::Integer(9_007_199_254_740_991),
        ),
    ])
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn assert_schema_valid(wire: &[u8]) {
    let schema_text = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/scanner-report.schema.json"),
    )
    .unwrap();
    let schema_json: serde_json::Value = serde_json::from_str(&schema_text).unwrap();
    let validator = jsonschema::validator_for(&schema_json).unwrap();
    let envelope_json: serde_json::Value = serde_json::from_slice(wire).unwrap();
    let defects: Vec<String> = validator
        .iter_errors(&envelope_json)
        .map(|error| format!("{}: {error}", error.instance_path()))
        .collect();
    assert_eq!(
        defects,
        Vec::<String>::new(),
        "the maximal golden is schema-valid"
    );
}

#[test]
fn the_maximal_fatal_envelope_fits_the_wire_reservation() {
    let engine = EngineProvenance {
        version: format!("100.200.300-{}", "a".repeat(52)),
        digest: hb("amiss/scanner-engine", b"maximal golden"),
    };
    let codes: BTreeSet<AnalysisErrorCode> = [
        AnalysisErrorCode::InvalidInvocation,
        AnalysisErrorCode::InvalidEvent,
        AnalysisErrorCode::InvalidProfile,
        AnalysisErrorCode::RequestUnreadable,
    ]
    .into_iter()
    .collect();
    let request_digest = hb("amiss/scanner-evaluation-request", b"maximal");
    let base_wire =
        unavailable_evaluation_wire(&engine, &codes, Some(request_digest), Some(request_digest))
            .unwrap();
    let trimmed = base_wire.strip_suffix(b"\n").unwrap();
    let envelope = amiss_wire::json::parse(trimmed).unwrap();

    let Value::Object(mut envelope_members) = envelope else {
        panic!("envelope is an object");
    };
    let payload = &mut envelope_members
        .iter_mut()
        .find(|(name, _)| name == "payload")
        .unwrap()
        .1;
    set_member(
        member_mut(payload, "engine"),
        "action_provenance",
        maximal_provenance(),
    );
    set_member(
        member_mut(payload, "controls"),
        "reasons",
        Value::Array(
            [
                "not-parsed",
                "invalid-profile",
                "invalid-repository-policy",
                "invalid-external-control",
                "control-binding-mismatch",
            ]
            .iter()
            .map(|reason| string(reason))
            .collect(),
        ),
    );
    let errors: Vec<Value> = (0..64).map(maximal_error).collect();
    set_member(payload, "errors", Value::Array(errors));
    set_member(
        member_mut(payload, "result"),
        "error_count",
        Value::Integer(64),
    );
    let payload_digest = hj(PAYLOAD_SCHEMA, payload);

    let mut maximal = Value::Object(envelope_members);
    set_member(
        &mut maximal,
        "payload_digest",
        string(&payload_digest.to_string()),
    );
    let mut wire = amiss_wire::json::canonical(&maximal);
    wire.push(b'\n');

    assert_schema_valid(&wire);

    assert!(
        wire.len() < WIRE_CAP,
        "the fatal-incomplete wire fits the 64 MiB reservation: {} bytes",
        wire.len()
    );
    assert!(
        wire.len() > 1_536 * 8_192,
        "the golden genuinely carries the 1,536 maximal runtime paths: {} bytes",
        wire.len()
    );

    let paper_paths: usize = 1_536 + 32 + 64 + 6 + 2;
    let paper_bound = paper_paths * 24_576 + 64 * 8_192 + 16_777_216;
    assert!(
        paper_bound < WIRE_CAP,
        "the documented worst-case decomposition stays under the reservation"
    );
    assert!(
        wire.len() < paper_bound,
        "the actual serializer stays under the paper bound: {} < {paper_bound}",
        wire.len()
    );

    prove_streamed_emission(&maximal, &wire);
}

/// The scratch-bound half of the E0 golden: the streamed emission is
/// byte-identical to the materialized wire, the counting pass reports the
/// exact length, and one maximal emission allocates at most the fixed
/// scratch beyond the reserve.
#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn prove_streamed_emission(maximal: &Value, wire: &[u8]) {
    assert_eq!(
        canonical_length(maximal).saturating_add(1),
        u64::try_from(wire.len()).unwrap(),
        "the counting canonical-serialization pass reports the exact wire length"
    );

    let mut reserve = FatalSerializer::new();
    let mut streamed: Vec<u8> = Vec::with_capacity(wire.len());
    let region = Region::new(GLOBAL);
    let emitted = reserve.emit(maximal, &mut streamed).unwrap();
    let stats = region.change();
    assert_eq!(
        emitted,
        u64::try_from(wire.len()).unwrap(),
        "the streamed emission reports the exact wire length"
    );
    assert_eq!(
        streamed, wire,
        "the streamed wire is byte-identical to the materialized wire"
    );
    let scratch = stats
        .bytes_allocated
        .saturating_add(usize::try_from(stats.bytes_reallocated).unwrap_or(0));
    assert!(
        scratch <= FATAL_SCRATCH_BYTES,
        "one maximal emission allocates at most the fixed scratch: {scratch} bytes over {} allocations",
        stats.allocations
    );
}

/// The union path definition, exercised through the same validator the suite
/// uses, so the pair-aligned lookahead pattern is proven under this exact
/// jsonschema engine: forbidden bytes are caught at pair offsets and their
/// odd-offset lookalikes stay legal.
#[test]
fn the_schema_path_union_accepts_and_refuses_at_pair_alignment() {
    let schema_text = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/scanner-report.schema.json"),
    )
    .unwrap();
    let schema_json: serde_json::Value = serde_json::from_str(&schema_text).unwrap();
    let harness = serde_json::json!({
        "$defs": schema_json["$defs"],
        "$ref": "#/$defs/RepoPath",
    });
    let validator = jsonschema::validator_for(&harness).unwrap();

    let accepted = [
        serde_json::json!("docs/guide.md"),
        serde_json::json!({"bytes_hex": "f2f2"}),
        serde_json::json!({"bytes_hex": "646f63732f62ff2e6d64"}),
        serde_json::json!({"bytes_hex": "a2f5c0"}),
    ];
    for value in accepted {
        assert!(validator.iter_errors(&value).next().is_none(), "{value}");
    }
    let refused = [
        serde_json::json!(""),
        serde_json::json!("/absolute"),
        serde_json::json!("a\\b"),
        serde_json::json!({"bytes_hex": "2f2f"}),
        serde_json::json!({"bytes_hex": "2fab"}),
        serde_json::json!({"bytes_hex": "ab2f"}),
        serde_json::json!({"bytes_hex": "ab2f2fcd"}),
        serde_json::json!({"bytes_hex": "005c"}),
        serde_json::json!({"bytes_hex": "ab00"}),
        serde_json::json!({"bytes_hex": "2e2e"}),
        serde_json::json!({"bytes_hex": "2e2e2fab"}),
        serde_json::json!({"bytes_hex": "ab2f2e2e"}),
        serde_json::json!({"bytes_hex": "F2F2"}),
        serde_json::json!({"bytes_hex": "abc"}),
        serde_json::json!({"bytes_hex": ""}),
        serde_json::json!({"bytes_hex": "f2f2", "extra": 1}),
        serde_json::json!({}),
    ];
    for value in refused {
        assert!(validator.iter_errors(&value).next().is_some(), "{value}");
    }
}
