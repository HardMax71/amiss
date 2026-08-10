#![cfg(test)]

use std::collections::BTreeSet;

use amiss_controller::{ChangeState, CheckConclusion, OpaqueId, ProviderError, RunFailure};
use amiss_wire::model::{ObjectFormat, Oid};

use super::{conclusion_matches, train_matches, wildcard_matches};
use crate::{GitLabRefreshQuery, GitLabTrainCar, PolicyBinding, RunnerTrust};

fn oid(fill: char) -> Oid {
    Oid::new(ObjectFormat::Sha1, fill.to_string().repeat(40)).expect("an object id")
}

fn query() -> GitLabRefreshQuery {
    GitLabRefreshQuery {
        project_id: 101,
        merge_request_iid: 42,
        pipeline_id: 202,
        job_id: 303,
        runner_id: 77,
        gate_commit: oid('b'),
    }
}

fn policy() -> PolicyBinding {
    PolicyBinding {
        integration: OpaqueId::new("policy/1".to_owned()).expect("an integration id"),
        project_id: 101,
        project_path: "platform/security".to_owned(),
        target_branch: "main".to_owned(),
        job_name: "amiss:policy".to_owned(),
        config_url: "https://gitlab.example/policy.yml".to_owned(),
        config_commit: oid('f'),
        runners: RunnerTrust {
            gitlab_hosted: true,
            self_hosted_ids: BTreeSet::from([77]),
        },
    }
}

fn car() -> GitLabTrainCar {
    GitLabTrainCar {
        id: 9,
        status: "idle".to_owned(),
        target_branch: "main".to_owned(),
        merge_request_iid: 42,
        merge_request_project_id: 101,
        merge_request_state: "opened".to_owned(),
        pipeline_id: 202,
        pipeline_project_id: 101,
        pipeline_sha: oid('b').as_str().to_owned(),
        pipeline_ref: crate::identity::train_ref(42),
        pipeline_source: "merge_request_event".to_owned(),
        pipeline_status: "running".to_owned(),
    }
}

#[test]
fn a_conclusion_names_exactly_one_state() {
    let states = [
        ChangeState::Active,
        ChangeState::Superseded,
        ChangeState::Closed,
        ChangeState::AuthorizationRevoked,
    ];
    let rows = [
        (CheckConclusion::Pass, ChangeState::Active),
        (CheckConclusion::Block, ChangeState::Active),
        (CheckConclusion::Superseded, ChangeState::Superseded),
        (
            CheckConclusion::Unavailable(RunFailure::AuthorizationRevoked),
            ChangeState::AuthorizationRevoked,
        ),
        (
            CheckConclusion::Unavailable(RunFailure::Closed),
            ChangeState::Closed,
        ),
    ];
    for (conclusion, expected) in rows {
        for state in states {
            assert_eq!(
                conclusion_matches(state, conclusion),
                state == expected,
                "{conclusion:?} against {state:?}"
            );
        }
    }
}

#[test]
fn a_train_car_answers_for_every_closed_vocabulary() {
    assert_eq!(train_matches(&query(), &policy(), &car()), Ok(true));

    for state in ["closed", "locked", "merged"] {
        let mut closed = car();
        closed.merge_request_state = state.to_owned();
        assert_eq!(
            train_matches(&query(), &policy(), &closed),
            Ok(false),
            "{state} is a known state that is not open"
        );
    }
    let mut foreign = car();
    foreign.merge_request_state = "reopened".to_owned();
    assert_eq!(
        train_matches(&query(), &policy(), &foreign),
        Err(ProviderError::InvalidResponse),
        "a state outside the vocabulary"
    );

    for status in [
        "canceled",
        "created",
        "failed",
        "manual",
        "pending",
        "preparing",
        "scheduled",
        "skipped",
        "success",
        "waiting_for_callback",
        "waiting_for_resource",
    ] {
        let mut settled = car();
        settled.pipeline_status = status.to_owned();
        assert_eq!(
            train_matches(&query(), &policy(), &settled),
            Ok(false),
            "{status} is a known status that is not running"
        );
    }
    let mut foreign = car();
    foreign.pipeline_status = "dancing".to_owned();
    assert_eq!(
        train_matches(&query(), &policy(), &foreign),
        Err(ProviderError::InvalidResponse),
        "a status outside the vocabulary"
    );

    let mut unnumbered = car();
    unnumbered.id = 0;
    assert_eq!(
        train_matches(&query(), &policy(), &unnumbered),
        Ok(false),
        "a car nobody numbered"
    );
}

#[test]
fn a_wildcard_matches_only_what_its_shape_allows() {
    for (pattern, value) in [
        ("main", "main"),
        ("*", "anything"),
        ("*", ""),
        ("release/*", "release/1.2"),
        ("*-stable", "v1-stable"),
        ("release/*/rc", "release/9/rc"),
        ("*mid*", "a-mid-z"),
    ] {
        assert!(wildcard_matches(pattern, value), "{pattern} over {value}");
    }
    for (pattern, value) in [
        ("main", "maintenance"),
        ("main", "premain"),
        ("release/*", "hotfix/1.2"),
        ("*-stable", "v1-stable-rc"),
        ("release/*/rc", "release/9/beta"),
        ("*mid*", "a-end-z"),
        ("", "main"),
    ] {
        assert!(!wildcard_matches(pattern, value), "{pattern} over {value}");
    }
    assert!(wildcard_matches("", ""));
}
