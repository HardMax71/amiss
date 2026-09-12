use amiss_wire::controls::{DebtSnapshot, parse_debt_snapshot};
use amiss_wire::de::ErrorKind;
use amiss_wire::model::Digest;
use amiss_wire::model::UtcInstant;

use super::support::DEBT;

#[test]
fn parses_a_valid_debt_snapshot() {
    let expected: DebtSnapshot = serde_json::from_slice(DEBT).unwrap();
    assert_eq!(parse_debt_snapshot(DEBT).unwrap(), expected);
}

#[test]
fn an_item_born_at_the_snapshot_instant_is_consistent() {
    let mut snapshot: DebtSnapshot = serde_json::from_slice(DEBT).unwrap();
    snapshot.items[0].created_at = snapshot.created_at.clone();
    assert!(snapshot.validate().is_ok());
}

#[test]
fn rejects_debt_digest_and_order_defects() {
    let original: DebtSnapshot = serde_json::from_slice(DEBT).unwrap();
    let mut bad_key = original.clone();
    bad_key.items[0].finding_key = Digest::from([0; 32]);
    let mut bad_fact = original.clone();
    bad_fact.items[0].accepted_fact_digest = Digest::from([0; 32]);
    let mut unsorted = original.clone();
    unsorted.items[0].debt_id = "debt/b".parse().unwrap();
    let mut second = unsorted.items[0].clone();
    second.debt_id = "debt/a".parse().unwrap();
    unsorted.items.push(second);
    let mut duplicate = unsorted.clone();
    duplicate.items.reverse();
    let mut late = original.clone();
    late.items[0].created_at = UtcInstant::new("2026-07-03T00:00:00Z".to_owned()).unwrap();
    let mut inverted = original;
    inverted.items[0].created_at = UtcInstant::new("2026-08-01T00:00:00Z".to_owned()).unwrap();
    inverted.items[0].expires_at = UtcInstant::new("2026-07-01T00:00:00Z".to_owned()).unwrap();
    inverted.created_at = UtcInstant::new("2026-08-02T00:00:00Z".to_owned()).unwrap();
    for (snapshot, expected) in [
        (bad_key, ErrorKind::DigestMismatch),
        (bad_fact, ErrorKind::DigestMismatch),
        (unsorted, ErrorKind::UnsortedSet),
        (duplicate, ErrorKind::DuplicateMember),
        (late, ErrorKind::Inconsistent),
        (inverted, ErrorKind::Inconsistent),
    ] {
        assert_eq!(snapshot.validate().unwrap_err().kind, expected);
    }
}
