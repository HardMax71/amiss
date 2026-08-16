use amiss_md::extract::BlockKind;
use amiss_scan::correlate::{Observation, Side, correlate};
use amiss_scan::resolve::{Intent, Resolution};
use amiss_scan::scan::SpanDisplay;
use amiss_wire::controls::{SourceConstruct, TargetKind};
use amiss_wire::digest::hb;
use amiss_wire::model::{Adapter, RepoPath};
use amiss_wire::report::IntentKind;
use amiss_wire::resolution::Missing;
use divan::counter::ItemsCount;
use divan::{Bencher, black_box};

fn main() {
    divan::main();
}

#[derive(Clone, Copy, Debug)]
enum PairedShape {
    Exact,
    Dense,
}

/// Equal snapshots take the identity path; dense snapshots take one complete
/// ambiguity component under the same input cardinalities.
#[divan::bench(
    args = [
        (PairedShape::Exact, 100_usize),
        (PairedShape::Exact, 1_000),
        (PairedShape::Exact, 10_000),
        (PairedShape::Dense, 100),
        (PairedShape::Dense, 1_000),
        (PairedShape::Dense, 10_000),
    ],
    sample_count = 10
)]
fn paired_components(bencher: Bencher<'_, '_>, case: (PairedShape, usize)) {
    let (shape, count) = case;
    bencher
        .with_inputs(|| match shape {
            PairedShape::Exact => {
                let base = side("same", 0, count);
                let candidate = base.clone();
                (base, candidate)
            }
            PairedShape::Dense => (
                dense_side("base", 0, count),
                dense_side("candidate", count, count),
            ),
        })
        .counter(ItemsCount::new(count))
        .bench_local_values(|(base, candidate)| correlate(black_box(base), black_box(candidate)));
}

/// Unmatched observations with unrelated intents. This is the scale shape
/// that regresses from indexed grouping to a base-by-candidate product if the
/// correlation key is removed.
#[divan::bench(args = [100_usize, 1_000, 10_000], sample_count = 10)]
fn unrelated_intents(bencher: Bencher<'_, '_>, count: usize) {
    bencher
        .with_inputs(|| (side("base", 0, count), side("candidate", count, count)))
        .bench_local_values(|(base, candidate)| correlate(black_box(base), black_box(candidate)));
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
        external_destination: None,
        intent: Intent {
            kind: IntentKind::RepositoryPath,
            repository_path: Some(target.clone()),
            target_kind: Some(TargetKind::Either),
            external_scheme: None,
            query: None,
            fragment: None,
        },
        raw_destination: String::new(),
        raw_destination_digest: hb("amiss/scanner-raw-destination", target.as_bytes()),
        projection_digest: hb("amiss/scanner-source-projection", b"reference"),
        resolution: Resolution::Missing(Missing::PathNotFound {
            path: target,
            near: None,
        }),
        fragment_span: None,
        path_span: None,
    }
}

#[expect(clippy::expect_used, reason = "benchmark paths are fixed and valid")]
fn repo_path(raw: String) -> RepoPath {
    RepoPath::new(raw).expect("valid benchmark repository path")
}
