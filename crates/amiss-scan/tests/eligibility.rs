use std::time::{Duration, Instant};

use amiss_scan::{ScanLimits, ScanResources, scan_document};
use amiss_wire::model::Adapter;

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
