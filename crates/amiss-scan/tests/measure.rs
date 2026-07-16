use std::time::Instant;

use amiss_scan::pipeline::{SetupShell, commit_pair};
use amiss_scan::report::RequestDigests;
use amiss_wire::digest::hb;
use amiss_wire::model::{ObjectFormat, Oid};
use amiss_wire::report::EngineProvenance;

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

/// The promotion-evidence measurement: incremental latency and heap peak
/// for one evaluation of the representative repository, printed for the
/// bench workflow's artifact. Ignored by default; run it explicitly with
/// `--run-ignored` in a release build.
#[test]
#[ignore = "promotion evidence, run explicitly in release"]
fn representative_repository_latency_and_memory() {
    let dir = tempfile::TempDir::new().unwrap_or_else(|defect| panic!("tempdir: {defect}"));
    amiss_fixtures::representative_repository(dir.path(), 500)
        .unwrap_or_else(|defect| panic!("fixture repository: {defect}"));
    let base = revision(dir.path(), "HEAD~1");
    let candidate = revision(dir.path(), "HEAD");
    let repo = amiss_git::Repository::open(dir.path(), ObjectFormat::Sha1)
        .unwrap_or_else(|defect| panic!("open: {defect:?}"));
    let shell = SetupShell {
        engine: EngineProvenance {
            version: "0.0.0-measure".to_owned(),
            digest: hb("amiss/scanner-engine", b"measure engine"),
        },
        enforce: false,
        repository: None,
        forge: None,
        candidate_ref: None,
        default_branch_ref: None,
        floor: None,
        debt: None,
        waiver: None,
        time: None,
        constraint: None,
        requests: RequestDigests::default(),
        external_defect: None,
        errors_retained: 64,
    };

    let start = Instant::now();
    let built = commit_pair(&repo, &shell.engine, None, &shell, &base, &candidate);
    let elapsed = start.elapsed();

    let profiler = dhat::Profiler::builder().testing().build();
    let repeated = commit_pair(&repo, &shell.engine, None, &shell, &base, &candidate);
    let stats = dhat::HeapStats::get();
    drop(profiler);
    assert_eq!(
        repeated.exit_code, built.exit_code,
        "profiling changes nothing"
    );

    if built.exit_code != 0 {
        let wire: serde_json::Value = serde_json::from_slice(&built.wire()).unwrap_or_default();
        eprintln!("errors: {}", wire["payload"]["errors"]);
        eprintln!("result: {}", wire["payload"]["result"]);
    }
    assert_eq!(built.exit_code, 0, "the representative evaluation passes");
    let wire = built.wire();
    let observations = serde_json::from_slice::<serde_json::Value>(&wire)
        .ok()
        .and_then(|envelope| envelope["payload"]["observations"].as_array().map(Vec::len))
        .unwrap_or(0);
    eprintln!("measure representative-500: wall {elapsed:?} (unprofiled)");
    eprintln!(
        "measure representative-500: heap peak {} bytes in {} blocks, total {} bytes",
        stats.max_bytes, stats.max_blocks, stats.total_bytes
    );
    eprintln!(
        "measure representative-500: {observations} observations, wire {} bytes of the 67108864 reservation",
        wire.len()
    );
}

#[expect(clippy::panic, reason = "measurement fixture fails loudly")]
fn revision(root: &std::path::Path, spec: &str) -> Oid {
    let raw = amiss_fixtures::git(root, &["rev-parse", spec])
        .unwrap_or_else(|defect| panic!("rev-parse: {defect}"));
    Oid::new(ObjectFormat::Sha1, raw.trim().to_owned()).unwrap_or_else(|| panic!("oid for {spec}"))
}
