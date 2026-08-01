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

/// Rewrites the one row frame around an edited payload, resealing the
/// length and domain digest so only the semantic defect under test remains.
#[expect(clippy::unwrap_used, reason = "test fixture helper")]
pub(crate) fn reseal_row(root: &Path, edit: impl FnOnce(&str) -> String) {
    use sha2::{Digest as _, Sha256};
    let path = row_file(root);
    let bytes = fs::read(&path).unwrap();
    let magic = b"AMISS-INBOX-ROW";
    let payload_start = magic.len().checked_add(1 + 8 + 32).unwrap();
    let payload = std::str::from_utf8(bytes.get(payload_start..).unwrap()).unwrap();
    let edited = edit(payload);
    let mut hasher = Sha256::new();
    hasher.update(b"amiss/controller-inbox-row-frame-v1");
    hasher.update([0]);
    hasher.update(edited.as_bytes());
    let mut frame = Vec::new();
    frame.extend_from_slice(magic);
    frame.push(1);
    frame.extend_from_slice(&u64::try_from(edited.len()).unwrap().to_be_bytes());
    frame.extend_from_slice(&hasher.finalize());
    frame.extend_from_slice(edited.as_bytes());
    fs::write(path, frame).unwrap();
}
