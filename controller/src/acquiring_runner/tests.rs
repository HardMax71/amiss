#![cfg(test)]
#![allow(clippy::unwrap_used, reason = "fixed timestamp fixtures must be valid")]

use amiss_controller_fixtures::clock::TestClock;

use super::trusted_window;

#[test]
fn trusted_windows_render_the_documented_epoch_seconds() {
    for (seconds, expected) in [
        (0, "1970-01-01T00:00:00Z"),
        (951_782_400, "2000-02-29T00:00:00Z"),
        (4_102_444_799, "2099-12-31T23:59:59Z"),
        (951_868_800, "2000-03-01T00:00:00Z"),
        (4_107_456_000, "2100-02-28T00:00:00Z"),
        (4_107_542_400, "2100-03-01T00:00:00Z"),
        (946_598_400, "1999-12-31T00:00:00Z"),
        (2_147_472_000, "2038-01-19T00:00:00Z"),
    ] {
        let clock = TestClock::at(seconds * 1_000 + 999);
        let (evaluation, until) = trusted_window(Some(clock.now()), 600).unwrap();
        assert_eq!(evaluation.as_str(), expected);
        assert!(until > evaluation);
    }

    let clock = TestClock::at(0);
    let mut previous = trusted_window(Some(clock.now()), 600).unwrap().0;
    for step in 1..=800 {
        clock.set(step * 86_400_000 * 90);
        let (evaluation, _) = trusted_window(Some(clock.now()), 600).unwrap();
        assert!(evaluation > previous, "{step}");
        previous = evaluation;
    }
}

#[test]
fn trusted_windows_respect_clock_and_calendar_bounds() {
    let clock = TestClock::at(253_402_300_199_999);
    let (evaluation, until) = trusted_window(Some(clock.now()), 600).unwrap();
    assert_eq!(evaluation.as_str(), "9999-12-31T23:49:59Z");
    assert_eq!(until.as_str(), "9999-12-31T23:59:59Z");
    for millis in [-1, 253_402_300_200_000, 253_402_300_800_000, i64::MAX] {
        clock.set(millis);
        assert!(trusted_window(Some(clock.now()), 600).is_none(), "{millis}");
    }
    assert!(trusted_window(None, 600).is_none());
    clock.set(1_000);
    assert!(trusted_window(Some(clock.now()), i64::MAX).is_none());
}
