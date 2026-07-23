#![cfg(test)]

use super::remaining_timeout;
use std::time::Duration;

#[test]
fn fetch_deadline_decreases_and_expires() {
    let limit = Duration::from_secs(10);
    assert_eq!(
        remaining_timeout(limit, Duration::from_secs(3)),
        Some(Duration::from_secs(7))
    );
    assert_eq!(remaining_timeout(limit, limit), None);
    assert_eq!(remaining_timeout(limit, Duration::from_secs(11)), None);
}
