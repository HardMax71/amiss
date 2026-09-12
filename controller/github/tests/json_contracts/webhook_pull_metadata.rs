use amiss_controller_github::pull::State;
use amiss_controller_github::pull::metadata::{AutoMergeRecord, MergeMethod, MilestoneRecord};
use amiss_controller_github::webhook::pull::{
    ParentTeam, PullRequestAccountKind, ReviewTeam, Reviewer, Team, TeamNotifications, TeamPrivacy,
};
use amiss_controller_github::webhook::repository::WorkflowOwner;
use amiss_wire::assessment::Nullable;

#[test]
fn pull_request_accounts_extend_only_their_own_kind_contract() {
    for (kind, spelling) in [
        (PullRequestAccountKind::Bot, "Bot"),
        (PullRequestAccountKind::User, "User"),
        (PullRequestAccountKind::Organization, "Organization"),
        (PullRequestAccountKind::Mannequin, "Mannequin"),
    ] {
        let input = format!(r#"{{"login":"reviewer","id":1,"type":"{spelling}"}}"#);
        let user: WorkflowOwner<PullRequestAccountKind> = serde_json::from_str(&input).unwrap();
        assert_eq!(user.kind, Some(kind));
        let reviewer: Reviewer = amiss_wire::read_json(input.as_bytes(), u64::MAX).unwrap();
        assert_eq!(reviewer, Reviewer::Account(Box::new(user)));
        assert_eq!(
            serde_json::from_str::<WorkflowOwner>(&input).is_ok(),
            kind != PullRequestAccountKind::Mannequin
        );
        assert_eq!(
            amiss_fixtures::canonical_json(&serde_json::to_vec(&reviewer).unwrap()).unwrap(),
            amiss_fixtures::canonical_json(input.as_bytes()).unwrap(),
        );
    }
    for input in [
        r#"{"login":"reviewer","id":1,"type":"unknown"}"#,
        r#"{"login":"reviewer","id":1,"type":{"Mannequin":null}}"#,
        r#"{"login":"reviewer","id":1,"unknown":true}"#,
        r#"{"login":"reviewer","id":1,"name":"team","slug":"team"}"#,
    ] {
        assert!(serde_json::from_str::<Reviewer>(input).is_err(), "{input}");
    }
}

#[test]
fn team_profiles_preserve_complete_metadata_without_relaxing_reviewers() {
    let minimal = r#"{"name":"docs","id":1}"#;
    assert!(serde_json::from_str::<Team>(minimal).is_ok());
    assert!(serde_json::from_str::<ReviewTeam>(minimal).is_err());
    assert!(serde_json::from_str::<Reviewer>(minimal).is_err());
    let parent = ParentTeam {
        name: "docs".to_owned(),
        id: js_int::uint!(1),
        node_id: "team-one".to_owned(),
        slug: "docs".to_owned(),
        description: Nullable::Null,
        privacy: TeamPrivacy::Secret,
        url: "https://example.com/team".to_owned(),
        html_url: "https://example.com/team".to_owned(),
        members_url: "https://example.com/team/members".to_owned(),
        repositories_url: "https://example.com/team/repos".to_owned(),
        permission: "pull".to_owned(),
        notification_setting: Some(TeamNotifications::NotificationsEnabled),
    };
    let parent_wire = serde_json::to_string(&parent).unwrap();
    let mut team: ReviewTeam = serde_json::from_str(&parent_wire).unwrap();
    team.parent = Some(Nullable::Value(Box::new(parent)));
    team.deleted = Some(false);
    team.notification_setting = Some(TeamNotifications::NotificationsDisabled);
    let input = serde_json::to_string(&team).unwrap();
    let requested: Team = amiss_wire::read_json(input.as_bytes(), u64::MAX).unwrap();
    assert_eq!(
        amiss_fixtures::canonical_json(&serde_json::to_vec(&requested).unwrap()).unwrap(),
        amiss_fixtures::canonical_json(input.as_bytes()).unwrap(),
    );
    assert_eq!(
        amiss_wire::read_json::<Reviewer>(input.as_bytes(), u64::MAX).unwrap(),
        Reviewer::Team(Box::new(team)),
    );
    assert!(serde_json::from_str::<ParentTeam>(&input).is_err());
    for (old, new) in [
        ("{", r#"{"unknown":true,"#),
        (r#""parent":{"#, r#""parent":{"unknown":true,"#),
        (r#""privacy":"secret""#, r#""privacy":"unknown""#),
        (r#""privacy":"secret","#, ""),
        (r#""deleted":false"#, r#""deleted":null"#),
        (
            r#""notification_setting":"notifications_disabled""#,
            r#""notification_setting":null"#,
        ),
        (
            r#""notification_setting":"notifications_enabled""#,
            r#""notification_setting":"unknown""#,
        ),
        (r#""description":null,"#, ""),
        (r#""id":1"#, r#""id":9007199254740992"#),
    ] {
        assert!(input.contains(old), "{old}");
        let invalid = input.replacen(old, new, 1);
        assert!(
            serde_json::from_str::<ReviewTeam>(&invalid).is_err(),
            "{new}"
        );
        assert!(serde_json::from_str::<Reviewer>(&invalid).is_err(), "{new}");
    }
    for (value, spelling) in [
        (TeamPrivacy::Open, "open"),
        (TeamPrivacy::Closed, "closed"),
        (TeamPrivacy::Secret, "secret"),
    ] {
        assert_eq!(
            serde_json::to_string(&value).unwrap(),
            format!("\"{spelling}\"")
        );
    }
    assert!(parent_wire.contains(r#""notification_setting":"notifications_enabled""#));
    assert!(input.contains(r#""notification_setting":"notifications_disabled""#));
    let nullable_parent = input.replacen(&parent_wire, "null", 1);
    assert_ne!(input, nullable_parent);
    assert!(serde_json::from_str::<ReviewTeam>(&nullable_parent).is_ok());
}

#[test]
fn shared_auto_merge_requires_nullable_webhook_fields_without_widening_rest() {
    let record: AutoMergeRecord<Option<WorkflowOwner>, Option<String>> = AutoMergeRecord {
        enabled_by: None,
        merge_method: MergeMethod::Squash,
        commit_title: None,
        commit_message: None,
    };
    let input = serde_json::to_string(&record).unwrap();
    assert_eq!(
        amiss_wire::read_json::<AutoMergeRecord<Option<WorkflowOwner>, Option<String>>>(
            input.as_bytes(),
            u64::MAX
        )
        .unwrap(),
        record,
    );
    assert!(serde_json::from_str::<AutoMergeRecord>(&input).is_err());
    for member in [
        r#""enabled_by":null,"#,
        r#""commit_title":null,"#,
        r#","commit_message":null"#,
    ] {
        assert_eq!(input.matches(member).count(), 1);
        let missing = input.replacen(member, "", 1);
        assert!(
            serde_json::from_str::<AutoMergeRecord<Option<WorkflowOwner>, Option<String>>>(
                &missing
            )
            .is_err(),
            "{member}"
        );
    }
    let populated = AutoMergeRecord {
        enabled_by: Some(
            serde_json::from_str::<WorkflowOwner>(r#"{"login":"author","id":1,"type":"User"}"#)
                .unwrap(),
        ),
        commit_title: Some("Docs".to_owned()),
        commit_message: Some("Update examples".to_owned()),
        ..record
    };
    let input = serde_json::to_vec(&populated).unwrap();
    assert_eq!(
        amiss_wire::read_json::<AutoMergeRecord<Option<WorkflowOwner>, Option<String>>>(
            &input,
            u64::MAX
        )
        .unwrap(),
        populated,
    );
}

#[test]
fn shared_milestones_retain_the_webhook_creator_contract() {
    let creator: WorkflowOwner<PullRequestAccountKind> =
        serde_json::from_str(r#"{"login":"creator","id":1,"type":"Mannequin"}"#).unwrap();
    let milestone = MilestoneRecord {
        url: "https://example.com/milestone".to_owned(),
        html_url: "https://example.com/milestone".to_owned(),
        labels_url: "https://example.com/milestone/labels".to_owned(),
        id: js_int::uint!(1),
        node_id: "milestone-one".to_owned(),
        number: js_int::uint!(1),
        state: State::Open,
        title: "Docs".to_owned(),
        description: Nullable::Null,
        creator: Nullable::Value(creator),
        open_issues: js_int::uint!(1),
        closed_issues: js_int::uint!(0),
        created_at: "2026-09-10T00:00:00Z".to_owned(),
        updated_at: "2026-09-10T00:00:00Z".to_owned(),
        closed_at: Nullable::Null,
        due_on: Nullable::Null,
    };
    let input = serde_json::to_vec(&milestone).unwrap();
    assert_eq!(
        amiss_wire::read_json::<MilestoneRecord<WorkflowOwner<PullRequestAccountKind>>>(
            &input,
            u64::MAX
        )
        .unwrap(),
        milestone,
    );
    let projected: MilestoneRecord = serde_json::from_slice(&input).unwrap();
    assert_eq!(
        projected.creator,
        Nullable::Value(amiss_controller_github::owner::OwnerRecord {
            login: "creator".to_owned(),
        })
    );
    let nullable = MilestoneRecord {
        creator: Nullable::<WorkflowOwner<PullRequestAccountKind>>::Null,
        ..milestone
    };
    let input = serde_json::to_string(&nullable).unwrap();
    assert_eq!(
        amiss_wire::read_json::<MilestoneRecord<WorkflowOwner<PullRequestAccountKind>>>(
            input.as_bytes(),
            u64::MAX
        )
        .unwrap(),
        nullable,
    );
    assert_eq!(input.matches(r#""creator":null,"#).count(), 1);
    let missing = input.replacen(r#""creator":null,"#, "", 1);
    assert!(
        serde_json::from_str::<MilestoneRecord<WorkflowOwner<PullRequestAccountKind>>>(&missing)
            .is_err()
    );
}
