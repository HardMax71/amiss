use amiss_wire::report::model::{ReportEnvelope, ReportPayload};
use amiss_wire::report::{
    AnalysisErrorCode, EngineProvenance, FATAL_SCRATCH_BYTES, MACHINE_JSON_BYTES, PAYLOAD_SCHEMA,
    emit_report, unavailable_evaluation_wire,
};
use serde_json::Value;
use sha2::Digest as _;
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};
use std::alloc::System;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

const WIRE_CAP: u64 = MACHINE_JSON_BYTES;

/// A maximal schema-valid `RepoPath`: 4,096 characters dominated by quotes,
/// the densest escaping the path grammar can reach on the wire, prefixed for
/// uniqueness and byte order.
fn maximal_path(index: usize) -> String {
    format!("{index:04}{}", "\"".repeat(4_092))
}

const DIGEST: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn runtime_file(index: usize) -> Value {
    Value::from_iter(vec![
        ("path", Value::from((maximal_path(index)).as_str())),
        ("role", Value::from("dynamic-library")),
        ("git_mode", Value::from("100755")),
        ("file_sha256", Value::from(DIGEST)),
    ])
}

fn artifact(platform: &str) -> Value {
    let files: Vec<Value> = (0..256).map(runtime_file).collect();
    Value::from_iter(vec![
        ("platform", Value::from(platform)),
        (
            "artifact_name",
            Value::from(
                (format!(
                    "amiss-{platform}{}",
                    "x".repeat(122_usize.saturating_sub(platform.len()))
                ))
                .as_str(),
            ),
        ),
        ("tree_path", Value::from((maximal_path(9_000)).as_str())),
        ("binary_sha256", Value::from(DIGEST)),
        ("engine_digest", Value::from(DIGEST)),
        ("runtime_contract", Value::from("manifest-closed")),
        ("environment_contract", Value::from("scanner-process-env")),
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
            Value::from_iter(vec![
                ("path", Value::from((maximal_path(index)).as_str())),
                ("raw_digest", Value::from(DIGEST)),
            ])
        })
        .collect();
    let manifest = Value::from_iter(vec![
        ("schema", Value::from("amiss/scanner-release-manifest")),
        (
            "engine_version",
            Value::from((format!("100.200.300-{}", "a".repeat(52))).as_str()),
        ),
        (
            "build_source",
            Value::from_iter(vec![
                (
                    "repository",
                    Value::from_iter(vec![
                        ("host", Value::from("git.example.internal")),
                        (
                            "owner",
                            Value::from(
                                (format!("{}/{}", "o".repeat(100), "g".repeat(100))).as_str(),
                            ),
                        ),
                        ("name", Value::from(("n".repeat(100)).as_str())),
                    ]),
                ),
                ("object_format", Value::from("sha256")),
                ("commit_oid", Value::from(("a".repeat(64)).as_str())),
            ]),
        ),
        (
            "dependency_lock",
            Value::from_iter(vec![
                ("schema", Value::from("amiss/scanner-dependency-lock-input")),
                ("files", Value::Array(locks)),
            ]),
        ),
        ("dependency_lock_digest", Value::from(DIGEST)),
        ("artifacts", Value::Array(artifacts)),
    ]);
    Value::from_iter(vec![
        ("kind", Value::from("forge-action")),
        (
            "action_repository",
            Value::from_iter(vec![
                ("host", Value::from("git.example.internal")),
                (
                    "owner",
                    Value::from((format!("{}/{}", "o".repeat(100), "g".repeat(100))).as_str()),
                ),
                ("name", Value::from(("n".repeat(100)).as_str())),
            ]),
        ),
        ("action_object_format", Value::from("sha256")),
        ("action_commit_oid", Value::from(("a".repeat(64)).as_str())),
        ("action_tree_oid", Value::from(("b".repeat(64)).as_str())),
        ("dependency_lock_digest", Value::from(DIGEST)),
        ("release_manifest", manifest),
        ("release_manifest_digest", Value::from(DIGEST)),
        ("manifest_path", Value::from((maximal_path(9_001)).as_str())),
        ("selected_platform", Value::from("linux-x86_64")),
        (
            "selected_artifact_name",
            Value::from((format!("amiss-linux-x86_64{}", "x".repeat(110))).as_str()),
        ),
    ])
}

/// One maximal retained error row: the widest phase and code strings, a
/// 4,096-character quote-dense path, the full 8,192-character byte hex, the
/// longest resource name, and safe-integer limits.
fn maximal_error(index: usize) -> Value {
    Value::from_iter(vec![
        ("phase", Value::from("configuration")),
        ("code", Value::from("RESOURCE_LIMIT_EXCEEDED")),
        ("description", Value::from(("d".repeat(400)).as_str())),
        ("path", Value::from((maximal_path(index)).as_str())),
        ("path_bytes_hex", Value::from(("ab".repeat(4_096)).as_str())),
        (
            "resource",
            Value::from("aggregate-git-compressed-object-bytes-per-evaluation"),
        ),
        ("configured_limit", Value::from(9_007_199_254_740_991_u64)),
        (
            "observed_lower_bound",
            Value::from(9_007_199_254_740_991_u64),
        ),
    ])
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn assert_schema_valid(wire: &[u8]) {
    let schema_text = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/scanner-report.schema.json"),
    )
    .unwrap();
    let schema_json: Value = serde_json::from_str(&schema_text).unwrap();
    let validator = jsonschema::validator_for(&schema_json).unwrap();
    let envelope_json: Value = serde_json::from_slice(wire).unwrap();
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
        digest: amiss_wire::model::Digest::from(
            sha2::Sha256::new_with_prefix("amiss/scanner-engine")
                .chain_update([0_u8])
                .chain_update(b"maximal golden")
                .finalize()
                .0,
        ),
    };
    let codes: BTreeSet<AnalysisErrorCode> = [
        AnalysisErrorCode::InvalidInvocation,
        AnalysisErrorCode::InvalidEvent,
        AnalysisErrorCode::InvalidProfile,
        AnalysisErrorCode::RequestUnreadable,
    ]
    .into_iter()
    .collect();
    let request_digest = amiss_wire::model::Digest::from(
        sha2::Sha256::new_with_prefix("amiss/scanner-evaluation-request")
            .chain_update([0_u8])
            .chain_update(b"maximal")
            .finalize()
            .0,
    );
    let base_wire =
        unavailable_evaluation_wire(&engine, &codes, Some(request_digest), Some(request_digest))
            .unwrap()
            .unwrap();
    let trimmed = base_wire.strip_suffix(b"\n").unwrap();
    let envelope = serde_json::from_slice::<Value>(trimmed).unwrap();

    let Value::Object(mut envelope_members) = envelope else {
        panic!("envelope is an object");
    };
    let payload = envelope_members.get_mut("payload").unwrap();
    *((payload).get_mut("engine").expect("fixture member exists"))
        .get_mut("action_provenance")
        .expect("fixture member exists") = maximal_provenance();
    *((payload)
        .get_mut("controls")
        .expect("fixture member exists"))
    .get_mut("reasons")
    .expect("fixture member exists") = Value::Array(
        [
            "not-parsed",
            "invalid-profile",
            "invalid-repository-policy",
            "invalid-external-control",
            "control-binding-mismatch",
        ]
        .iter()
        .map(|reason| Value::from(*reason))
        .collect(),
    );
    let errors: Vec<Value> = (0..64).map(maximal_error).collect();
    *(payload).get_mut("errors").expect("fixture member exists") = Value::Array(errors);
    *((payload).get_mut("result").expect("fixture member exists"))
        .get_mut("error_count")
        .expect("fixture member exists") = Value::from(64);
    let payload_digest = amiss_wire::model::Digest::from(
        sha2::Sha256::new_with_prefix(PAYLOAD_SCHEMA)
            .chain_update([0_u8])
            .chain_update(serde_json_canonicalizer::to_vec(payload).unwrap())
            .finalize()
            .0,
    );

    let mut maximal = Value::Object(envelope_members);
    *maximal
        .get_mut("payload_digest")
        .expect("fixture member exists") = Value::from((payload_digest.to_string()).as_str());
    let mut wire = serde_json_canonicalizer::to_vec(&maximal).unwrap();
    wire.push(b'\n');

    assert_schema_valid(&wire);

    assert!(
        u64::try_from(wire.len()).unwrap_or(u64::MAX) < WIRE_CAP,
        "the fatal-incomplete wire fits the reservation: {} bytes",
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
        u64::try_from(paper_bound).unwrap_or(u64::MAX) < WIRE_CAP,
        "the documented worst-case decomposition stays under the reservation"
    );
    assert!(
        wire.len() < paper_bound,
        "the actual serializer stays under the paper bound: {} < {paper_bound}",
        wire.len()
    );

    let typed: ReportEnvelope = serde_json::from_slice(&wire).unwrap();
    prove_streamed_emission(&typed, &wire);

    prove_binary_error_paths(&engine);
}

#[expect(clippy::unwrap_used, reason = "allocation fixture setup")]
fn prove_binary_error_paths(engine: &EngineProvenance) {
    let mut binary = amiss_wire::report::invocation_failure_envelope(
        engine,
        &BTreeSet::from([AnalysisErrorCode::InvalidInvocation]),
    )
    .unwrap()
    .unwrap();
    binary.payload.errors = (128_u8..192)
        .map(|first| {
            let path =
                amiss_wire::model::RepoPath::from_bytes([vec![first], vec![0xff; 4095]].concat())
                    .unwrap();
            amiss_wire::report::error_row(&amiss_wire::report::ErrorDetail {
                code: AnalysisErrorCode::GitObjectUnreadable,
                path: Some(path),
                path_bytes: None,
                resource: None,
            })
        })
        .collect();
    binary.payload.result.error_count = 64;
    binary.payload_digest = {
        let mut writer = digest_io::IoWrapper(
            sha2::Sha256::new_with_prefix(PAYLOAD_SCHEMA).chain_update([0_u8]),
        );
        serde_json_canonicalizer::to_writer(&binary.payload, &mut writer)
            .map(|()| amiss_wire::model::Digest::from(writer.0.finalize().0))
    }
    .unwrap();
    let mut wire = serde_json_canonicalizer::to_vec(&binary).unwrap();
    wire.push(b'\n');
    assert_schema_valid(&wire);
    prove_streamed_emission(&binary, &wire);
}

#[expect(clippy::unwrap_used, reason = "allocation assertions")]
fn prove_streamed_emission<P: serde::Serialize>(
    maximal: &ReportEnvelope<ReportPayload<P>>,
    wire: &[u8],
) {
    let mut streamed =
        std::io::BufWriter::with_capacity(FATAL_SCRATCH_BYTES, Vec::with_capacity(wire.len()));
    let region = Region::new(GLOBAL);
    let emitted = emit_report(maximal, &mut streamed).unwrap();
    let stats = region.change();
    assert_eq!(
        emitted,
        u64::try_from(wire.len()).unwrap(),
        "the streamed emission reports the exact wire length"
    );
    assert_eq!(
        streamed.into_inner().unwrap(),
        wire,
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
