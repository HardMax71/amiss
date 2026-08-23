use std::collections::BTreeMap;
use std::time::Instant;

use amiss_git::{GitLimits, GitResources, Repository};
use amiss_scan::pipeline::{SetupShell, commit_pair};
use amiss_scan::report::RequestDigests;
use amiss_scan::resolve::{Resolver, TargetCache};
use amiss_scan::{Resolution, ScanLimits, ScanResources, SnapshotDiscovery};
use amiss_wire::controls::GitMode;
use amiss_wire::digest::hb;
use amiss_wire::model::{Adapter, ObjectFormat, Oid, RepoPath};
use amiss_wire::report::EngineProvenance;
use amiss_wire::report::MACHINE_JSON_BYTES;
use amiss_wire::resolution::Missing;

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
    let repo = Repository::open(dir.path(), ObjectFormat::Sha1)
        .unwrap_or_else(|defect| panic!("open: {defect:?}"));
    let shell = SetupShell {
        engine: EngineProvenance {
            version: "0.0.0-measure".to_owned(),
            digest: hb("amiss/scanner-engine", b"measure engine"),
        },
        profile: amiss_wire::controls::Profile::Observe,
        repository: None,
        forge: None,
        candidate_ref: None,
        target_ref: None,
        default_branch_ref: None,
        floor: None,
        debt: None,
        waiver: None,
        time: None,
        constraint: None,
        semantic: amiss_scan::semantic::Inputs::default(),
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
        "measure representative-500: {observations} observations, wire {} bytes of the {MACHINE_JSON_BYTES} reservation",
        wire.len()
    );
}

/// A late case-only match isolates the missing-target fallback from discovery
/// across a large ordered entry inventory.
#[test]
#[ignore = "promotion evidence, run explicitly in release"]
fn late_case_neighbor_latency_and_memory() {
    let dir = tempfile::TempDir::new().unwrap_or_else(|defect| panic!("tempdir: {defect}"));
    amiss_fixtures::git(dir.path(), &["init", "-q"])
        .unwrap_or_else(|defect| panic!("git init: {defect}"));
    let repo = Repository::open(dir.path(), ObjectFormat::Sha1)
        .unwrap_or_else(|defect| panic!("open: {defect:?}"));
    let oid = Oid::new(ObjectFormat::Sha1, "a".repeat(40)).unwrap_or_else(|| panic!("fixture oid"));
    let entries = (0..100_000_usize)
        .map(|index| {
            let path = RepoPath::new(format!("generated/{index:06}.page"))
                .unwrap_or_else(|| panic!("fixture path"));
            (path, (GitMode::RegularFile, oid.clone()))
        })
        .collect();
    let snapshot = SnapshotDiscovery {
        documents: Vec::new(),
        outside_document_set: 0,
        tree_entries: 100_000,
        path_defects: Vec::new(),
        entries,
        labels: BTreeMap::new(),
    };
    let document =
        RepoPath::new("README.md".to_owned()).unwrap_or_else(|| panic!("fixture document"));
    let mut git = GitResources::new(GitLimits::CONTRACT);
    let mut scan = ScanResources::new(ScanLimits::CONTRACT);
    let mut cache = TargetCache::default();
    let mut resolve = || {
        Resolver::new(&repo, &mut git, &mut scan, &mut cache, &snapshot)
            .resolve(
                None,
                Adapter::Markdown,
                &document,
                false,
                "GENERATED/099999.PAGE",
            )
            .unwrap_or_else(|defect| panic!("resolve: {defect:?}"))
            .1
    };

    let start = Instant::now();
    let unprofiled = resolve();
    let elapsed = start.elapsed();
    let profiler = dhat::Profiler::builder().testing().build();
    let repeated = resolve();
    let stats = dhat::HeapStats::get();
    drop(profiler);
    assert_eq!(repeated, unprofiled, "profiling changes nothing");
    let Resolution::Missing(Missing::PathNotFound {
        near: Some(near), ..
    }) = repeated
    else {
        panic!("expected one case neighbor");
    };
    assert_eq!(near.as_str(), Some("generated/099999.page"));
    eprintln!("measure late-case-neighbor-100k: wall {elapsed:?} (unprofiled)");
    eprintln!(
        "measure late-case-neighbor-100k: {} allocations, {} total bytes, {} peak bytes",
        stats.total_blocks, stats.total_bytes, stats.max_bytes
    );
}

#[expect(clippy::panic, reason = "measurement fixture fails loudly")]
fn revision(root: &std::path::Path, spec: &str) -> Oid {
    let raw = amiss_fixtures::git(root, &["rev-parse", spec])
        .unwrap_or_else(|defect| panic!("rev-parse: {defect}"));
    Oid::new(ObjectFormat::Sha1, raw.trim().to_owned()).unwrap_or_else(|| panic!("oid for {spec}"))
}
