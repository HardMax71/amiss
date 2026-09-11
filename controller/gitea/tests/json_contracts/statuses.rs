use amiss_controller_gitea::status::{CommitStatusRecord, CommitStatusState, CreateCommitStatus};

#[test]
fn complete_status_pages_retain_metadata_and_nullable_creators()
-> Result<(), Box<dyn std::error::Error>> {
    let input = include_bytes!("../fixtures/commit-status.json");
    let mut status: CommitStatusRecord = serde_json::from_slice(input)?;
    assert_eq!(status.created_at, "2026-08-31T12:00:00Z");
    assert_eq!(status.updated_at, status.created_at);
    assert_eq!(
        status.url,
        "https://forge.example/api/v1/repos/acme/widget/statuses/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    );
    assert_eq!(status.creator.as_ref().ok_or("missing creator")?.id, 77);
    for creator in [
        None,
        Some(serde_json::from_slice(include_bytes!(
            "../fixtures/gitea-user.json"
        ))?),
        Some(serde_json::from_slice(include_bytes!(
            "../fixtures/forgejo-user.json"
        ))?),
    ] {
        status.creator = creator;
        status.id = js_int::MAX_SAFE_UINT;
        let page = vec![status.clone()];
        super::numbers::assert_integer_contract(&page, js_int::MAX_SAFE_INT)?;
    }
    status.id = js_int::MAX_SAFE_UINT + 1;
    assert!(serde_json::to_vec(&status).is_err());
    Ok(())
}

#[test]
fn status_records_reject_unknown_missing_and_normalized_fields()
-> Result<(), Box<dyn std::error::Error>> {
    let input = include_str!("../fixtures/commit-status.json");
    for (old, new) in [
        (r#""id": 42"#, r#""id": 42, "extra": true"#),
        (r#""id": 42"#, r#""id": 42, "\u0069d": 42"#),
        (r#""status": "success""#, r#""status": "unknown""#),
        (r#""status": "success""#, r#""status": 1"#),
        (r#""created_at": "2026-08-31T12:00:00Z","#, ""),
        (r#""updated_at": "2026-08-31T12:00:00Z","#, ""),
        (
            r#""url": "https://forge.example/api/v1/repos/acme/widget/statuses/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"#,
            r#""url": null"#,
        ),
        (
            r#""created_at": "2026-08-31T12:00:00Z""#,
            r#""created_at": false"#,
        ),
        (r#""id": 77"#, r#""id": 77, "login": null"#),
    ] {
        let changed = input.replace(old, new);
        assert_ne!(changed, input);
        assert!(serde_json::from_str::<CommitStatusRecord>(&changed).is_err());
        assert!(amiss_wire::read_json::<CommitStatusRecord>(changed.as_bytes(), u64::MAX).is_err());
        assert!(
            amiss_wire::read_json::<Vec<CommitStatusRecord>>(
                format!("[{changed}]").as_bytes(),
                u64::MAX
            )
            .is_err()
        );
    }
    let status: CommitStatusRecord = serde_json::from_str(input)?;
    let creator = status.creator.as_ref().ok_or("missing creator")?;
    let encoded = serde_json::to_string(&status)?;
    let removed = encoded.replace(
        &format!("\"creator\":{},", serde_json::to_string(creator)?),
        "",
    );
    assert_ne!(removed, encoded);
    assert!(serde_json::from_str::<CommitStatusRecord>(&removed).is_err());
    assert!(amiss_wire::read_json::<CommitStatusRecord>(removed.as_bytes(), u64::MAX).is_err());
    for invalid in [
        "null",
        "[]",
        "{}",
        "true",
        "[42,null,\"success\",\"\",\"\",\"\",\"\",\"\",\"\"]",
    ] {
        assert!(amiss_wire::read_json::<CommitStatusRecord>(invalid.as_bytes(), u64::MAX).is_err());
    }
    Ok(())
}

#[test]
fn status_states_keep_the_provider_spellings_in_both_directions()
-> Result<(), Box<dyn std::error::Error>> {
    let input = br#"{"state":"failure","target_url":"","description":"amiss-relation-v1: example","context":"Amiss cross-repository"}"#;
    let mut request: CreateCommitStatus = serde_json::from_slice(input)?;
    assert_eq!(request.state, CommitStatusState::Failure);
    assert_eq!(serde_json::to_vec(&request)?, input);
    let mut response: CommitStatusRecord =
        serde_json::from_slice(include_bytes!("../fixtures/commit-status.json"))?;
    for (state, spelling) in [
        (CommitStatusState::Pending, "pending"),
        (CommitStatusState::Success, "success"),
        (CommitStatusState::Error, "error"),
        (CommitStatusState::Failure, "failure"),
        (CommitStatusState::Warning, "warning"),
        (CommitStatusState::Skipped, "skipped"),
    ] {
        assert_eq!(
            serde_json::to_string(&state)?,
            serde_json::to_string(spelling)?
        );
        request.state = state;
        response.status = request.state;
        assert_eq!(
            amiss_wire::read_json::<CreateCommitStatus>(&serde_json::to_vec(&request)?, u64::MAX)?,
            request
        );
        assert_eq!(
            amiss_wire::read_json::<CommitStatusRecord>(&serde_json::to_vec(&response)?, u64::MAX)?,
            response
        );
    }
    Ok(())
}
