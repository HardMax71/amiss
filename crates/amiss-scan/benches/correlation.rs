use amiss_md::extract::BlockKind;
use amiss_scan::correlate::{Observation, Side, correlate};
use amiss_scan::resolve::{Intent, Resolution};
use amiss_scan::scan::SpanDisplay;
use amiss_wire::controls::{ContentAvailability, SourceConstruct, TargetKind};
use amiss_wire::digest::hb;
use amiss_wire::model::{Adapter, RepoPath};
use amiss_wire::report::{IntentKind, ResolutionCode};
use divan::{Bencher, black_box};

fn main() {
    divan::main();
}

/// Unmatched observations with unrelated intents. This is the scale shape
/// that regresses from indexed grouping to a base-by-candidate product if the
/// correlation key is removed.
#[divan::bench(args = [100_usize, 1_000, 10_000], sample_count = 10)]
fn unrelated_intents(bencher: Bencher<'_, '_>, count: usize) {
    let base = side("base", 0, count);
    let candidate = side("candidate", count, count);
    bencher.bench_local(|| correlate(black_box(&base), black_box(&candidate)));
}

/// One dense ambiguity component. Every base observation could pair with
/// every candidate observation, but connected-component construction needs
/// only a linear spanning tree.
#[divan::bench(args = [100_usize, 1_000, 10_000], sample_count = 10)]
fn dense_ambiguity(bencher: Bencher<'_, '_>, count: usize) {
    let base = dense_side("base", 0, count);
    let candidate = dense_side("candidate", count, count);
    bencher.bench_local(|| correlate(black_box(&base), black_box(&candidate)));
}

fn side(label: &str, offset: usize, count: usize) -> Side {
    let observations = (offset..offset.saturating_add(count))
        .map(|index| observation(label, index))
        .collect();
    Side {
        observations,
        ..Side::default()
    }
}

fn dense_side(label: &str, offset: usize, count: usize) -> Side {
    let observations = (offset..offset.saturating_add(count))
        .map(|index| observation_with_target(label, index, "targets/shared.rs"))
        .collect();
    Side {
        observations,
        ..Side::default()
    }
}

fn observation(side: &str, index: usize) -> Observation {
    let token = format!("{side}/{index}");
    observation_with_target(side, index, &format!("targets/{token}.rs"))
}

fn observation_with_target(side: &str, index: usize, target: &str) -> Observation {
    let token = format!("{side}/{index}");
    let target = repo_path(target.to_owned());
    Observation {
        id: hb("amiss/bench-correlation-id", token.as_bytes()),
        document: repo_path("docs/references.md".to_owned()),
        span: (0, 0),
        display: SpanDisplay {
            start_line: 1,
            start_column: 1,
            end_line: 1,
            end_column: 1,
        },
        block_kind: BlockKind::Paragraph,
        node_path: Vec::new(),
        adapter: Adapter::Markdown,
        construct: SourceConstruct::InlineLink,
        intent: Intent {
            kind: IntentKind::RepositoryPath,
            repository_path: Some(target.clone()),
            target_kind: Some(TargetKind::Either),
            external_scheme: None,
            query: None,
            fragment: None,
        },
        raw_destination_digest: hb("amiss/scanner-raw-destination", target.as_bytes()),
        projection_digest: hb("amiss/scanner-source-projection", b"reference"),
        resolution: Resolution {
            code: ResolutionCode::PathNotFound,
            path: Some(target),
            entry_kind: None,
            git_mode: None,
            raw_digest: None,
            projection_digest: None,
            content_availability: ContentAvailability::NotApplicable,
        },
    }
}

#[expect(clippy::expect_used, reason = "benchmark paths are fixed and valid")]
fn repo_path(raw: String) -> RepoPath {
    RepoPath::new(raw).expect("valid benchmark repository path")
}
