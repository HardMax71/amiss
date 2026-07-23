use std::time::Duration;

use amiss_controller_service::{
    ClaimOutcome, EnqueueOutcome, Inbox, InboxError, InboxState, IncomingDelivery, IncomingHeader,
};
use tempfile::TempDir;

use super::support::{claimed, incoming, incoming_at, limits, open};

#[test]
fn delivery_survives_restart_and_is_enumerable() {
    let directory = TempDir::new().unwrap();
    let mut inbox = open(directory.path());
    assert_eq!(
        inbox
            .enqueue(incoming("delivery-1", b"{\"pull\":1}"))
            .unwrap(),
        EnqueueOutcome::Stored
    );
    assert_eq!(
        inbox.entries().unwrap(),
        vec![amiss_controller_service::InboxEntry {
            route: "github-main".to_owned(),
            source_id: "delivery-1".to_owned(),
            state: InboxState::Pending {
                attempts: 0,
                available_at_unix_millis: 0,
            },
        }]
    );
    drop(inbox);

    let mut reopened = open(directory.path());
    let delivery = claimed(reopened.claim(10).unwrap());
    assert_eq!(delivery.delivery.route, "github-main");
    assert_eq!(delivery.delivery.source_id, "delivery-1");
    assert_eq!(delivery.delivery.body, b"{\"pull\":1}");
    assert_eq!(delivery.delivery.headers[0].name, "x-delivery");
}

#[test]
fn stable_source_is_repeat_safe_and_conflicts_fail_closed() {
    let directory = TempDir::new().unwrap();
    let mut inbox = open(directory.path());
    assert_eq!(
        inbox.enqueue(incoming("delivery-1", b"same")).unwrap(),
        EnqueueOutcome::Stored
    );
    assert_eq!(
        inbox
            .enqueue(incoming_at("delivery-1", b"same", 2_000))
            .unwrap(),
        EnqueueOutcome::Duplicate
    );
    assert!(matches!(
        inbox.enqueue(incoming("delivery-1", b"different")),
        Err(InboxError::Conflict)
    ));
}

#[test]
fn record_and_byte_capacity_are_enforced_before_writing() {
    let record_directory = TempDir::new().unwrap();
    let mut record_limits = limits();
    record_limits.max_records = 1;
    let mut records = Inbox::open(record_directory.path(), record_limits).unwrap();
    records.enqueue(incoming("delivery-1", b"one")).unwrap();
    assert!(matches!(
        records.enqueue(incoming("delivery-2", b"two")),
        Err(InboxError::Full)
    ));

    let byte_directory = TempDir::new().unwrap();
    let mut byte_limits = limits();
    byte_limits.max_record_bytes = 256;
    byte_limits.max_bytes = 512;
    let mut bytes = Inbox::open(byte_directory.path(), byte_limits).unwrap();
    assert!(matches!(
        bytes.enqueue(incoming("delivery-1", b"body")),
        Err(InboxError::Full)
    ));
    assert!(bytes.entries().unwrap().is_empty());
}

#[test]
fn component_limits_reject_before_copying_or_persisting() {
    let directory = TempDir::new().unwrap();
    let mut limits = limits();
    limits.max_body_bytes = 3;
    limits.max_headers = 1;
    limits.max_header_bytes = 8;
    limits.max_route_bytes = 4;
    limits.max_source_id_bytes = 4;
    let mut inbox = Inbox::open(directory.path(), limits).unwrap();
    let headers = [IncomingHeader {
        name: "X-Test",
        value: b"value",
    }];

    for delivery in [
        IncomingDelivery {
            route: "toolong",
            source_id: "id",
            received_at_unix_millis: 0,
            headers: &[],
            body: b"",
        },
        IncomingDelivery {
            route: "r",
            source_id: "long-id",
            received_at_unix_millis: 0,
            headers: &[],
            body: b"",
        },
        IncomingDelivery {
            route: "r",
            source_id: "id",
            received_at_unix_millis: 0,
            headers: &[],
            body: b"four",
        },
        IncomingDelivery {
            route: "r",
            source_id: "id",
            received_at_unix_millis: 0,
            headers: &headers,
            body: b"",
        },
        IncomingDelivery {
            route: "r",
            source_id: "id",
            received_at_unix_millis: -1,
            headers: &[],
            body: b"",
        },
    ] {
        assert!(matches!(
            inbox.enqueue(delivery),
            Err(InboxError::InvalidDelivery)
        ));
    }
    assert!(matches!(inbox.claim(0).unwrap(), ClaimOutcome::Empty));
}

#[test]
fn impossible_limits_are_configuration_errors() {
    let directory = TempDir::new().unwrap();
    let mut limits = limits();
    limits.lease_duration = Duration::ZERO;
    assert!(matches!(
        Inbox::open(directory.path(), limits),
        Err(InboxError::Configuration)
    ));
}
