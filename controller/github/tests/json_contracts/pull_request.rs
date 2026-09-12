use amiss_controller::{ProviderError, decode_bounded_json};
use amiss_controller_github::pull::{PullRequestRecord, State};

const OPEN: &str = include_str!("../fixtures/pull-request.json");
const CLOSED: &str = include_str!("../fixtures/pull-request-closed.json");

#[test]
fn captured_pull_requests_keep_refresh_facts_with_bounded_native_reading() {
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
        assert!(record.id > 0 && record.number > 0);
        assert!(record.base.repo.is_some());
        let encoded = serde_json::to_vec(&record).unwrap();
        assert!(serde_json::from_slice::<PullRequestRecord>(&encoded).unwrap() == record);
        assert!(matches!(
            decode_bounded_json::<PullRequestRecord, _>(
                input.as_bytes(),
                None,
                input.len() - 1,
                |bytes| serde_json::from_slice(bytes),
            ),
            Err(ProviderError::InvalidResponse)
        ));
        let trailing = format!("{input} {{}}");
        assert!(serde_json::from_str::<PullRequestRecord>(&trailing).is_err());
    }
}

#[test]
fn pull_request_merge_facts_are_required_even_when_null() {
    let mut record: PullRequestRecord = serde_json::from_str(OPEN).unwrap();
    record.mergeable = None;
    record.merge_commit_sha = None;
    let input = serde_json::to_string(&record).unwrap();
    amiss_fixtures::assert_json_rejections::<PullRequestRecord>(
        &input,
        &[
            (r#""mergeable":null,"#, ""),
            (r#""merge_commit_sha":null,"#, ""),
            (r#""mergeable":null"#, r#""mergeable":0"#),
            (r#""mergeable":null"#, r#""mergeable":"true""#),
            (r#""merge_commit_sha":null"#, r#""merge_commit_sha":"bad""#),
            (r#""merge_commit_sha":null"#, r#""merge_commit_sha":false"#),
            (
                r#""mergeable":null"#,
                r#""mergeable":null,"mergeable":true"#,
            ),
        ],
    );
    for (mergeable, merge_commit_sha) in [
        (None, None),
        (Some(false), None),
        (Some(true), Some(record.head.sha.clone())),
    ] {
        record.mergeable = mergeable;
        record.merge_commit_sha = merge_commit_sha;
        let input = serde_json::to_vec(&record).unwrap();
        assert!(serde_json::from_slice::<PullRequestRecord>(&input).unwrap() == record);
    }
}

#[test]
fn pull_request_binding_fields_remain_required_typed_and_unique() {
    let record: PullRequestRecord = serde_json::from_str(OPEN).unwrap();
    let input = serde_json::to_string(&record).unwrap();
    for (field, value) in [("id", record.id), ("number", record.number)] {
        let member = format!(r#""{field}":{value},"#);
        amiss_fixtures::assert_json_rejections::<PullRequestRecord>(
            &input,
            &[(&member, ""), (&member, &format!("{member}{member}"))],
        );
        for invalid in ["null", "true", "-1", "1.5", "9007199254740992", r#""1""#] {
            amiss_fixtures::assert_json_rejections::<PullRequestRecord>(
                &input,
                &[(&member, &format!(r#""{field}":{invalid},"#))],
            );
        }
    }
    amiss_fixtures::assert_json_rejections::<PullRequestRecord>(
        &input,
        &[
            (r#""state":"open","#, ""),
            (r#""state":"open""#, r#""state":"unknown""#),
            (r#""state":"open""#, r#""state":"OPEN""#),
            (r#""state":"open""#, r#""state":null"#),
            (r#""state":"open""#, r#""state":{"open":null}"#),
            (
                r#""state":"open""#,
                r#""state":"open","\u0073tate":"closed""#,
            ),
            (r#""id":"#, r#""\u0069d":0,"id":"#),
        ],
    );
    for (field, reference) in [("head", &record.head), ("base", &record.base)] {
        let value = serde_json::to_string(reference).unwrap();
        let member = format!(r#""{field}":{value}"#);
        for invalid in ["null", "{}", "false"] {
            amiss_fixtures::assert_json_rejections::<PullRequestRecord>(
                &input,
                &[(&member, &format!(r#""{field}":{invalid}"#))],
            );
        }
    }
}

#[test]
fn pull_requests_ignore_unconsumed_metadata() {
    let record: PullRequestRecord = serde_json::from_str(OPEN).unwrap();
    let input = serde_json::to_string(&record).unwrap().replacen(
        '{',
        r#"{"future":{"anything":[null,false]},"body":{},"user":null,"labels":false,"_links":null,"draft":[],"auto_merge":42,"requested_teams":{},"stack":false,"#,
        1,
    );
    assert!(serde_json::from_str::<PullRequestRecord>(&input).unwrap() == record);
}
