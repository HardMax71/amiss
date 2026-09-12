#![cfg(test)]

use std::time::Duration;

use super::{Delivery, IncomingDelivery, IncomingHeader, StoredLimits};
use crate::InboxLimits;

#[test]
fn stored_headers_must_be_normalized_and_within_limits() {
    let limits = StoredLimits::read(InboxLimits {
        lease_duration: Duration::from_secs(1),
        max_records: 1,
        max_bytes: 262_144,
        max_record_bytes: 131_072,
        max_body_bytes: 4,
        max_headers: 3,
        max_header_bytes: 12,
        max_route_bytes: 1,
        max_source_id_bytes: 1,
    })
    .unwrap();
    let mut delivery = Delivery::read(
        IncomingDelivery {
            route: "r",
            source_id: "s",
            received_at_unix_millis: 0,
            headers: &[
                IncomingHeader {
                    name: "X-A",
                    value: b"b",
                },
                IncomingHeader {
                    name: "x-a",
                    value: b"a",
                },
                IncomingHeader {
                    name: "X-A",
                    value: b"a",
                },
            ],
            body: b"body",
        },
        limits,
    )
    .unwrap();
    assert!(delivery.validate(limits).is_ok());
    assert_eq!(delivery.headers[0].value, b"a");
    assert_eq!(delivery.headers[1].value, b"a");
    assert_eq!(delivery.headers[2].value, b"b");

    delivery.headers.swap(0, 2);
    assert!(delivery.validate(limits).is_err());
    delivery.headers.swap(0, 2);
    delivery.headers[0].name.make_ascii_uppercase();
    assert!(delivery.validate(limits).is_err());
    delivery.headers[0].name.make_ascii_lowercase();
    delivery.headers[2].value.push(b'c');
    assert!(delivery.validate(limits).is_err());
    delivery.headers[2].value.pop();
    delivery.headers[2].value[0] = b'\n';
    assert!(delivery.validate(limits).is_err());
    delivery.headers[2].value[0] = b'b';
    delivery.body.push(b'!');
    assert!(delivery.validate(limits).is_err());
    delivery.body.pop();
    assert!(delivery.validate(limits).is_ok());
}
