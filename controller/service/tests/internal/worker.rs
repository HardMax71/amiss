#![cfg(test)]

use std::time::{Duration, Instant};

use amiss_controller::ControllerClock;

use super::{add, renewal_wait, retry_delay, sleep_until, trusted_time};

struct FixedClock(Option<i64>);

impl ControllerClock for FixedClock {
    fn now_unix_millis(&self) -> Option<i64> {
        self.0
    }
}

#[test]
fn an_untrusted_clock_names_itself() {
    let defect = trusted_time(&FixedClock(None)).unwrap_err();
    assert_eq!(defect.to_string(), "controller time is unavailable");
}

#[test]
fn retry_delay_doubles_from_the_minimum_to_the_maximum() {
    let minimum = Duration::from_millis(100);
    let maximum = Duration::from_secs(10);
    assert_eq!(retry_delay(1, minimum, maximum), minimum);
    assert_eq!(retry_delay(3, minimum, maximum), Duration::from_millis(400));
    assert_eq!(retry_delay(20, minimum, maximum), maximum);
}

#[test]
fn renewal_waits_half_the_remaining_lease_bounded_by_the_poll() {
    let directory = tempfile::TempDir::new().expect("tempdir");
    let mut inbox = crate::Inbox::open(
        directory.path(),
        crate::InboxLimits {
            lease_duration: Duration::from_millis(100),
            max_records: 8,
            max_bytes: 262_144,
            max_record_bytes: 131_072,
            max_body_bytes: 4_096,
            max_headers: 8,
            max_header_bytes: 2_048,
            max_route_bytes: 128,
            max_source_id_bytes: 128,
        },
    )
    .expect("inbox opens");
    inbox
        .enqueue(crate::IncomingDelivery {
            route: "route",
            source_id: "source",
            received_at_unix_millis: 0,
            headers: &[],
            body: b"body",
        })
        .expect("enqueue");
    let crate::ClaimOutcome::Claimed(mut claimed) = inbox.claim(0).expect("claim") else {
        panic!("one pending row claims");
    };

    claimed.lease.expires_at_unix_millis = 8_000;
    assert_eq!(
        renewal_wait(&claimed.lease, &FixedClock(Some(0))),
        Ok(Duration::from_secs(4))
    );
    claimed.lease.expires_at_unix_millis = 1;
    assert_eq!(
        renewal_wait(&claimed.lease, &FixedClock(Some(0))),
        Ok(Duration::from_millis(1))
    );
    claimed.lease.expires_at_unix_millis = 60_000;
    assert_eq!(
        renewal_wait(&claimed.lease, &FixedClock(Some(0))),
        Ok(Duration::from_secs(5)),
        "the poll interval bounds the wait"
    );
}

#[test]
fn sleeping_until_a_deadline_takes_at_least_the_gap() {
    let started = Instant::now();
    sleep_until(0, 50, Duration::from_secs(1));
    assert!(started.elapsed() >= Duration::from_millis(50));
    sleep_until(50, 0, Duration::from_secs(1));
}

#[test]
fn retry_time_is_now_plus_the_duration_or_refused() {
    assert_eq!(add(1_000, Duration::from_secs(5)), Ok(6_000));
    assert!(add(i64::MAX, Duration::from_millis(1)).is_err());
}
