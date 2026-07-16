use std::collections::BTreeSet;
use std::hint::black_box;
use std::time::{Duration, Instant};

use amiss_scan::{Includes, ScanLimits, ScanResources, scan_document};
use amiss_wire::model::{Adapter, RepoPath};

/// The parser-eligibility law: the maximal valid 4 MiB fixture stays below
/// two seconds on every supported release platform. The bound is enforced
/// only in optimized builds; a debug run still proves the fixture is valid
/// under every ceiling.
#[test]
fn the_maximal_valid_fixture_parses_under_the_eligibility_ceiling() {
    let source = amiss_fixtures::worst_case_markdown(4 * 1_024 * 1_024);
    assert!(
        source.len() <= 4_194_304 && source.len() > 4_192_000,
        "the fixture fills the 4 MiB document ceiling"
    );
    for adapter in [Adapter::Markdown, Adapter::Mdx] {
        let mut resources = ScanResources::new(ScanLimits::CONTRACT);
        let start = Instant::now();
        let scanned = scan_document(&mut resources, adapter, &source);
        let elapsed = start.elapsed();
        assert!(
            scanned.is_ok(),
            "the eligibility fixture is valid under the contract ceilings"
        );
        eprintln!("eligibility {adapter:?}: {elapsed:?}");
        if !cfg!(debug_assertions) {
            assert!(
                elapsed < Duration::from_secs(2),
                "the {adapter:?} adapter parses the maximal fixture below two seconds: {elapsed:?}"
            );
        }
    }
}

/// The largest reachable include union is two policy ceilings, one from each
/// snapshot side. Repeated late-root matches keep that valid input from
/// regressing to a scan of every policy row. The time bound is enforced only
/// in optimized builds; debug CI still exercises the maximum-sized union.
#[test]
fn the_maximal_policy_union_matches_under_the_eligibility_ceiling() {
    let per_policy =
        usize::try_from(ScanLimits::CONTRACT.repository_policy_entries).unwrap_or(usize::MAX / 2);
    let root_count = per_policy.saturating_mul(2);
    let trees = (0..root_count)
        .map(|index| {
            RepoPath::new(format!("roots/{index:06}")).expect("valid eligibility include path")
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(trees.len(), root_count, "the union reaches both ceilings");
    let query = RepoPath::new(format!(
        "roots/{:06}/nested/page.md",
        root_count.saturating_sub(1)
    ))
    .expect("valid eligibility query path");
    let includes = Includes {
        documents: BTreeSet::new(),
        trees,
    };

    let start = Instant::now();
    let matched = (0..1_000).all(|_| black_box(&includes).matches(black_box(&query)));
    let elapsed = start.elapsed();
    assert!(matched, "the last root covers its descendant");
    eprintln!("eligibility policy include union: {elapsed:?}");
    if !cfg!(debug_assertions) {
        assert!(
            elapsed < Duration::from_secs(2),
            "a maximal policy union serves 1,000 late-root matches below two seconds: {elapsed:?}"
        );
    }
}
