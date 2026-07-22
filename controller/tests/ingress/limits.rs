use std::time::Duration;

use amiss_controller::{
    IngressError, IngressLimits, IngressPolicy, ReplayWindow, SignedTimePolicy,
};

use super::support::{BODY, FixedClock, GITHUB_HEADERS, policy, raw, route};

#[test]
fn limits_are_checked_before_trusted_time() {
    let route = route(SignedTimePolicy::ReplayOnly);
    let limits = IngressLimits::new(BODY.len(), GITHUB_HEADERS.len(), 128).unwrap();
    let replay = ReplayWindow::new(Duration::from_secs(1), Duration::from_millis(100)).unwrap();
    let policy = IngressPolicy::new(limits, replay, Duration::ZERO).unwrap();
    assert!(
        policy
            .pre_auth(
                raw(&route, 1_000, GITHUB_HEADERS, BODY),
                &FixedClock(Some(1_000))
            )
            .is_ok()
    );

    let oversized = b"01234567890123456789";
    assert_eq!(
        policy.pre_auth(
            raw(&route, 1_000, GITHUB_HEADERS, oversized),
            &FixedClock(None),
        ),
        Err(IngressError::Limits)
    );

    let header = GITHUB_HEADERS.first().copied().unwrap();
    let too_many = [header, header];
    assert_eq!(
        policy.pre_auth(
            raw(&route, 1_000, &too_many, BODY),
            &FixedClock(Some(1_000)),
        ),
        Err(IngressError::Limits)
    );
}

#[test]
fn receipt_window_boundaries_are_inclusive() {
    let route = route(SignedTimePolicy::ReplayOnly);
    let policy = policy(Duration::from_millis(100), Duration::from_millis(10));
    let clock = FixedClock(Some(1_000));

    for accepted in [900, 1_000, 1_010] {
        assert!(
            policy
                .pre_auth(raw(&route, accepted, GITHUB_HEADERS, BODY), &clock)
                .is_ok()
        );
    }
    for rejected in [899, 1_011] {
        assert_eq!(
            policy.pre_auth(raw(&route, rejected, GITHUB_HEADERS, BODY), &clock),
            Err(IngressError::Freshness)
        );
    }
    assert_eq!(
        policy.pre_auth(raw(&route, 1_000, GITHUB_HEADERS, BODY), &FixedClock(None)),
        Err(IngressError::Clock)
    );
    assert_eq!(
        policy.pre_auth(raw(&route, -1, GITHUB_HEADERS, BODY), &clock),
        Err(IngressError::Clock)
    );
}

#[test]
fn invalid_policy_values_fail_closed() {
    assert!(IngressLimits::new(0, 1, 1).is_none());
    let limits = IngressLimits::new(1, 1, 1).unwrap();
    assert!(ReplayWindow::new(Duration::ZERO, Duration::from_millis(1)).is_none());
    assert!(ReplayWindow::new(Duration::from_millis(1), Duration::ZERO).is_none());
    assert!(ReplayWindow::new(Duration::MAX, Duration::from_millis(1)).is_none());
    let replay = ReplayWindow::new(Duration::from_millis(1), Duration::from_millis(1)).unwrap();
    assert!(IngressPolicy::new(limits, replay, Duration::MAX).is_none());

    let policy = policy(Duration::from_millis(100), Duration::from_millis(10));
    for max_age in [Duration::ZERO, Duration::MAX] {
        let route = route(SignedTimePolicy::Required(max_age));
        assert_eq!(
            policy.pre_auth(
                raw(&route, 1_000, GITHUB_HEADERS, BODY),
                &FixedClock(Some(1_000)),
            ),
            Err(IngressError::Policy)
        );
    }

    let route = route(SignedTimePolicy::Required(Duration::from_secs(101)));
    assert_eq!(
        policy.pre_auth(
            raw(&route, 1_000, GITHUB_HEADERS, BODY),
            &FixedClock(Some(1_000)),
        ),
        Err(IngressError::Policy)
    );
}
