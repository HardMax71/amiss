use std::time::Duration;

use amiss_controller::{
    IngressError, IngressLimits, IngressPolicy, ReplayWindow, SignedTimePolicy,
};

use super::support::{BODY, GITHUB_HEADERS, TestClock, policy, raw, route};

#[test]
fn limits_are_checked_before_trusted_time() -> Result<(), IngressError> {
    let route = route(SignedTimePolicy::ReplayOnly);
    let limits =
        IngressLimits::new(BODY.len(), GITHUB_HEADERS.len(), 128).ok_or(IngressError::Policy)?;
    let replay = ReplayWindow::new(Duration::from_secs(1), Duration::from_millis(100))
        .ok_or(IngressError::Policy)?;
    let policy = IngressPolicy::new(limits, replay, Duration::ZERO).ok_or(IngressError::Policy)?;
    assert!(
        policy
            .pre_auth(
                raw(&route, 1_000, GITHUB_HEADERS, BODY),
                &*TestClock::at(1_000)
            )
            .is_ok()
    );

    let oversized = b"01234567890123456789";
    assert_eq!(
        policy.pre_auth(
            raw(&route, 1_000, GITHUB_HEADERS, oversized),
            &*TestClock::untrusted(),
        ),
        Err(IngressError::Limits)
    );

    let header = GITHUB_HEADERS
        .first()
        .copied()
        .ok_or(IngressError::Policy)?;
    let too_many = [header, header];
    assert_eq!(
        policy.pre_auth(raw(&route, 1_000, &too_many, BODY), &*TestClock::at(1_000),),
        Err(IngressError::Limits)
    );
    Ok(())
}

#[test]
fn receipt_window_boundaries_are_inclusive() -> Result<(), IngressError> {
    let route = route(SignedTimePolicy::ReplayOnly);
    let policy = policy(Duration::from_millis(100), Duration::from_millis(10))?;
    let clock = TestClock::at(1_000);

    for accepted in [900, 1_000, 1_010] {
        assert!(
            policy
                .pre_auth(raw(&route, accepted, GITHUB_HEADERS, BODY), &*clock)
                .is_ok()
        );
    }
    for rejected in [899, 1_011] {
        assert_eq!(
            policy.pre_auth(raw(&route, rejected, GITHUB_HEADERS, BODY), &*clock),
            Err(IngressError::Freshness)
        );
    }
    assert_eq!(
        policy.pre_auth(
            raw(&route, 1_000, GITHUB_HEADERS, BODY),
            &*TestClock::untrusted()
        ),
        Err(IngressError::Clock)
    );
    assert_eq!(
        policy.pre_auth(raw(&route, -1, GITHUB_HEADERS, BODY), &*clock),
        Err(IngressError::Clock)
    );
    Ok(())
}

#[test]
fn invalid_policy_values_fail_closed() -> Result<(), IngressError> {
    assert!(IngressLimits::new(0, 1, 1).is_none());
    let limits = IngressLimits::new(1, 1, 1).ok_or(IngressError::Policy)?;
    assert!(ReplayWindow::new(Duration::ZERO, Duration::from_millis(1)).is_none());
    assert!(ReplayWindow::new(Duration::from_millis(1), Duration::ZERO).is_none());
    assert!(ReplayWindow::new(Duration::MAX, Duration::from_millis(1)).is_none());
    let replay = ReplayWindow::new(Duration::from_millis(1), Duration::from_millis(1))
        .ok_or(IngressError::Policy)?;
    assert!(IngressPolicy::new(limits, replay, Duration::MAX).is_none());

    let policy = policy(Duration::from_millis(100), Duration::from_millis(10))?;
    for max_age in [Duration::ZERO, Duration::MAX] {
        let route = route(SignedTimePolicy::Required(max_age));
        assert_eq!(
            policy.pre_auth(
                raw(&route, 1_000, GITHUB_HEADERS, BODY),
                &*TestClock::at(1_000),
            ),
            Err(IngressError::Policy)
        );
    }

    let route = route(SignedTimePolicy::Required(Duration::from_secs(101)));
    assert_eq!(
        policy.pre_auth(
            raw(&route, 1_000, GITHUB_HEADERS, BODY),
            &*TestClock::at(1_000),
        ),
        Err(IngressError::Policy)
    );
    Ok(())
}

#[test]
fn ceilings_report_what_they_were_given() {
    let limits = IngressLimits::new(4_096, 8, 2_048).expect("valid ceilings");
    assert_eq!(limits.max_body_bytes(), 4_096);
    assert_eq!(limits.max_header_count(), 8);
    assert_eq!(limits.max_header_bytes(), 2_048);
}

/// The epoch is an ordinary instant, and only a clock behind it is untrusted.
#[test]
fn the_epoch_is_trusted_and_a_clock_behind_it_is_not() -> Result<(), IngressError> {
    let route = route(SignedTimePolicy::ReplayOnly);
    let policy = policy(Duration::from_millis(100), Duration::from_millis(10))?;
    assert!(
        policy
            .pre_auth(raw(&route, 0, GITHUB_HEADERS, BODY), &*TestClock::at(0))
            .is_ok(),
        "a receipt at the epoch under a clock at the epoch"
    );
    assert_eq!(
        policy.pre_auth(raw(&route, 0, GITHUB_HEADERS, BODY), &*TestClock::at(-1)),
        Err(IngressError::Clock),
        "a clock behind the epoch is not a time"
    );
    Ok(())
}

#[test]
fn every_ingress_refusal_names_itself() {
    for (error, message) in [
        (IngressError::Clock, "controller time cannot be trusted"),
        (
            IngressError::Limits,
            "provider delivery exceeds an ingress ceiling",
        ),
        (IngressError::Policy, "provider ingress policy is invalid"),
        (
            IngressError::Request,
            "provider proof does not bind this request",
        ),
        (
            IngressError::Route,
            "authenticated delivery does not match its route",
        ),
        (
            IngressError::Freshness,
            "provider delivery is outside its freshness window",
        ),
        (IngressError::Replay, "provider replay identity is invalid"),
    ] {
        assert_eq!(error.to_string(), message);
    }
}
