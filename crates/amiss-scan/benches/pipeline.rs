#![expect(clippy::panic, reason = "bench fixture setup fails loudly")]

use std::collections::BTreeMap;
use std::sync::Arc;

use amiss_git::{GitLimits, GitResources};
use amiss_scan::correlate::{Side, correlate};
use amiss_scan::evaluate::evaluate_with_policy;
use amiss_scan::pipeline::{SetupShell, commit_pair};
use amiss_scan::report::{CandidateBlock, RequestDigests, Setup, SnapshotIdentity, construct};
use amiss_scan::resolve::{ForgeContext, Resolver, TargetCache};
use amiss_scan::{
    Classification, DocumentRecord, DocumentStatus, Effects, ScanLimits, ScanResources, Scanned,
    SnapshotDiscovery,
};
use amiss_wire::controls::GitMode;
use amiss_wire::digest::hb;
use amiss_wire::extraction::{Opaque, Work};
use amiss_wire::model::{Adapter, ForgeDialect, ObjectFormat, Oid, RepoPath};
use amiss_wire::report::EngineProvenance;
use divan::{Bencher, black_box};

#[path = "support/exceptions.rs"]
mod exception_support;
use exception_support::exception_fixture;
#[path = "support/observations.rs"]
mod observation_support;
use observation_support::side;

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
    bencher.bench_local(|| commit_pair(&repo, &shell.engine, None, &shell, &base, &candidate));
}

#[derive(Clone, Copy, Debug)]
enum ReportShape {
    SameDocuments,
    UnlinkedDocuments,
    RemovedObservations,
}

/// Report construction across ordered no-finding, document-finding, and
/// observation-finding surfaces.
#[divan::bench(
    args = [
        (ReportShape::SameDocuments, 1_000_usize),
        (ReportShape::SameDocuments, 10_000),
        (ReportShape::UnlinkedDocuments, 1_000),
        (ReportShape::UnlinkedDocuments, 10_000),
        (ReportShape::RemovedObservations, 100),
        (ReportShape::RemovedObservations, 1_000),
        (ReportShape::RemovedObservations, 5_000),
    ],
    sample_count = 10
)]
fn construct_reports(bencher: Bencher<'_, '_>, case: (ReportShape, usize)) {
    let (shape, count) = case;
    let setup = report_setup();
    let (discovery, comparisons) = match shape {
        ReportShape::SameDocuments => (
            document_discovery(count, &DocumentStatus::ExcludedBuiltIn),
            Vec::new(),
        ),
        ReportShape::UnlinkedDocuments => {
            let scanned = Arc::new(Scanned {
                adapter: Adapter::Markdown,
                work: Work {
                    nodes: 0,
                    nesting: 0,
                },
                embedded_code_bytes: 0,
                occurrences: Vec::new(),
                opaque: Opaque::default(),
                governed: Vec::new(),
                declared_anchors: Vec::new(),
                anchor_source: None,
            });
            (
                document_discovery(count, &DocumentStatus::Scanned(scanned)),
                Vec::new(),
            )
        }
        ReportShape::RemovedObservations => (
            document_discovery(0, &DocumentStatus::ExcludedBuiltIn),
            correlate(side("base", 0, count, None), Side::default())
                .unwrap_or_else(|defect| panic!("correlate observations: {defect:?}")),
        ),
    };
    bencher
        .with_inputs(|| comparisons.clone())
        .bench_local_values(|comparisons| {
            construct(
                black_box(&setup),
                black_box(&discovery),
                black_box(&discovery),
                black_box(comparisons),
                black_box(&[]),
            )
        });
}

#[derive(Clone, Copy, Debug)]
enum ResolutionShape {
    Native,
    SameRepositoryForge,
}

#[divan::bench(args = [ResolutionShape::Native, ResolutionShape::SameRepositoryForge])]
fn resolve_repository_path(bencher: Bencher<'_, '_>, shape: ResolutionShape) {
    let dir = tempfile::TempDir::new().unwrap_or_else(|defect| panic!("tempdir: {defect}"));
    amiss_fixtures::git(dir.path(), &["init", "-q"])
        .unwrap_or_else(|defect| panic!("git init: {defect}"));
    let repo = amiss_git::Repository::open(dir.path(), ObjectFormat::Sha1)
        .unwrap_or_else(|defect| panic!("open: {defect:?}"));
    let oid = Oid::new(ObjectFormat::Sha1, "a".repeat(40))
        .unwrap_or_else(|| panic!("benchmark object id"));
    let context = ForgeContext {
        host: "github.com".to_owned(),
        dialect: ForgeDialect::Github,
        object_format: ObjectFormat::Sha1,
        owner: "acme".to_owned(),
        repository: "widgets".to_owned(),
        candidate_ref: "refs/heads/feature/x".to_owned(),
        default_ref: "refs/heads/main".to_owned(),
        candidate_oid: None,
    };
    let (target, mode, forge, document, semantic) = match shape {
        ResolutionShape::Native => (
            "docs/assets/logo.png",
            GitMode::Tree,
            None,
            "docs/guide/source.md",
            "../assets/./logo%2Epng",
        ),
        ResolutionShape::SameRepositoryForge => (
            "docs/guide/page.md",
            GitMode::RegularFile,
            Some(&context),
            "docs/source.md",
            "https://github.com/acme/widgets/tree/feature/x/docs/guide/",
        ),
    };
    let target = RepoPath::new(target.to_owned()).unwrap_or_else(|| panic!("benchmark target"));
    let snapshot = SnapshotDiscovery {
        documents: Vec::new(),
        labels: BTreeMap::new(),
        outside_document_set: 0,
        tree_entries: 1,
        path_defects: Vec::new(),
        entries: BTreeMap::from([(target, (mode, oid))]),
    };
    let document =
        RepoPath::new(document.to_owned()).unwrap_or_else(|| panic!("benchmark document"));
    let mut git = GitResources::new(GitLimits::CONTRACT);
    let mut scan = ScanResources::new(ScanLimits::CONTRACT);
    let mut cache = TargetCache::default();
    bencher.bench_local(|| {
        Resolver::new(&repo, &mut git, &mut scan, &mut cache, &snapshot)
            .resolve(
                forge,
                Adapter::Markdown,
                &document,
                false,
                black_box(semantic),
            )
            .unwrap_or_else(|defect| panic!("resolve: {defect:?}"))
    });
}

/// Exact lookup at the end of an ordered discovery, where a linear scan would
/// pay for every preceding document.
#[divan::bench(args = [1_000_usize, 10_000, 100_000])]
fn lookup_last_document(bencher: Bencher<'_, '_>, count: usize) {
    let discovery = document_discovery(count, &DocumentStatus::ExcludedBuiltIn);
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
        evaluate_with_policy(
            &[],
            black_box(&comparisons),
            amiss_wire::controls::Profile::Enforce,
            black_box(&policy),
            &[],
            &[],
        )
    });
}

fn engine() -> EngineProvenance {
    EngineProvenance {
        version: "0.0.0-bench".to_owned(),
        digest: hb("amiss/scanner-engine", b"bench engine"),
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
        profile: amiss_wire::controls::Profile::Observe,
        repository: None,
        forge: None,
        candidate_ref: None,
        target_ref: None,
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

fn document_discovery(count: usize, status: &DocumentStatus) -> SnapshotDiscovery {
    let oid = Oid::new(ObjectFormat::Sha1, "b".repeat(40))
        .unwrap_or_else(|| panic!("benchmark object id"));
    let adapter = match status {
        DocumentStatus::Scanned(scanned) => Some(scanned.adapter),
        DocumentStatus::ExcludedBuiltIn
        | DocumentStatus::Unsupported(_)
        | DocumentStatus::Failed(_) => None,
    };
    let documents = (0..count)
        .map(|index| DocumentRecord {
            path: RepoPath::new(format!("docs/{index:05}.md"))
                .unwrap_or_else(|| panic!("benchmark document path")),
            classification: Classification::StructuredMarkdown,
            adapter,
            status: status.clone(),
            oid: oid.clone(),
            mode: GitMode::RegularFile,
            byte_count: 0,
            raw_digest: None,
        })
        .collect();
    SnapshotDiscovery {
        documents,
        labels: BTreeMap::new(),
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
