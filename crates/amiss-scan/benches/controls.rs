#![expect(
    clippy::expect_used,
    reason = "benchmark fixture paths are fixed and valid"
)]

use std::collections::BTreeSet;

use amiss_scan::policy::{InventoryState, effects};
use amiss_scan::{Includes, PolicySide};
use amiss_wire::controls::{DocumentInclude, IncludeKind, ScannerPolicy};
use amiss_wire::digest::hb;
use amiss_wire::model::{RepoPath, RepoPathText};
use divan::{Bencher, black_box};

fn main() {
    divan::main();
}

/// A descendant of the lexicographically last tree root. A scan of all roots
/// grows with `count`; ancestor probes do not.
#[divan::bench(args = [100_usize, 10_000, 100_000])]
fn late_tree_include(bencher: Bencher<'_, '_>, count: usize) {
    let trees = (0..count)
        .map(|index| path(format!("roots/{index:06}")))
        .collect::<BTreeSet<_>>();
    let query = path(format!(
        "roots/{:06}/nested/page.md",
        count.saturating_sub(1)
    ));
    let includes = Includes {
        documents: BTreeSet::new(),
        trees,
    };
    bencher.bench_local(|| black_box(&includes).matches(black_box(&query)));
}

/// Identical semantic policy sets in opposite vector order. Public policy
/// values can be constructed directly, so comparison cannot rely on parser
/// canonicalization even though parsed policies are sorted.
#[divan::bench(args = [100_usize, 1_000, 10_000], sample_count = 10)]
fn identical_policy_sets(bencher: Bencher<'_, '_>, count: usize) {
    let base = policy(count, false);
    let candidate = policy(count, true);
    let scanned = |_path: &str| InventoryState::Scanned;
    bencher.bench_local(|| effects(black_box(&base), black_box(&candidate), black_box(&scanned)));
}

fn path(raw: String) -> RepoPath {
    RepoPath::new(raw).expect("valid benchmark repository path")
}

fn policy(count: usize, reverse: bool) -> PolicySide {
    let indexes: Box<dyn Iterator<Item = usize>> = if reverse {
        Box::new((0..count).rev())
    } else {
        Box::new(0..count)
    };
    let document_includes = indexes
        .map(|index| DocumentInclude {
            path: RepoPathText::new(format!("roots/{index:06}"))
                .expect("valid benchmark include path"),
            kind: IncludeKind::Tree,
        })
        .collect();
    let policy = ScannerPolicy {
        digest: hb("amiss/raw-evidence", b"benchmark policy"),
        document_includes,
        protected_inventory: Vec::new(),
        finding_dispositions: Vec::new(),
    };
    PolicySide {
        digest: Some(policy.digest),
        policy: Some(policy),
    }
}
