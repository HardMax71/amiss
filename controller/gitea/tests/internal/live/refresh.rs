use amiss_controller::ChangeState;
use amiss_wire::model::ForgeDialect;

use super::super::model::{BranchProtectionRecord, RefreshData, ReviewRecord, UserRecord};
use super::support::{
    FORGEJO_PROTECTION, FORGEJO_REPOSITORY, Fixture, GITEA_PROTECTION, GITEA_REPOSITORY, commit,
    oid,
};

#[test]
fn exact_live_snapshot_accepts_gitea_and_forgejo() {
    for namespace in ["gitea", "forgejo"] {
        let fixture = Fixture::new(namespace);
        let snapshot = fixture.client.refresh(fixture.pull_request()).unwrap();
        assert_eq!(snapshot.state, ChangeState::Active);
        assert_eq!(snapshot.run.change, fixture.change);
        assert_eq!(snapshot.run.refs.forge, ForgeDialect::Gitea);
        assert_eq!(snapshot.run.commits.base, oid('a'));
        assert_eq!(snapshot.run.commits.candidate, oid('b'));
        assert_eq!(snapshot.run.trees.base, oid('c'));
        assert_eq!(snapshot.run.trees.candidate, oid('d'));
        assert_eq!(snapshot.gate_commit, oid('b'));
    }
}

#[test]
fn protection_capabilities_are_wire_shaped_not_namespace_shaped() {
    for (namespace, protection, repository) in [
        ("gitea", FORGEJO_PROTECTION, FORGEJO_REPOSITORY),
        ("forgejo", GITEA_PROTECTION, GITEA_REPOSITORY),
    ] {
        let fixture = Fixture::mutated(namespace, |data| {
            data.protection = serde_json::from_str(protection).unwrap();
            data.repository = serde_json::from_str(repository).unwrap();
        });
        assert_eq!(
            fixture
                .client
                .refresh(fixture.pull_request())
                .unwrap()
                .state,
            ChangeState::Active
        );
    }
}

#[test]
fn manual_merge_capability_must_match_the_observed_wire_shape() {
    let gitea_missing = Fixture::mutated("gitea", |data| {
        data.repository.allow_manual_merge = None;
    });
    let forgejo_injected = Fixture::mutated("forgejo", |data| {
        data.repository.allow_manual_merge = Some(false);
    });
    assert_revoked(&gitea_missing);
    assert_revoked(&forgejo_injected);
}

#[test]
fn admin_enforcement_rejects_absent_false_and_dual_fields() {
    let absent = FORGEJO_PROTECTION.replace(",\n  \"apply_to_admins\":true", "");
    let disabled =
        FORGEJO_PROTECTION.replace("\"apply_to_admins\":true", "\"apply_to_admins\":false");
    let contradictory = GITEA_PROTECTION.replace(
        "\"block_admin_merge_override\":true",
        "\"block_admin_merge_override\":true,\n  \"apply_to_admins\":true",
    );
    for raw in [absent, disabled, contradictory] {
        let fixture = Fixture::mutated("compatible-fork", |data| {
            data.protection = serde_json::from_str(&raw).unwrap();
        });
        assert_eq!(
            fixture
                .client
                .refresh(fixture.pull_request())
                .unwrap()
                .state,
            ChangeState::AuthorizationRevoked
        );
    }
}

#[test]
fn common_push_escape_hatches_revoke_both_wire_shapes() {
    let escapes: [fn(&mut RefreshData); 7] = [
        |data| data.protection.writes.enable_push = true,
        |data| data.protection.writes.enable_push_whitelist = true,
        |data| {
            data.protection.writes.push_whitelist_usernames = vec!["writer".to_owned()];
        },
        |data| data.protection.writes.push_whitelist_teams = vec!["writers".to_owned()],
        |data| data.protection.writes.push_whitelist_deploy_keys = true,
        |data| data.protection.writes.unprotected_file_patterns = "docs/**".to_owned(),
        |data| data.repository.allow_manual_merge = Some(true),
    ];
    for mutate in escapes {
        for namespace in ["gitea", "forgejo"] {
            assert_revoked(&Fixture::mutated(namespace, mutate));
        }
    }
}

#[test]
fn gitea_force_and_bypass_escape_hatches_revoke() {
    let escapes: [fn(&mut RefreshData); 9] = [
        |data| data.protection.force.enable_force_push = Some(true),
        |data| data.protection.force.enable_force_push_allowlist = Some(true),
        |data| {
            data.protection.force.force_push_allowlist_usernames = Some(vec!["writer".to_owned()]);
        },
        |data| {
            data.protection.force.force_push_allowlist_teams = Some(vec!["writers".to_owned()]);
        },
        |data| data.protection.force.force_push_allowlist_deploy_keys = Some(true),
        |data| data.protection.bypass.enable_bypass_allowlist = Some(true),
        |data| {
            data.protection.bypass.bypass_allowlist_usernames = Some(vec!["admin".to_owned()]);
        },
        |data| {
            data.protection.bypass.bypass_allowlist_teams = Some(vec!["admins".to_owned()]);
        },
        |data| data.protection.force.enable_force_push = None,
    ];
    for mutate in escapes {
        assert_revoked(&Fixture::mutated("gitea", mutate));
    }
}

#[test]
fn forgejo_shape_rejects_injected_gitea_capabilities() {
    let fixture = Fixture::mutated("forgejo", |data| {
        data.protection.force.enable_force_push = Some(false);
    });
    assert_revoked(&fixture);
}

#[test]
fn missing_common_push_capabilities_do_not_deserialize() {
    for raw in [
        FORGEJO_PROTECTION.replace("  \"enable_push_whitelist\":false,\n", ""),
        FORGEJO_PROTECTION.replace("  \"push_whitelist_deploy_keys\":false,\n", ""),
        FORGEJO_PROTECTION.replace("  \"unprotected_file_patterns\":\"\",\n", ""),
    ] {
        assert!(serde_json::from_str::<BranchProtectionRecord>(&raw).is_err());
    }
}

#[test]
fn wrong_identity_tree_and_review_rule_fail_closed() {
    let cases: [fn(&mut RefreshData); 9] = [
        |data| data.repository.id = 999,
        |data| data.pull_request.base.repo_id = 999,
        |data| data.target_branch.commit.as_mut().unwrap().id = oid('e').as_str().to_owned(),
        |data| {
            data.candidate
                .commit
                .as_mut()
                .unwrap()
                .tree
                .as_mut()
                .unwrap()
                .sha = "bad".to_owned();
        },
        |data| data.reviewer.id = 999,
        |data| data.pull_request.head.repo_id = 0,
        |data| {
            data.protection.approvals.approvals_whitelist_usernames =
                vec!["someone-else".to_owned()];
        },
        |data| data.protection.reviews.block_on_outdated_branch = false,
        |data| data.protection.reviews.dismiss_stale_approvals = false,
    ];
    for mutate in cases {
        let fixture = Fixture::mutated("gitea", mutate);
        let result = fixture.client.refresh(fixture.pull_request());
        assert!(
            result.is_err()
                || result.is_ok_and(|snapshot| snapshot.state == ChangeState::AuthorizationRevoked)
        );
    }
}

#[test]
fn unrelated_historical_reviews_cannot_brick_the_lane() {
    let fixture = Fixture::mutated("gitea", |data| {
        data.reviews.push(ReviewRecord {
            id: 0,
            user: None,
            state: "REMOVED_PROVIDER_STATE".to_owned(),
            body: String::new(),
            commit_id: "not-an-object-id".to_owned(),
            stale: false,
            dismissed: false,
        });
        data.reviews.push(ReviewRecord {
            id: 0,
            user: Some(UserRecord {
                id: 99,
                login: "former-reviewer".to_owned(),
            }),
            state: "REMOVED_PROVIDER_STATE".to_owned(),
            body: String::new(),
            commit_id: "not-an-object-id".to_owned(),
            stale: false,
            dismissed: false,
        });
    });
    assert_eq!(
        fixture
            .client
            .refresh(fixture.pull_request())
            .unwrap()
            .state,
        ChangeState::Active
    );
}

#[test]
fn dedicated_reviewer_rows_are_strict() {
    let cases: [fn(&mut ReviewRecord); 5] = [
        |review| review.id = 0,
        |review| review.commit_id = "not-an-object-id".to_owned(),
        |review| review.state = "REMOVED_PROVIDER_STATE".to_owned(),
        |review| review.user.as_mut().unwrap().id = 99,
        |review| review.user.as_mut().unwrap().login = "other".to_owned(),
    ];
    for mutate in cases {
        let fixture = Fixture::mutated("forgejo", |data| {
            let review = ReviewRecord {
                id: 100,
                user: Some(UserRecord {
                    id: 77,
                    login: "amiss-controller".to_owned(),
                }),
                state: "APPROVED".to_owned(),
                body: "prior".to_owned(),
                commit_id: oid('b').as_str().to_owned(),
                stale: false,
                dismissed: false,
            };
            data.reviews.push(review);
            mutate(data.reviews.last_mut().unwrap());
        });
        assert_eq!(
            fixture.client.refresh(fixture.pull_request()),
            Err(amiss_controller::ProviderError::InvalidResponse)
        );
    }
}

#[test]
fn head_or_base_drift_is_superseded() {
    let stale_head = Fixture::mutated("gitea", |data| {
        data.pull_request.head.sha = oid('e').as_str().to_owned();
        data.current_head = commit('e', 'f');
    });
    assert_eq!(
        stale_head
            .client
            .refresh(stale_head.pull_request())
            .unwrap()
            .state,
        ChangeState::Superseded
    );

    let stale_base = Fixture::mutated("forgejo", |data| {
        data.pull_request.merge_base = oid('e').as_str().to_owned();
    });
    assert_eq!(
        stale_base
            .client
            .refresh(stale_base.pull_request())
            .unwrap()
            .state,
        ChangeState::Superseded
    );
}

fn assert_revoked(fixture: &Fixture) {
    assert_eq!(
        fixture
            .client
            .refresh(fixture.pull_request())
            .unwrap()
            .state,
        ChangeState::AuthorizationRevoked
    );
}
