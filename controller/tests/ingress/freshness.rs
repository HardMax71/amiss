use std::time::Duration;

use amiss_controller::{
    IngressError, IngressLimits, IngressPolicy, ReplayWindow, SignedTimePolicy,
};

use super::support::{
    BODY, FixedClock, GITHUB_HEADERS, GITLAB_BODY, GITLAB_HEADERS, GITLAB_NOW, github_verified,
    gitlab_verified, policy, raw, route,
};

#[test]
fn required_signed_time_boundaries_are_inclusive() {
    let route = route(SignedTimePolicy::Required(Duration::from_secs(100)));
    let policy = policy(Duration::from_secs(200), Duration::from_secs(10));

    for received_at in [GITLAB_NOW - 10_000, GITLAB_NOW, GITLAB_NOW + 100_000] {
        let check = policy
            .pre_auth(
                raw(&route, received_at, GITLAB_HEADERS, GITLAB_BODY),
                &FixedClock(Some(received_at)),
            )
            .unwrap();
        let verified = gitlab_verified(check, &route.provider);
        assert!(policy.post_auth(check, verified).is_ok());
    }

    for received_at in [GITLAB_NOW - 10_001, GITLAB_NOW + 100_001] {
        let check = policy
            .pre_auth(
                raw(&route, received_at, GITLAB_HEADERS, GITLAB_BODY),
                &FixedClock(Some(received_at)),
            )
            .unwrap();
        let verified = gitlab_verified(check, &route.provider);
        assert_eq!(
            policy.post_auth(check, verified),
            Err(IngressError::Freshness)
        );
    }
}

#[test]
fn a_required_time_cannot_be_missing() {
    let route = route(SignedTimePolicy::Required(Duration::from_secs(100)));
    let policy = policy(Duration::from_secs(200), Duration::from_secs(10));
    let check = policy
        .pre_auth(
            raw(&route, GITLAB_NOW, GITHUB_HEADERS, BODY),
            &FixedClock(Some(GITLAB_NOW)),
        )
        .unwrap();
    let verified = github_verified(check, &route.provider, route.trust_set.clone());

    assert_eq!(
        policy.post_auth(check, verified),
        Err(IngressError::Freshness)
    );
}

#[test]
fn a_signed_time_cannot_be_downgraded_to_replay_only() {
    let route = route(SignedTimePolicy::ReplayOnly);
    let policy = policy(Duration::from_secs(200), Duration::from_secs(10));
    let check = policy
        .pre_auth(
            raw(&route, GITLAB_NOW, GITLAB_HEADERS, GITLAB_BODY),
            &FixedClock(Some(GITLAB_NOW)),
        )
        .unwrap();
    let verified = gitlab_verified(check, &route.provider);

    assert_eq!(policy.post_auth(check, verified), Err(IngressError::Policy));
}

#[test]
fn replay_lifetime_uses_the_fixed_window_ceiling() {
    let route = route(SignedTimePolicy::Required(Duration::from_secs(1)));
    let policy = policy(Duration::from_secs(200), Duration::from_secs(10));
    let check = policy
        .pre_auth(
            raw(&route, GITLAB_NOW, GITLAB_HEADERS, GITLAB_BODY),
            &FixedClock(Some(GITLAB_NOW)),
        )
        .unwrap();
    let accepted = policy
        .post_auth(check, gitlab_verified(check, &route.provider))
        .unwrap();

    assert_eq!(
        accepted.replay_keep_through_unix_millis(),
        Some(GITLAB_NOW + 300_000)
    );
}

#[test]
fn an_unrepresentable_replay_lifetime_fails_closed() {
    let route = route(SignedTimePolicy::Required(Duration::from_secs(100)));
    let maximum = u64::try_from(i64::MAX).unwrap();
    let replay =
        ReplayWindow::new(Duration::from_millis(maximum), Duration::from_millis(1)).unwrap();
    let policy = IngressPolicy::new(
        IngressLimits::new(1_024, 16, 2_048).unwrap(),
        replay,
        Duration::from_secs(10),
    )
    .unwrap();
    let check = policy
        .pre_auth(
            raw(&route, GITLAB_NOW, GITLAB_HEADERS, GITLAB_BODY),
            &FixedClock(Some(GITLAB_NOW)),
        )
        .unwrap();

    assert_eq!(
        policy.post_auth(check, gitlab_verified(check, &route.provider)),
        Err(IngressError::Replay)
    );
}
