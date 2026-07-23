use std::fs;
use std::time::Duration;

use amiss_controller_service::{
    ClaimOutcome, EnqueueOutcome, Inbox, InboxError, InboxState, IncomingDelivery, IncomingHeader,
};
use tempfile::TempDir;

use super::support::{claimed, incoming, incoming_at, limits, open, row_file};

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
    byte_limits.max_bytes = 160_000;
    let mut bytes = Inbox::open(byte_directory.path(), byte_limits).unwrap();
    bytes.enqueue(incoming("delivery-1", b"body")).unwrap();
    assert!(matches!(
        bytes.enqueue(incoming("delivery-2", b"body")),
        Err(InboxError::Full)
    ));
    assert_eq!(bytes.entries().unwrap().len(), 1);
}

#[test]
fn maximum_valid_delivery_can_be_claimed_after_acknowledgement() {
    let directory = TempDir::new().unwrap();
    let mut bounded = limits();
    bounded.max_record_bytes = 80_000;
    bounded.max_bytes = 160_000;
    let mut inbox = Inbox::open(directory.path(), bounded).unwrap();
    let route = "\"".repeat(128);
    let source_id = "\\".repeat(128);
    let names = ["X-A", "X-B", "X-C", "X-D", "X-E", "X-F", "X-G", "X-H"];
    let values = (0..8).map(|_| vec![b'x'; 253]).collect::<Vec<_>>();
    let headers = names
        .iter()
        .zip(&values)
        .map(|(name, value)| IncomingHeader {
            name,
            value: value.as_slice(),
        })
        .collect::<Vec<_>>();
    let body = vec![b'x'; 4_096];

    assert_eq!(
        inbox
            .enqueue(IncomingDelivery {
                route: &route,
                source_id: &source_id,
                received_at_unix_millis: 0,
                headers: &headers,
                body: &body,
            })
            .unwrap(),
        EnqueueOutcome::Stored
    );
    let claimed = claimed(inbox.claim(0).unwrap());
    assert_eq!(claimed.delivery.route, route);
    assert_eq!(claimed.delivery.source_id, source_id);
    assert_eq!(claimed.delivery.headers.len(), 8);
    assert_eq!(claimed.delivery.body.len(), 4_096);
}

#[test]
fn existing_dense_roots_drain_while_new_admission_uses_the_reservation() {
    let directory = TempDir::new().unwrap();
    let second_directory = TempDir::new().unwrap();
    let mut bounded = limits();
    bounded.max_record_bytes = 80_000;
    bounded.max_bytes = 160_000;
    let mut first = Inbox::open(directory.path(), bounded).unwrap();
    first.enqueue(incoming("delivery-1", b"one")).unwrap();
    drop(first);
    let mut second = Inbox::open(second_directory.path(), bounded).unwrap();
    second.enqueue(incoming("delivery-2", b"two")).unwrap();
    drop(second);
    let second_row = row_file(second_directory.path());
    fs::copy(
        &second_row,
        directory.path().join(second_row.file_name().unwrap()),
    )
    .unwrap();

    let mut reopened = Inbox::open(directory.path(), bounded).unwrap();
    assert!(matches!(
        reopened.enqueue(incoming("delivery-3", b"three")),
        Err(InboxError::Full)
    ));
    for _ in 0..2 {
        let claimed = claimed(reopened.claim(0).unwrap());
        assert_eq!(
            reopened.complete(&claimed.lease, 0).unwrap(),
            amiss_controller_service::CompleteOutcome::Completed
        );
    }
    assert!(matches!(reopened.claim(0).unwrap(), ClaimOutcome::Empty));
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
    let mut invalid = limits();
    invalid.lease_duration = Duration::ZERO;
    assert!(matches!(
        Inbox::open(directory.path(), invalid),
        Err(InboxError::Configuration)
    ));
    for limits in [
        amiss_controller_service::InboxLimits {
            max_records: 1_025,
            ..limits()
        },
        amiss_controller_service::InboxLimits {
            max_bytes: 128 * 1_024 * 1_024 + 1,
            ..limits()
        },
        amiss_controller_service::InboxLimits {
            max_record_bytes: 16 * 1_024 * 1_024 + 1,
            ..limits()
        },
        amiss_controller_service::InboxLimits {
            max_record_bytes: 256,
            ..limits()
        },
        amiss_controller_service::InboxLimits {
            max_body_bytes: u64::MAX,
            ..limits()
        },
    ] {
        assert!(matches!(
            Inbox::open(directory.path(), limits),
            Err(InboxError::Configuration)
        ));
    }
}
