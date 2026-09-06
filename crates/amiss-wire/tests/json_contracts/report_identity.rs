use amiss_wire::report::model::{IdentityEvaluation, IdentityPreimage};
use amiss_wire::requests::CandidateIdentitySchema;
use serde_json::{Value, json};

#[test]
fn snapshot_and_unavailable_tags_require_strings() -> serde_json::Result<()> {
    use amiss_wire::report::model::UnavailableStatus;
    use amiss_wire::requests::GitSnapshotKind;

    assert_eq!(
        serde_json::to_value(GitSnapshotKind::GitCommit)?,
        json!("git-commit")
    );
    assert_eq!(
        serde_json::to_value(UnavailableStatus::Unavailable)?,
        json!("unavailable")
    );
    for value in [
        json!({"git-commit": null}),
        Value::Null,
        json!(false),
        json!(1),
    ] {
        assert!(serde_json::from_value::<GitSnapshotKind>(value).is_err());
    }
    for value in [
        json!({"unavailable": null}),
        Value::Null,
        json!(false),
        json!(1),
    ] {
        assert!(serde_json::from_value::<UnavailableStatus>(value).is_err());
    }
    Ok(())
}

#[test]
fn report_identity_projections_keep_the_candidate_contract() -> serde_json::Result<()> {
    for bytes in [
        include_bytes!("../../../../spec/examples/candidate-identity.json").as_slice(),
        include_bytes!("../../../../spec/examples/candidate-identity-index.json").as_slice(),
    ] {
        let mut expected: Value = serde_json::from_slice(bytes)?;
        for extended in [false, true] {
            if extended {
                expected["future"] = json!({"\u{1f600}": [null, true, -7], "\u{e000}": "extra"});
                expected["candidate"]["future"] = json!({"nested": [1, 2]});
            }
            for trusted in [None, Some(false), Some(true)] {
                let mut evaluation = expected.clone();
                evaluation.as_object_mut().unwrap().remove("schema");
                if let Some(trusted) = trusted {
                    evaluation["evaluation_instant"] = json!("2026-09-06T10:00:00Z");
                    evaluation["trusted_time"] = json!(trusted);
                }
                let preimage = IdentityPreimage {
                    evaluation: serde_json::from_value(evaluation.clone())?,
                    schema: CandidateIdentitySchema::Current,
                };
                assert_eq!(serde_json::to_value(&preimage)?, expected);
                assert_eq!(
                    serde_json_canonicalizer::to_vec(&preimage)?,
                    serde_json_canonicalizer::to_vec(&expected)?,
                );
                for reserved in [
                    Value::Null,
                    json!(false),
                    json!(CandidateIdentitySchema::Current),
                ] {
                    evaluation["schema"] = reserved;
                    assert!(
                        serde_json::from_value::<IdentityEvaluation>(evaluation.clone()).is_err()
                    );
                }
            }
        }
    }
    Ok(())
}
