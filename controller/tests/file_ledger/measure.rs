use std::fs::File;
use std::time::Instant;

use amiss_controller::{DeliveryClaim, DeliveryLedger, FileLedgerError};
use tempfile::TempDir;

use super::support::{TestClock, check_binding, delivery_with_id, open_with_max};

const THRESHOLDS: [u64; 4] = [1_000, 10_000, 50_000, 100_000];
const MAX_ENTRIES: u64 = 100_000;
const MAX_RECORDS: u64 = 64;
const ADMISSION_PROBES: u64 = 36;
const SAMPLES: u64 = 9;
const MEDIAN_SAMPLE: usize = 4;

#[test]
#[ignore = "weekly release-mode filesystem measurement"]
fn admission_cost_is_measured_against_retained_root_entries() {
    let directory = TempDir::new().unwrap();
    let clock = TestClock::at(1_000);
    let mut ledger = open_with_max(directory.path(), &clock, MAX_RECORDS);
    let binding = check_binding();
    let mut created = 0_u64;

    for threshold in THRESHOLDS {
        while created < threshold {
            File::create(directory.path().join(format!("{created:064x}.report"))).unwrap();
            created += 1;
        }

        let mut elapsed = (0..SAMPLES)
            .map(|sample| {
                let delivery = delivery_with_id(
                    &format!("scale-{threshold}-{sample}"),
                    &format!("{}", threshold + sample + 1),
                );
                let started = Instant::now();
                let claim = ledger.claim(&delivery, &binding).unwrap();
                let elapsed = started.elapsed();
                assert!(matches!(claim, DeliveryClaim::Execute(_)));
                elapsed
            })
            .collect::<Vec<_>>();
        elapsed.sort_unstable();
        eprintln!(
            "ledger admission with {threshold} retained entries: median {:?}",
            elapsed.into_iter().nth(MEDIAN_SAMPLE).unwrap_or_default()
        );
    }

    for fill in ADMISSION_PROBES..MAX_RECORDS {
        ledger
            .claim(
                &delivery_with_id(&format!("fill-{fill}"), &format!("{}", 200_000 + fill)),
                &binding,
            )
            .unwrap();
    }
    let mut full = (0..SAMPLES)
        .map(|sample| {
            let delivery =
                delivery_with_id(&format!("full-{sample}"), &format!("{}", 300_000 + sample));
            let started = Instant::now();
            let claim = ledger.claim(&delivery, &binding);
            let elapsed = started.elapsed();
            assert!(matches!(claim, Err(FileLedgerError::Full)));
            elapsed
        })
        .collect::<Vec<_>>();
    full.sort_unstable();
    eprintln!(
        "full-ledger rejection with {MAX_ENTRIES} retained entries: median {:?}",
        full.into_iter().nth(MEDIAN_SAMPLE).unwrap_or_default()
    );

    let started = Instant::now();
    let cleanup = ledger.cleanup().unwrap();
    eprintln!(
        "ledger cleanup with {} retained entries: {:?}",
        MAX_ENTRIES,
        started.elapsed()
    );
    assert_eq!(cleanup.removed_reports, MAX_ENTRIES);
}
