use amiss_controller::{ProviderError, decode_bounded_json};
use amiss_controller_gitea::review::{
    CreateReview, CreateReviewComment, ReviewRecord, ReviewState,
};

#[test]
fn review_captures_retain_user_identity_and_team_metadata() -> Result<(), Box<dyn std::error::Error>>
{
    for (input, count) in [
        (include_str!("../fixtures/gitea-reviews.json"), 1),
        (include_str!("../fixtures/forgejo-reviews.json"), 2),
    ] {
        let (reviews, length): (Vec<ReviewRecord>, _) =
            decode_bounded_json(input.as_bytes(), None, input.len(), |bytes| {
                serde_json::from_slice(bytes)
            })?;
        assert_eq!(length, input.len());
        assert_eq!(reviews.len(), count);
        assert_eq!(
            decode_bounded_json::<Vec<ReviewRecord>, _>(
                input.as_bytes(),
                None,
                input.len() - 1,
                |bytes| amiss_wire::read_json(bytes, u64::MAX),
            ),
            Err(ProviderError::InvalidResponse)
        );
        let approved = reviews.last().ok_or("missing approval")?;
        assert_eq!(approved.state, ReviewState::Approved);
        assert!(approved.commit_id.is_some());
        assert!(approved.user.is_some());
        assert!(approved.team.is_none());
        assert!(approved.official);
        assert_eq!(approved.comments_count, 0);
        assert_eq!(approved.submitted_at, approved.updated_at);
        assert!(approved.html_url.starts_with(&approved.pull_request_url));
        for request in reviews
            .iter()
            .filter(|review| review.state == ReviewState::RequestReview)
        {
            assert!(request.user.is_none());
            assert!(request.commit_id.is_none());
            assert!(request.html_url.is_empty());
            assert_eq!(request.team.as_ref().ok_or("missing team")?.id, 80022);
        }
        for (old, new) in [
            (r#""state":"#, r#""extra":true,"state":"#),
            (r#""state":"#, r#""sta\u0074e":"APPROVED","state":"#),
            (r#""login":"#, r#""login":false,"login":"#),
            (r#""state":"APPROVED""#, r#""state":"UNKNOWN""#),
            (r#""state":"APPROVED""#, r#""state":"""#),
            (r#""state":"APPROVED""#, r#""state":{"APPROVED":null}"#),
            (r#""state":"APPROVED""#, r#""state":null"#),
            (r#""comments_count":0"#, r#""comments_count":-1"#),
            (
                r#""comments_count":0"#,
                r#""comments_count":9007199254740992"#,
            ),
        ] {
            let changed = input.replace(old, new);
            assert_ne!(changed, input);
            assert!(serde_json::from_str::<Vec<ReviewRecord>>(&changed).is_err());
            assert!(
                amiss_wire::read_json::<Vec<ReviewRecord>>(changed.as_bytes(), u64::MAX).is_err()
            );
        }
    }
    let input = include_str!("../fixtures/forgejo-reviews.json");
    let changed = input.replace(r#""organization":"#, r#""extra":true,"organization":"#);
    assert_ne!(changed, input);
    assert!(serde_json::from_str::<Vec<ReviewRecord>>(&changed).is_err());
    Ok(())
}

#[test]
fn review_records_require_metadata_and_lossless_commit_identity()
-> Result<(), Box<dyn std::error::Error>> {
    let approved =
        serde_json::from_str::<Vec<ReviewRecord>>(include_str!("../fixtures/gitea-reviews.json"))?
            .pop()
            .ok_or("missing approval")?;
    let encoded = serde_json::to_string(&approved)?;
    for field in [
        format!(",\"user\":{}", serde_json::to_string(&approved.user)?),
        ",\"team\":null".to_owned(),
        ",\"official\":true".to_owned(),
        ",\"comments_count\":0".to_owned(),
        format!(
            ",\"commit_id\":{}",
            serde_json::to_string(&approved.commit_id)?
        ),
        format!(
            ",\"submitted_at\":{}",
            serde_json::to_string(&approved.submitted_at)?
        ),
        format!(
            ",\"updated_at\":{}",
            serde_json::to_string(&approved.updated_at)?
        ),
        format!(
            ",\"html_url\":{}",
            serde_json::to_string(&approved.html_url)?
        ),
        format!(
            ",\"pull_request_url\":{}",
            serde_json::to_string(&approved.pull_request_url)?
        ),
    ] {
        let changed = encoded.replace(&field, "");
        assert_ne!(changed, encoded);
        assert!(serde_json::from_str::<ReviewRecord>(&changed).is_err());
    }
    let sha = approved
        .commit_id
        .as_ref()
        .ok_or("missing commit")?
        .as_str();
    for invalid in [
        "g".repeat(40),
        "A".repeat(40),
        "a".repeat(39),
        "a".repeat(41),
    ] {
        let changed = encoded.replace(sha, &invalid);
        assert_ne!(changed, encoded);
        assert!(serde_json::from_str::<ReviewRecord>(&changed).is_err());
    }
    let null_commit = encoded.replace(&serde_json::to_string(sha)?, "null");
    assert!(
        serde_json::from_str::<ReviewRecord>(&null_commit)?
            .commit_id
            .is_none()
    );
    assert!(amiss_wire::read_json::<ReviewRecord>(null_commit.as_bytes(), u64::MAX).is_err());
    let positional = serde_json::to_vec(&(
        approved.id,
        &approved.user,
        &approved.team,
        approved.state,
        &approved.body,
        sha,
        approved.stale,
        approved.official,
        approved.dismissed,
        approved.comments_count,
        &approved.submitted_at,
        &approved.updated_at,
        &approved.html_url,
        &approved.pull_request_url,
    ))?;
    assert_eq!(
        serde_json::from_slice::<ReviewRecord>(&positional)?,
        approved
    );
    assert!(amiss_wire::read_json::<ReviewRecord>(&positional, u64::MAX).is_err());
    let oversized = ReviewRecord {
        id: js_int::MAX_SAFE_UINT + 1,
        ..approved
    };
    assert!(serde_json::to_vec(&oversized).is_err());
    Ok(())
}

#[test]
fn review_requests_use_the_shared_state_and_real_comment_fields()
-> Result<(), Box<dyn std::error::Error>> {
    for (event, spelling) in [
        (ReviewState::Approved, "APPROVED"),
        (ReviewState::Pending, "PENDING"),
        (ReviewState::Comment, "COMMENT"),
        (ReviewState::RequestChanges, "REQUEST_CHANGES"),
        (ReviewState::RequestReview, "REQUEST_REVIEW"),
    ] {
        assert_eq!(
            serde_json::to_string(&event)?,
            serde_json::to_string(spelling)?
        );
        for extra_lines_count in [None, Some(2.into())] {
            let request = CreateReview {
                event,
                body: "Review feedback".to_owned(),
                commit_id: "a".repeat(40).parse()?,
                comments: vec![CreateReviewComment {
                    path: "src/lib.rs".to_owned(),
                    body: "Line feedback".to_owned(),
                    old_position: 0.into(),
                    new_position: 1.into(),
                    extra_lines_count,
                }],
            };
            let encoded = serde_json::to_string(&request)?;
            assert_eq!(
                amiss_wire::read_json::<CreateReview>(encoded.as_bytes(), u64::MAX)?,
                request
            );
            assert_eq!(
                encoded.contains("extra_lines_count"),
                extra_lines_count.is_some()
            );
            for (old, new) in [
                (r#""event":"#, r#""extra":true,"event":"#),
                (r#""path":"#, r#""extra":true,"path":"#),
                (r#""path":"#, r#""extra_lines_count":null,"path":"#),
                (r#""old_position":0,"#, ""),
                (r#""old_position":0"#, r#""old_position":9007199254740992"#),
                (r#""new_position":1"#, r#""new_position":-9007199254740992"#),
                (r#""new_position":1"#, r#""new_position":true"#),
                (r#""new_position":1"#, r#""new_position":1.5"#),
            ] {
                let changed = encoded.replace(old, new);
                assert_ne!(changed, encoded);
                assert!(serde_json::from_str::<CreateReview>(&changed).is_err());
            }
        }
    }
    Ok(())
}
