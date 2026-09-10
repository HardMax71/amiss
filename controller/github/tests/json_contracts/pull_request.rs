use amiss_controller::decode_bounded_json;
use amiss_controller_github::pull::{PullRequestRecord, State};
use amiss_wire::assessment::Nullable;

const OPEN: &str = include_str!("../fixtures/pull-request.json");
const CLOSED: &str = include_str!("../fixtures/pull-request-closed.json");

#[path = "pull_request/metadata.rs"]
mod metadata;

#[test]
fn pull_request_captures_retain_all_metadata_before_strict_reading() {
    for (input, state, has_head_repository) in
        [(OPEN, State::Open, true), (CLOSED, State::Closed, false)]
    {
        let (record, consumed): (PullRequestRecord, _) = decode_bounded_json(
            input.as_bytes(),
            Some(u64::try_from(input.len()).unwrap()),
            input.len(),
            |bytes| serde_json::from_slice(bytes),
        )
        .unwrap();
        assert_eq!(consumed, input.len());
        assert_eq!(record.state, state);
        assert_eq!(record.head.repo.is_some(), has_head_repository);
        assert_eq!(
            amiss_fixtures::canonical_json(&serde_json::to_vec(&record).unwrap()).unwrap(),
            amiss_fixtures::canonical_json(input.as_bytes()).unwrap()
        );
        assert!(
            amiss_wire::read_json::<PullRequestRecord>(input.as_bytes(), u64::MAX).unwrap()
                == record
        );
        assert!(amiss_wire::read_json::<PullRequestRecord>(input.as_bytes(), 0).is_err());
        let trailing = format!("{input} {{}}");
        assert!(amiss_wire::read_json::<PullRequestRecord>(trailing.as_bytes(), u64::MAX).is_err());
    }
}

#[test]
fn pull_request_presence_keeps_required_nulls_and_optional_nulls_distinct() {
    let absent = PullRequestRecord {
        mergeable: None,
        merge_commit_sha: None,
        body: Nullable::Null,
        milestone: Nullable::Null,
        closed_at: Nullable::Null,
        merged_at: Nullable::Null,
        assignee: Nullable::Null,
        auto_merge: Nullable::Null,
        merged_by: Nullable::Null,
        active_lock_reason: None,
        assignees: None,
        requested_reviewers: None,
        requested_teams: None,
        stack: None,
        draft: None,
        rebaseable: None,
        ..serde_json::from_str(OPEN).unwrap()
    };
    let nulls = PullRequestRecord {
        active_lock_reason: Some(Nullable::Null),
        rebaseable: Some(Nullable::Null),
        stack: Some(Nullable::Null),
        ..absent.clone()
    };
    for record in [&absent, &nulls] {
        let encoded = serde_json::to_vec(record).unwrap();
        assert!(serde_json::from_slice::<PullRequestRecord>(&encoded).unwrap() == *record);
        assert!(amiss_wire::read_json::<PullRequestRecord>(&encoded, u64::MAX).unwrap() == *record);
    }
    let encoded = serde_json::to_string(&absent).unwrap();
    for field in [
        "mergeable",
        "merge_commit_sha",
        "body",
        "milestone",
        "closed_at",
        "merged_at",
        "assignee",
        "auto_merge",
        "merged_by",
    ] {
        let member = format!("\"{field}\":null,");
        assert_eq!(encoded.matches(&member).count(), 1, "{field}");
        let missing = encoded.replacen(&member, "", 1);
        assert!(
            serde_json::from_str::<PullRequestRecord>(&missing).is_err(),
            "{field}"
        );
        assert!(
            amiss_wire::read_json::<PullRequestRecord>(missing.as_bytes(), u64::MAX).is_err(),
            "{field}"
        );
    }
    for field in [
        "assignees",
        "requested_reviewers",
        "requested_teams",
        "draft",
    ] {
        let changed = encoded.replacen('{', &format!("{{\"{field}\":null,"), 1);
        assert!(
            serde_json::from_str::<PullRequestRecord>(&changed).is_err(),
            "{field}"
        );
        assert!(
            amiss_wire::read_json::<PullRequestRecord>(changed.as_bytes(), u64::MAX).is_err(),
            "{field}"
        );
    }
}

#[test]
fn pull_request_identity_and_counters_stay_required_and_exact() {
    let record: PullRequestRecord = amiss_wire::read_json(OPEN.as_bytes(), u64::MAX).unwrap();
    let encoded = serde_json::to_string(&record).unwrap();
    for (field, value) in [
        ("id", record.id.to_string()),
        ("number", record.number.to_string()),
        ("additions", record.additions.to_string()),
        ("deletions", record.deletions.to_string()),
        ("changed_files", record.changed_files.to_string()),
        ("comments", record.comments.to_string()),
        ("review_comments", record.review_comments.to_string()),
        ("commits", record.commits.to_string()),
    ] {
        let member = format!("\"{field}\":{value},");
        assert_eq!(encoded.matches(&member).count(), 1, "{field}");
        for replacement in [
            String::new(),
            format!("\"{field}\":null,"),
            format!("\"{field}\":9007199254740992,"),
            format!("\"{field}\":-1,"),
            format!("\"{field}\":1.5,"),
        ] {
            let changed = encoded.replacen(&member, &replacement, 1);
            assert!(
                serde_json::from_str::<PullRequestRecord>(&changed).is_err(),
                "{field}"
            );
            assert!(
                amiss_wire::read_json::<PullRequestRecord>(changed.as_bytes(), u64::MAX).is_err(),
                "{field}"
            );
        }
    }
}

#[test]
fn pull_requests_refuse_unknown_state_and_nested_data() {
    for (old, new) in [
        ("{", "{\"unexpected\":true,"),
        ("\"_links\":{", "\"_links\":{\"unexpected\":true,"),
        (
            "\"comments\":{\"href\":",
            "\"comments\":{\"unexpected\":true,\"href\":",
        ),
        ("\"head\":{", "\"head\":{\"unexpected\":true,"),
        ("\"labels\":[],", ""),
        ("\"locked\":false,", ""),
        ("\"state\":\"open\",", ""),
        ("\"state\":\"open\"", "\"state\":\"unknown\""),
        ("\"state\":\"open\"", "\"state\":\"OPEN\""),
        ("\"state\":\"open\"", "\"state\":null"),
        ("\"state\":\"open\"", "\"state\":{\"open\":null}"),
        ("\"state\":\"open\"", "\"state\":0"),
        (
            "\"state\":\"open\"",
            "\"state\":\"open\",\"\\u0073tate\":\"closed\"",
        ),
        (
            "\"author_association\":\"OWNER\"",
            "\"author_association\":\"owner\"",
        ),
        (
            "\"author_association\":\"OWNER\"",
            "\"author_association\":\"UNKNOWN\"",
        ),
    ] {
        assert!(OPEN.contains(old), "{old}");
        let changed = OPEN.replacen(old, new, 1);
        assert!(
            serde_json::from_str::<PullRequestRecord>(&changed).is_err(),
            "{new}"
        );
        assert!(
            amiss_wire::read_json::<PullRequestRecord>(changed.as_bytes(), u64::MAX).is_err(),
            "{new}"
        );
    }
    let record: PullRequestRecord = amiss_wire::read_json(CLOSED.as_bytes(), u64::MAX).unwrap();
    let label = record.labels.first().unwrap();
    let encoded = serde_json::to_string(label).unwrap();
    let positional = serde_json::to_string(&(
        &label.id,
        &label.node_id,
        &label.url,
        &label.name,
        &label.description,
        &label.color,
        &label.default,
    ))
    .unwrap();
    let named = serde_json::to_string(&record).unwrap();
    assert_eq!(named.matches(&encoded).count(), 1);
    let changed = named.replacen(&encoded, &positional, 1);
    assert!(serde_json::from_str::<PullRequestRecord>(&changed).is_ok());
    assert!(amiss_wire::read_json::<PullRequestRecord>(changed.as_bytes(), u64::MAX).is_err());
    let link = serde_json::to_string(&record.links.comments).unwrap();
    assert_eq!(named.matches(&link).count(), 1);
    let changed = named.replacen(&link, "{}", 1);
    assert!(serde_json::from_str::<PullRequestRecord>(&changed).is_err());
    assert!(amiss_wire::read_json::<PullRequestRecord>(changed.as_bytes(), u64::MAX).is_err());
}
