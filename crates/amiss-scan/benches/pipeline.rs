#![expect(clippy::panic, reason = "bench fixture setup fails loudly")]

use std::collections::BTreeMap;

use amiss_scan::evaluate::evaluate_with_policy;
use amiss_scan::pipeline::{SetupShell, commit_pair};
use amiss_scan::report::{CandidateBlock, RequestDigests, Setup, SnapshotIdentity, construct};
use amiss_scan::{Classification, DocumentRecord, DocumentStatus, Effects, SnapshotDiscovery};
use amiss_wire::controls::GitMode;
use amiss_wire::digest::hb;
use amiss_wire::model::{ObjectFormat, Oid, RepoPath};
use amiss_wire::report::EngineProvenance;
use divan::{Bencher, black_box};

#[path = "support/exceptions.rs"]
mod exception_support;
use exception_support::exception_fixture;

fn main() {
    divan::main();
}

/// One complete evaluation over the representative repository: the
/// incremental-latency figure the promotion gate asks to see measured.
#[divan::bench(sample_count = 10)]
fn commit_pair_500_docs(bencher: Bencher<'_, '_>) {
    let dir = tempfile::TempDir::new().unwrap_or_else(|defect| panic!("tempdir: {defect}"));
    amiss_fixtures::representative_repository(dir.path(), 500)
        .unwrap_or_else(|defect| panic!("fixture repository: {defect}"));
    let base = revision(dir.path(), "HEAD~1");
    let candidate = revision(dir.path(), "HEAD");
    let repo = amiss_git::Repository::open(dir.path(), ObjectFormat::Sha1)
        .unwrap_or_else(|defect| panic!("open: {defect:?}"));
    let shell = SetupShell {
        engine: engine(),
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
    bencher.bench_local(|| commit_pair(&repo, &shell.engine, None, &shell, &base, &candidate));
}

/// Report construction over two identical ordered document sides. This
/// isolates the merge join from Git acquisition and parser work.
#[divan::bench(args = [1_000_usize, 10_000], sample_count = 10)]
fn construct_same_documents(bencher: Bencher<'_, '_>, count: usize) {
    let setup = report_setup();
    let discovery = document_discovery(count);
    bencher.bench_local(|| {
        construct(
            black_box(&setup),
            black_box(&discovery),
            black_box(&discovery),
            black_box(&[]),
        )
    });
}

/// Exact lookup at the end of an ordered discovery, where a linear scan would
/// pay for every preceding document.
#[divan::bench(args = [1_000_usize, 10_000, 100_000])]
fn lookup_last_document(bencher: Bencher<'_, '_>, count: usize) {
    let discovery = document_discovery(count);
    let path = RepoPath::new(format!("docs/{:05}.md", count.saturating_sub(1)))
        .unwrap_or_else(|| panic!("benchmark lookup path"));
    bencher.bench_local(|| black_box(&discovery).is_scanned_structured(black_box(&path)));
}

/// Exact matching of verified debt items to current candidate findings. The
/// fixture is outside the timed region, so this guards the target lookup from
/// regressing to a findings-by-items product.
#[divan::bench(args = [100_usize, 1_000, 10_000], sample_count = 10)]
fn evaluate_matching_debt(bencher: Bencher<'_, '_>, count: usize) {
    let (comparisons, policy) = exception_fixture(count);
    bencher.bench_local(|| {
        evaluate_with_policy(&[], black_box(&comparisons), true, black_box(&policy), &[])
    });
}

fn engine() -> EngineProvenance {
    EngineProvenance {
        version: "0.0.0-bench".to_owned(),
        digest: hb("amiss/scanner-engine/v1", b"bench engine"),
    }
}

fn report_setup() -> Setup {
    let oid = "a".repeat(40);
    let identity = SnapshotIdentity {
        object_format: "sha1",
        commit_oid: oid.clone(),
        tree_oid: oid,
    };
    Setup {
        engine: engine(),
        enforce: false,
        repository: None,
        forge: None,
        candidate_ref: None,
        default_branch_ref: None,
        base: identity.clone(),
        candidate: CandidateBlock::Commit(identity),
        policy: Effects {
            errors_retained: 64,
            ..Effects::default()
        },
        controls_unavailable: None,
        requests: RequestDigests::default(),
    }
}

fn document_discovery(count: usize) -> SnapshotDiscovery {
    let oid = Oid::new(ObjectFormat::Sha1, "b".repeat(40))
        .unwrap_or_else(|| panic!("benchmark object id"));
    let documents = (0..count)
        .map(|index| DocumentRecord {
            path: RepoPath::new(format!("docs/{index:05}.md"))
                .unwrap_or_else(|| panic!("benchmark document path")),
            classification: Classification::StructuredMarkdown,
            status: DocumentStatus::ExcludedBuiltIn,
            oid: oid.clone(),
            mode: GitMode::RegularFile,
            byte_count: 0,
            raw_digest: None,
        })
        .collect();
    SnapshotDiscovery {
        documents,
        outside_document_set: 0,
        tree_entries: u64::try_from(count).unwrap_or(u64::MAX),
        path_defects: Vec::new(),
        entries: BTreeMap::new(),
    }
}

fn revision(root: &std::path::Path, spec: &str) -> Oid {
    let raw = amiss_fixtures::git(root, &["rev-parse", spec])
        .unwrap_or_else(|defect| panic!("rev-parse: {defect}"));
    Oid::new(ObjectFormat::Sha1, raw.trim().to_owned()).unwrap_or_else(|| panic!("oid for {spec}"))
}
