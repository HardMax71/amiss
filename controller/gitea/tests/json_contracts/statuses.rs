use amiss_controller_gitea::status::{CommitStatusRecord, CommitStatusState, CreateCommitStatus};

#[test]
fn status_pages_keep_owned_feedback_and_nullable_creators() -> Result<(), Box<dyn std::error::Error>>
{
    let mut status: CommitStatusRecord =
        serde_json::from_slice(include_bytes!("../fixtures/commit-status.json"))?;
    assert_eq!(status.status, CommitStatusState::Success);
    assert_eq!(status.context, "Amiss cross-repository");
    assert_eq!(status.creator.as_ref().ok_or("missing creator")?.id, 77);
    let minimal = serde_json::to_string(&status)?;
    let metadata = minimal.replacen(
        '{',
        r#"{"created_at":false,"updated_at":42,"url":null,"extra":[],"#,
        1,
    );
    assert_eq!(
        serde_json::from_str::<CommitStatusRecord>(&metadata)?,
        status
    );
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
        super::numbers::assert_integer_contract(&vec![status.clone()], js_int::MAX_SAFE_INT)?;
        let positional = serde_json::to_vec(&(
            status.id,
            &status.creator,
            status.status,
            &status.target_url,
            &status.description,
            &status.context,
        ))?;
        assert_eq!(
            serde_json::from_slice::<CommitStatusRecord>(&positional)?,
            status
        );
    }
    status.id = js_int::MAX_SAFE_UINT + 1;
    assert!(serde_json::to_vec(&status).is_err());
    Ok(())
}

#[test]
fn status_records_require_owned_identity_and_feedback_fields()
-> Result<(), Box<dyn std::error::Error>> {
    let status: CommitStatusRecord =
        serde_json::from_slice(include_bytes!("../fixtures/commit-status.json"))?;
    let encoded = serde_json::to_string(&status)?;
    for (field, value) in [
        ("id", status.id.to_string()),
        ("creator", serde_json::to_string(&status.creator)?),
        ("status", serde_json::to_string(&status.status)?),
        ("target_url", serde_json::to_string(&status.target_url)?),
        ("description", serde_json::to_string(&status.description)?),
        ("context", serde_json::to_string(&status.context)?),
    ] {
        for replacement in [
            format!(r#""missing_{field}":{value}"#),
            format!(r#""{field}":{value},"{field}":{value}"#),
        ] {
            let invalid = encoded.replacen(&format!(r#""{field}":{value}"#), &replacement, 1);
            assert_ne!(invalid, encoded);
            assert!(
                serde_json::from_str::<CommitStatusRecord>(&invalid).is_err(),
                "{field}"
            );
        }
    }
    for (old, new) in [
        (r#""id":42"#, r#""id":42,"\u0069d":42"#),
        (r#""status":"success""#, r#""status":"unknown""#),
        (r#""status":"success""#, r#""status":1"#),
        (r#""id":77"#, r#""id":77,"login":null"#),
        (r#""description":"#, r#""description":null,"description":"#),
    ] {
        let invalid = encoded.replacen(old, new, 1);
        assert_ne!(invalid, encoded);
        assert!(
            serde_json::from_str::<CommitStatusRecord>(&invalid).is_err(),
            "{new}"
        );
        assert!(serde_json::from_str::<Vec<CommitStatusRecord>>(&format!("[{invalid}]")).is_err());
    }
    for invalid in ["null", "[]", "{}", "true", "[42,null]"] {
        assert!(serde_json::from_str::<CommitStatusRecord>(invalid).is_err());
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
            serde_json::from_slice::<CommitStatusRecord>(&serde_json::to_vec(&response)?)?,
            response
        );
    }
    Ok(())
}
