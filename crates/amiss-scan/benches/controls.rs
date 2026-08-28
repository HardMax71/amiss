#![expect(
    clippy::expect_used,
    reason = "benchmark fixture paths are fixed and valid"
)]

use std::collections::{BTreeMap, BTreeSet};

use amiss_scan::claim::{ClaimCarrier, ClaimMissingReason, ClaimOutcome, ClaimVerdict};
use amiss_scan::evaluate::claim_groups;
use amiss_scan::policy::{InventoryState, effects};
use amiss_scan::scan::SpanDisplay;
use amiss_scan::{Includes, PolicySide};
use amiss_wire::controls::{DocumentInclude, IncludeKind, ScannerPolicy};
use amiss_wire::digest::hb;
use amiss_wire::model::{RepoPath, RepoPathText};
use divan::{Bencher, black_box};

fn main() {
    divan::main();
}

#[derive(Clone, Copy, Debug)]
enum IncludeShape {
    Tree,
    Suffix,
}

/// A descendant of the lexicographically last root. A scan of all roots grows
/// with `count`; indexed ancestor and suffix probes do not.
#[divan::bench(
    args = [
        (IncludeShape::Tree, 100_usize),
        (IncludeShape::Tree, 10_000),
        (IncludeShape::Tree, 100_000),
        (IncludeShape::Suffix, 100),
        (IncludeShape::Suffix, 10_000),
        (IncludeShape::Suffix, 100_000),
    ]
)]
fn late_include(bencher: Bencher<'_, '_>, case: (IncludeShape, usize)) {
    let (shape, count) = case;
    let roots = (0..count)
        .map(|index| path(format!("roots/{index:06}")))
        .collect::<BTreeSet<_>>();
    let (tail, includes) = match shape {
        IncludeShape::Tree => (
            "page.md",
            Includes {
                trees: roots,
                ..Includes::default()
            },
        ),
        IncludeShape::Suffix => (
            "page.txt",
            Includes {
                suffix_roots: BTreeMap::from([(".txt".to_owned(), roots)]),
                ..Includes::default()
            },
        ),
    };
    let query = path(format!(
        "roots/{:06}/nested/{tail}",
        count.saturating_sub(1)
    ));
    bencher.bench_local(|| black_box(&includes).matches(black_box(&query)));
}

/// Identical semantic policy sets supplied in opposite order. Construction
/// canonicalizes them before comparison.
#[divan::bench(args = [100_usize, 1_000, 10_000], sample_count = 10)]
fn identical_policy_sets(bencher: Bencher<'_, '_>, count: usize) {
    let base = policy(count, false);
    let candidate = policy(count, true);
    let scanned = |_path: &str| InventoryState::Scanned;
    bencher.bench_local(|| effects(black_box(&base), black_box(&candidate), black_box(&scanned)));
}

/// One claim group whose members all carry distinct source evidence. The
/// per-document reference ceiling fixes the largest lawful group at 16,384.
#[divan::bench(args = [1_000_usize, 10_000, 16_384], sample_count = 10)]
fn distinct_claim_sources(bencher: Bencher<'_, '_>, count: usize) {
    let outcomes = claim_outcomes(count);
    bencher.bench_local(|| claim_groups(black_box(&outcomes)));
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
            suffix: None,
            adapter: None,
        })
        .collect();
    let policy = ScannerPolicy::new(document_includes, Vec::new(), Vec::new(), Vec::new())
        .expect("benchmark policy is valid");
    PolicySide {
        digest: Some(policy.digest()),
        policy: Some(policy),
    }
}

fn claim_outcomes(count: usize) -> Vec<ClaimOutcome> {
    let document = path("docs/claims.md".to_owned());
    let target = path("src/value.rs".to_owned());
    let expected_digest = hb("amiss/bench-expected", b"expected");
    (0..count)
        .map(|index| {
            let token = index.to_string();
            let display_line = u64::try_from(index).unwrap_or(u64::MAX).saturating_add(1);
            ClaimOutcome {
                carrier: ClaimCarrier::Definition,
                document: document.clone(),
                name: "value".to_owned(),
                span: (index, index.saturating_add(1)),
                display: SpanDisplay {
                    start_line: display_line,
                    start_column: 1,
                    end_line: display_line,
                    end_column: 2,
                },
                source_digest: hb("amiss/bench-claim-source", token.as_bytes()),
                path: target.clone(),
                line: 1,
                expected_digest,
                verdict: ClaimVerdict::TargetMissing(ClaimMissingReason::Absent),
            }
        })
        .collect()
}
