use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use amiss_controller_service::{
    ClaimOutcome, ClaimedDelivery, Inbox, InboxLimits, IncomingDelivery, IncomingHeader,
};

static HEADERS: [IncomingHeader<'static>; 2] = [
    IncomingHeader {
        name: "X-Delivery",
        value: b"delivery-1",
    },
    IncomingHeader {
        name: "X-Signature",
        value: b"sha256=1234",
    },
];

pub(crate) fn limits() -> InboxLimits {
    InboxLimits {
        lease_duration: Duration::from_millis(100),
        max_records: 8,
        max_bytes: 262_144,
        max_record_bytes: 131_072,
        max_body_bytes: 4_096,
        max_headers: 8,
        max_header_bytes: 2_048,
        max_route_bytes: 128,
        max_source_id_bytes: 128,
    }
}

pub(crate) fn incoming<'a>(source_id: &'a str, body: &'a [u8]) -> IncomingDelivery<'a> {
    incoming_at(source_id, body, 1_000)
}

pub(crate) fn incoming_at<'a>(
    source_id: &'a str,
    body: &'a [u8],
    received_at_unix_millis: i64,
) -> IncomingDelivery<'a> {
    IncomingDelivery {
        route: "github-main",
        source_id,
        received_at_unix_millis,
        headers: &HEADERS,
        body,
    }
}

pub(crate) fn claimed(outcome: ClaimOutcome) -> ClaimedDelivery {
    let ClaimOutcome::Claimed(claimed) = outcome else {
        panic!("expected a claimed delivery");
    };
    claimed
}

pub(crate) fn row_file(root: &Path) -> PathBuf {
    fs::read_dir(root)
        .unwrap()
        .map(Result::unwrap)
        .find(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("row"))
        })
        .unwrap()
        .path()
}

pub(crate) fn open(root: &Path) -> Inbox {
    Inbox::open(root, limits()).unwrap()
}
