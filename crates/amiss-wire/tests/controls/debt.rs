use amiss_wire::controls::DebtSnapshot;
use amiss_wire::de::ErrorKind;

use crate::support::{computed_digests, debt_item, debt_snapshot, flip_last};

#[test]
fn parses_a_valid_debt_snapshot() {
    let (key, fact) = computed_digests();
    let item = debt_item(
        "debt/readme",
        &key,
        &fact,
        "2026-07-01T00:00:00Z",
        "2026-08-01T00:00:00Z",
    );
    let doc = debt_snapshot("2026-07-02T00:00:00Z", &[item]);
    let snapshot = DebtSnapshot::parse(doc.as_bytes()).unwrap();
    assert_eq!(snapshot.schema(), "amiss/debt-snapshot");
    assert_eq!(snapshot.items().len(), 1);
    assert_eq!(
        snapshot.items().first().unwrap().finding_key.to_string(),
        key
    );
}

#[test]
fn an_item_born_at_the_snapshot_instant_is_consistent() {
    let (key, fact) = computed_digests();
    let item = debt_item(
        "debt/readme",
        &key,
        &fact,
        "2026-07-02T00:00:00Z",
        "2026-08-01T00:00:00Z",
    );
    let doc = debt_snapshot("2026-07-02T00:00:00Z", &[item]);
    let snapshot = DebtSnapshot::parse(doc.as_bytes())
        .expect("an item created at the snapshot instant is not from the future");
    assert_eq!(snapshot.items().len(), 1);
}

#[test]
fn rejects_debt_digest_and_order_defects() {
    let (key, fact) = computed_digests();

    let bad_key = debt_item(
        "debt/readme",
        &flip_last(&key),
        &fact,
        "2026-07-01T00:00:00Z",
        "2026-08-01T00:00:00Z",
    );
    let doc = debt_snapshot("2026-07-02T00:00:00Z", &[bad_key]);
    assert_eq!(
        DebtSnapshot::parse(doc.as_bytes()).unwrap_err().kind,
        ErrorKind::DigestMismatch
    );

    let bad_fact = debt_item(
        "debt/readme",
        &key,
        &flip_last(&fact),
        "2026-07-01T00:00:00Z",
        "2026-08-01T00:00:00Z",
    );
    let doc = debt_snapshot("2026-07-02T00:00:00Z", &[bad_fact]);
    assert_eq!(
        DebtSnapshot::parse(doc.as_bytes()).unwrap_err().kind,
        ErrorKind::DigestMismatch
    );

    let first = debt_item(
        "debt/b",
        &key,
        &fact,
        "2026-07-01T00:00:00Z",
        "2026-08-01T00:00:00Z",
    );
    let second = debt_item(
        "debt/a",
        &key,
        &fact,
        "2026-07-01T00:00:00Z",
        "2026-08-01T00:00:00Z",
    );
    let doc = debt_snapshot("2026-07-02T00:00:00Z", &[first, second]);
    assert_eq!(
        DebtSnapshot::parse(doc.as_bytes()).unwrap_err().kind,
        ErrorKind::UnsortedSet
    );

    let first = debt_item(
        "debt/a",
        &key,
        &fact,
        "2026-07-01T00:00:00Z",
        "2026-08-01T00:00:00Z",
    );
    let second = debt_item(
        "debt/b",
        &key,
        &fact,
        "2026-07-01T00:00:00Z",
        "2026-08-01T00:00:00Z",
    );
    let doc = debt_snapshot("2026-07-02T00:00:00Z", &[first, second]);
    assert_eq!(
        DebtSnapshot::parse(doc.as_bytes()).unwrap_err().kind,
        ErrorKind::DuplicateMember
    );

    let late = debt_item(
        "debt/readme",
        &key,
        &fact,
        "2026-07-03T00:00:00Z",
        "2026-08-01T00:00:00Z",
    );
    let doc = debt_snapshot("2026-07-02T00:00:00Z", &[late]);
    assert_eq!(
        DebtSnapshot::parse(doc.as_bytes()).unwrap_err().kind,
        ErrorKind::Inconsistent
    );

    let inverted = debt_item(
        "debt/readme",
        &key,
        &fact,
        "2026-08-01T00:00:00Z",
        "2026-07-01T00:00:00Z",
    );
    let doc = debt_snapshot("2026-08-02T00:00:00Z", &[inverted]);
    assert_eq!(
        DebtSnapshot::parse(doc.as_bytes()).unwrap_err().kind,
        ErrorKind::Inconsistent
    );
}
