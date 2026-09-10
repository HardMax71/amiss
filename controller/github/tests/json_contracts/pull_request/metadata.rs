use amiss_controller_github::pull::metadata::{
    AuthorAssociation, AutoMergeRecord, MergeMethod, MilestoneRecord, StackBase, StackRecord,
    TeamKind, TeamRecord,
};
use amiss_controller_github::pull::{PullRequestRecord, State};
use amiss_wire::assessment::Nullable;

use super::{CLOSED, OPEN};

#[test]
fn pull_requests_retain_all_declared_optional_metadata() {
    let mut complete: PullRequestRecord = amiss_wire::read_json(OPEN.as_bytes(), u64::MAX).unwrap();
    let closed: PullRequestRecord = amiss_wire::read_json(CLOSED.as_bytes(), u64::MAX).unwrap();
    complete.labels = closed.labels;
    complete.milestone = Nullable::Value(Box::new(MilestoneRecord {
        url: "https://example.com/milestone".to_owned(),
        html_url: "https://example.com/milestone".to_owned(),
        labels_url: "https://example.com/milestone/labels".to_owned(),
        id: js_int::uint!(1),
        node_id: "milestone-node".to_owned(),
        number: js_int::uint!(1),
        state: State::Open,
        title: "Synthetic milestone".to_owned(),
        description: Nullable::Null,
        creator: Nullable::Value(complete.user.clone()),
        open_issues: js_int::uint!(1),
        closed_issues: js_int::uint!(0),
        created_at: "2026-09-10T00:00:00Z".to_owned(),
        updated_at: "2026-09-10T00:00:00Z".to_owned(),
        closed_at: Nullable::Null,
        due_on: Nullable::Null,
    }));
    complete.auto_merge = Nullable::Value(Box::new(AutoMergeRecord {
        enabled_by: complete.user.clone(),
        merge_method: MergeMethod::Squash,
        commit_title: "Synthetic merge".to_owned(),
        commit_message: String::new(),
    }));
    complete.requested_teams = Some(vec![TeamRecord {
        id: js_int::uint!(2),
        node_id: "team-node".to_owned(),
        url: "https://example.com/team".to_owned(),
        members_url: "https://example.com/team/members{/member}".to_owned(),
        name: "Synthetic team".to_owned(),
        description: Nullable::Null,
        permission: "pull".to_owned(),
        html_url: "https://example.com/team".to_owned(),
        repositories_url: "https://example.com/team/repos".to_owned(),
        slug: "synthetic-team".to_owned(),
        kind: TeamKind::Organization,
        privacy: Some("closed".to_owned()),
        notification_setting: Some("notifications_enabled".to_owned()),
        ldap_dn: Some("uid=example,ou=users,dc=github,dc=com".to_owned()),
        organization_id: Some(js_int::uint!(3)),
        enterprise_id: Some(js_int::uint!(4)),
    }]);
    complete.assignee = Nullable::Value(Box::new(complete.user.clone()));
    complete.merged_by = Nullable::Value(Box::new(complete.user.clone()));
    complete.assignees = Some(vec![complete.user.clone()]);
    complete.requested_reviewers = Some(vec![complete.user.clone()]);
    complete.author_association = AuthorAssociation::FirstTimeContributor;
    complete.active_lock_reason = Some(Nullable::Value("resolved".to_owned()));
    complete.rebaseable = Some(Nullable::Value(true));
    complete.draft = Some(false);
    let encoded = serde_json::to_string(&complete).unwrap();
    assert!(serde_json::from_str::<PullRequestRecord>(&encoded).unwrap() == complete);
    assert!(
        amiss_wire::read_json::<PullRequestRecord>(encoded.as_bytes(), u64::MAX).unwrap()
            == complete
    );

    for (old, new) in [
        ("\"milestone\":{", "\"milestone\":{\"unexpected\":true,"),
        ("\"auto_merge\":{", "\"auto_merge\":{\"unexpected\":true,"),
        (
            "\"requested_teams\":[{",
            "\"requested_teams\":[{\"unexpected\":true,",
        ),
        ("\"node_id\":\"milestone-node\",", ""),
        ("\"due_on\":null", "\"due_on\":0"),
        ("\"commit_message\":\"\"", "\"commit_message\":null"),
        (",\"commit_message\":\"\"", ""),
        ("\"merge_method\":\"squash\"", "\"merge_method\":\"other\""),
        ("\"type\":\"organization\",", ""),
        ("\"type\":\"organization\"", "\"type\":\"other\""),
        ("\"organization_id\":3", "\"organization_id\":null"),
    ] {
        assert_eq!(encoded.matches(old).count(), 1, "{old}");
        let changed = encoded.replacen(old, new, 1);
        assert!(
            serde_json::from_str::<PullRequestRecord>(&changed).is_err(),
            "{new}"
        );
        assert!(
            amiss_wire::read_json::<PullRequestRecord>(changed.as_bytes(), u64::MAX).is_err(),
            "{new}"
        );
    }
}

#[test]
fn pull_request_stacks_keep_their_base_and_optional_counts() {
    let mut record: PullRequestRecord = amiss_wire::read_json(OPEN.as_bytes(), u64::MAX).unwrap();
    let stack = StackRecord {
        base: StackBase {
            branch: "main".to_owned(),
            sha: record.base.sha.clone(),
        },
        size: Some(js_int::uint!(2)),
        position: Some(js_int::uint!(1)),
        id: Some(js_int::uint!(5)),
        number: Some(js_int::uint!(1)),
    };
    record.stack = Some(Nullable::Value(stack.clone()));
    let encoded = serde_json::to_vec(&record).unwrap();
    assert!(serde_json::from_slice::<PullRequestRecord>(&encoded).unwrap() == record);
    assert!(amiss_wire::read_json::<PullRequestRecord>(&encoded, u64::MAX).unwrap() == record);
    let encoded = serde_json::to_string(&stack).unwrap();
    for (old, new) in [
        ("{", "{\"unexpected\":true,"),
        ("\"base\":{", "\"base\":{\"unexpected\":true,"),
        ("\"position\":1", "\"position\":null"),
        ("\"size\":2", "\"size\":9007199254740992"),
        ("\"size\":2", "\"size\":-1"),
        ("\"ref\":\"main\",", ""),
    ] {
        assert!(encoded.contains(old), "{old}");
        let changed = encoded.replacen(old, new, 1);
        assert!(
            serde_json::from_str::<StackRecord>(&changed).is_err(),
            "{new}"
        );
        assert!(
            amiss_wire::read_json::<StackRecord>(changed.as_bytes(), u64::MAX).is_err(),
            "{new}"
        );
    }
    let base = format!("\"base\":{},", serde_json::to_string(&stack.base).unwrap());
    assert_eq!(encoded.matches(&base).count(), 1);
    let missing = encoded.replacen(&base, "", 1);
    assert!(serde_json::from_str::<StackRecord>(&missing).is_err());
    let minimal = StackRecord {
        size: None,
        position: None,
        id: None,
        number: None,
        ..stack
    };
    let encoded = serde_json::to_vec(&minimal).unwrap();
    assert_eq!(
        serde_json::from_slice::<StackRecord>(&encoded).unwrap(),
        minimal
    );
    assert_eq!(
        amiss_wire::read_json::<StackRecord>(&encoded, u64::MAX).unwrap(),
        minimal
    );
}
