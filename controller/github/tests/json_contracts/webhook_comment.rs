use amiss_controller_github::webhook::comment::{Comment, ReviewCommentRecord};
use amiss_controller_github::webhook::pull::review::CommentPullRequest;
use amiss_controller_github::webhook::review::ReviewEvent;
use amiss_wire::assessment::Nullable;
use js_int::UInt;

#[test]
fn issue_comment_metadata_retains_its_distinct_closed_shape() {
    let input = amiss_fixtures::GITHUB_WEBHOOK_ISSUE_COMMENT;
    let record: Comment = amiss_wire::read_json(input, u64::MAX).unwrap();
    assert!(matches!(record, Comment::Issue(_)));
    let wire = serde_json::to_string(&record).unwrap();
    assert_eq!(
        amiss_fixtures::canonical_json(wire.as_bytes()).unwrap(),
        amiss_fixtures::canonical_json(input).unwrap()
    );
    for (old, new, valid) in [
        ("{", r#"{"minimized":null,"pin":null,"#, true),
        (
            "{",
            r#"{"minimized":{"reason":null},"pin":{"pinned_at":"2026-09-10T00:00:00Z","pinned_by":null},"#,
            true,
        ),
        ("{", r#"{"minimized":{},"#, false),
        ("{", r#"{"pin":{},"#, false),
        ("{", r#"{"unknown":true,"#, false),
        (r#""performed_via_github_app":null,"#, "", false),
        (
            r#""performed_via_github_app":null"#,
            r#""performed_via_github_app":{}"#,
            false,
        ),
        (r#""type":"User""#, r#""type":"Mannequin""#, true),
    ] {
        assert!(wire.contains(old), "{old}");
        let candidate = wire.replacen(old, new, 1);
        assert_eq!(
            serde_json::from_str::<Comment>(&candidate).is_ok(),
            valid,
            "{new}"
        );
        assert_eq!(
            amiss_wire::read_json::<Comment>(candidate.as_bytes(), u64::MAX).is_ok(),
            valid,
            "{new}"
        );
    }
}

#[test]
fn review_comment_members_keep_presence_nullability_and_closed_shapes() {
    let input = amiss_fixtures::GITHUB_WEBHOOK_REVIEW_COMMENT;
    let comment: Comment = amiss_wire::read_json(input, u64::MAX).unwrap();
    assert!(matches!(comment, Comment::Review(_)));
    let encoded = serde_json::to_vec(&comment).unwrap();
    assert_eq!(
        amiss_fixtures::canonical_json(input).unwrap(),
        amiss_fixtures::canonical_json(&encoded).unwrap()
    );
    let wire = String::from_utf8(encoded).unwrap();
    for (old, new, valid) in [
        ("{", r#"{"unknown":true,"#, false),
        ("{", r#"{"in_reply_to_id":1,"subject_type":"file","#, true),
        ("{", r#"{"in_reply_to_id":null,"#, false),
        ("{", r#"{"subject_type":null,"#, false),
        ("{", r#"{"subject_type":"unknown","#, false),
        (r#""position":1"#, r#""position":null"#, true),
        (r#""position":1,"#, "", false),
        (
            r#""original_position":1"#,
            r#""original_position":null"#,
            false,
        ),
        (r#""side":"RIGHT""#, r#""side":"LEFT""#, true),
        (r#""side":"RIGHT""#, r#""side":{"RIGHT":null}"#, false),
        (r#""side":"RIGHT""#, r#""side":"CENTER""#, false),
        (r#""start_side":null,"#, "", false),
        (r#""start_side":null"#, r#""start_side":"LEFT""#, true),
        (r#""reactions":{"#, r#""reactions":{"unknown":0,"#, false),
        (r#""+1":0,"#, "", false),
        (r#""+1":0"#, r#""+1":9007199254740992"#, false),
        (r#""_links":{"#, r#""_links":{"unknown":{},"#, false),
        (r#""user":{"#, r#""user":{"unknown":true,"#, false),
        (r#""type":"User""#, r#""type":"Mannequin""#, false),
        (r#""original_line":265"#, r#""original_line":null"#, true),
        (r#""original_line":265,"#, "", false),
    ] {
        assert!(wire.contains(old), "{old}");
        let candidate = wire.replacen(old, new, 1);
        assert_eq!(
            serde_json::from_str::<ReviewCommentRecord<Nullable<UInt>>>(&candidate).is_ok(),
            valid,
            "{new}"
        );
        assert_eq!(
            amiss_wire::read_json::<Comment>(candidate.as_bytes(), u64::MAX).is_ok(),
            valid,
            "{new}"
        );
        assert_eq!(
            serde_json::from_str::<ReviewCommentRecord>(&candidate).is_ok(),
            valid && new != r#""original_line":null"#,
            "non-null original line: {new}"
        );
    }
}

#[test]
fn review_comment_pr_options_remain_absent_without_defaults() {
    let ReviewEvent::Submitted { event } =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_REVIEW).unwrap()
    else {
        panic!("submitted review")
    };
    let pull = CommentPullRequest {
        context: event.pull_request.request,
        draft: None,
        auto_merge: None,
        stack: None,
    };
    let input = serde_json::to_string(&pull).unwrap();
    assert!(
        amiss_wire::read_json::<CommentPullRequest>(input.as_bytes(), u64::MAX).unwrap() == pull
    );
    for field in ["draft", "auto_merge", "stack"] {
        assert!(!input.contains(&format!("\"{field}\":")));
    }
    for (extra, valid) in [
        (r#""draft":false,"auto_merge":null"#, true),
        (r#""draft":null"#, false),
        (r#""stack":null"#, false),
        (r#""auto_merge":{}"#, false),
        (r#""unknown":false"#, false),
    ] {
        let candidate = input.replacen('{', &format!("{{{extra},"), 1);
        assert_eq!(
            serde_json::from_str::<CommentPullRequest>(&candidate).is_ok(),
            valid,
            "{extra}"
        );
        assert_eq!(
            amiss_wire::read_json::<CommentPullRequest>(candidate.as_bytes(), u64::MAX).is_ok(),
            valid,
            "{extra}"
        );
    }
}
