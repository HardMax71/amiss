use amiss_controller::{ProviderError, decode_bounded_json};
use amiss_controller_gitea::review::{
    CreateReview, CreateReviewComment, ReviewRecord, ReviewState,
};

#[test]
fn review_captures_retain_owned_feedback_and_ignore_metadata()
-> Result<(), Box<dyn std::error::Error>> {
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
                |bytes| serde_json::from_slice(bytes),
            ),
            Err(ProviderError::InvalidResponse)
        );
        let approved = reviews.last().ok_or("missing approval")?;
        assert_eq!(approved.state, ReviewState::Approved);
        assert!(approved.commit_id.is_some());
        assert!(approved.user.is_some());
        for request in reviews
            .iter()
            .filter(|review| review.state == ReviewState::RequestReview)
        {
            assert!(request.user.is_none());
            assert!(request.commit_id.is_none());
        }
        let minimal = serde_json::to_string(&reviews)?;
        let metadata = minimal.replacen(
            '{',
            r#"{"team":false,"official":[],"comments_count":{},"submitted_at":42,"updated_at":true,"html_url":null,"pull_request_url":[],"extra":{},"#,
            1,
        );
        assert_eq!(
            serde_json::from_str::<Vec<ReviewRecord>>(&metadata)?,
            reviews
        );
        amiss_fixtures::assert_json_rejections::<Vec<ReviewRecord>>(
            input,
            &[
                (r#""state":"#, r#""sta\u0074e":"APPROVED","state":"#),
                (r#""login":"#, r#""login":false,"login":"#),
                (r#""state":"APPROVED""#, r#""state":"UNKNOWN""#),
                (r#""state":"APPROVED""#, r#""state":null"#),
                (r#""state":"APPROVED""#, r#""state":{"APPROVED":null}"#),
                (r#""stale":false"#, r#""stale":"false""#),
            ],
        );
    }
    Ok(())
}

#[test]
fn review_records_require_identity_freshness_and_lossless_commits()
-> Result<(), Box<dyn std::error::Error>> {
    let approved =
        serde_json::from_str::<Vec<ReviewRecord>>(include_str!("../fixtures/gitea-reviews.json"))?
            .pop()
            .ok_or("missing approval")?;
    let encoded = serde_json::to_string(&approved)?;
    for (field, value) in [
        ("id", approved.id.to_string()),
        ("user", serde_json::to_string(&approved.user)?),
        ("state", serde_json::to_string(&approved.state)?),
        ("body", serde_json::to_string(&approved.body)?),
        ("commit_id", serde_json::to_string(&approved.commit_id)?),
        ("stale", approved.stale.to_string()),
        ("dismissed", approved.dismissed.to_string()),
    ] {
        for replacement in [
            format!(r#""missing_{field}":{value}"#),
            format!(r#""{field}":{value},"{field}":{value}"#),
        ] {
            let invalid = encoded.replacen(&format!(r#""{field}":{value}"#), &replacement, 1);
            assert_ne!(invalid, encoded);
            assert!(
                serde_json::from_str::<ReviewRecord>(&invalid).is_err(),
                "{field}"
            );
        }
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
    for empty in [r#""""#, "null"] {
        let changed = encoded.replace(&serde_json::to_string(sha)?, empty);
        assert_eq!(
            serde_json::from_str::<ReviewRecord>(&changed)?.commit_id,
            None
        );
    }
    let positional = serde_json::to_vec(&(
        approved.id,
        &approved.user,
        approved.state,
        &approved.body,
        sha,
        approved.stale,
        approved.dismissed,
    ))?;
    assert_eq!(
        serde_json::from_slice::<ReviewRecord>(&positional)?,
        approved
    );
    let oversized = ReviewRecord {
        id: js_int::MAX_SAFE_UINT + 1,
        ..approved
    };
    assert!(serde_json::to_vec(&oversized).is_err());
    Ok(())
}

#[test]
fn review_requests_use_the_shared_state() -> Result<(), Box<dyn std::error::Error>> {
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
        assert_eq!(
            serde_json::from_str::<ReviewState>(&serde_json::to_string(spelling)?)?,
            event
        );
    }
    Ok(())
}

#[test]
fn review_comments_preserve_optional_context_and_reject_invalid_positions()
-> Result<(), Box<dyn std::error::Error>> {
    let request = CreateReview {
        event: ReviewState::Comment,
        body: "Review feedback".to_owned(),
        commit_id: "a".repeat(40).parse()?,
        comments: [None, Some(2.into())]
            .into_iter()
            .map(|extra_lines_count| CreateReviewComment {
                path: "src/lib.rs".to_owned(),
                body: "Line feedback".to_owned(),
                old_position: 0.into(),
                new_position: 1.into(),
                extra_lines_count,
            })
            .collect(),
    };
    let encoded = serde_json::to_string(&request)?;
    assert_eq!(
        amiss_wire::read_json::<CreateReview>(encoded.as_bytes(), u64::MAX)?,
        request
    );
    assert_eq!(encoded.matches("extra_lines_count").count(), 1);
    amiss_fixtures::assert_json_rejections::<CreateReview>(
        &encoded,
        &[
            (r#""event":"#, r#""extra":true,"event":"#),
            (r#""path":"#, r#""extra":true,"path":"#),
            (r#""path":"#, r#""extra_lines_count":null,"path":"#),
            (r#""extra_lines_count":2"#, r#""extra_lines_count":null"#),
            (r#""old_position":0,"#, ""),
            (r#""old_position":0"#, r#""old_position":9007199254740992"#),
            (r#""new_position":1"#, r#""new_position":-9007199254740992"#),
            (r#""new_position":1"#, r#""new_position":true"#),
            (r#""new_position":1"#, r#""new_position":1.5"#),
        ],
    );
    Ok(())
}
