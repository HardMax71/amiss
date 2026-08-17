use amiss_scan::correlate::correlate;
use divan::counter::ItemsCount;
use divan::{Bencher, black_box};

#[path = "support/observations.rs"]
mod observation_support;
use observation_support::side;

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
                let base = side("same", 0, count, None);
                let candidate = base.clone();
                (base, candidate)
            }
            PairedShape::Dense => (
                side("base", 0, count, Some("targets/shared.rs")),
                side("candidate", count, count, Some("targets/shared.rs")),
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
        .with_inputs(|| {
            (
                side("base", 0, count, None),
                side("candidate", count, count, None),
            )
        })
        .bench_local_values(|(base, candidate)| correlate(black_box(base), black_box(candidate)));
}
