#![cfg(test)]

use std::time::Duration;

use super::{InboxLimits, StoredLimits, record_reservation};

#[test]
fn stored_limits_admit_their_exact_boundaries() {
    let reservation = record_reservation(1, 1, 1, 1, 1).unwrap();
    let exact = InboxLimits {
        lease_duration: Duration::from_secs(1),
        max_records: 1,
        max_bytes: reservation.checked_mul(2).unwrap(),
        max_record_bytes: reservation,
        max_body_bytes: 1,
        max_headers: 1,
        max_header_bytes: 1,
        max_route_bytes: 1,
        max_source_id_bytes: 1,
    };
    assert!(StoredLimits::read(exact).is_ok(), "both caps met exactly");
    let record_short = InboxLimits {
        max_record_bytes: reservation - 1,
        ..exact
    };
    assert!(StoredLimits::read(record_short).is_err());
    let budget_short = InboxLimits {
        max_bytes: reservation.checked_mul(2).unwrap() - 1,
        ..exact
    };
    assert!(StoredLimits::read(budget_short).is_err());
}
