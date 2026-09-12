#![cfg(test)]

use super::accepted_report;
use crate::ArtifactError;

#[test]
fn candidate_identity_preserves_additive_fields_at_every_depth() -> Result<(), ArtifactError> {
    let fixture = amiss_fixtures::publication_audit(true).ok_or(ArtifactError::Corrupt)?;
    let original = accepted_report(&fixture.report)?;
    for parent in ["", "/repository", "/base", "/candidate"] {
        let mut report: serde_json::Value =
            serde_json::from_slice(&fixture.report).map_err(|_defect| ArtifactError::Corrupt)?;
        let object = report
            .pointer_mut(&format!("/payload/evaluation{parent}"))
            .and_then(serde_json::Value::as_object_mut)
            .ok_or(ArtifactError::Corrupt)?;
        object.insert(
            "future_binding".to_owned(),
            serde_json::json!({"value": null}),
        );
        let accepted = accepted_report(&rebind(&mut report)?)?;
        assert_ne!(
            original.candidate_identity_digest, accepted.candidate_identity_digest,
            "{parent}"
        );
        assert_eq!(original.repository, accepted.repository);
        assert_eq!(original.base.commit, accepted.base.commit);
        assert_eq!(original.candidate.tree, accepted.candidate.tree);
    }
    Ok(())
}

#[test]
fn a_received_candidate_schema_cannot_replace_the_identity_domain() -> Result<(), ArtifactError> {
    let fixture = amiss_fixtures::publication_audit(true).ok_or(ArtifactError::Corrupt)?;
    let mut report: serde_json::Value =
        serde_json::from_slice(&fixture.report).map_err(|_defect| ArtifactError::Corrupt)?;
    report["payload"]["evaluation"]["schema"] = serde_json::Value::Null;
    assert!(matches!(
        accepted_report(&rebind(&mut report)?),
        Err(ArtifactError::Corrupt)
    ));
    Ok(())
}

#[test]
fn repository_identity_requires_an_object_even_with_a_valid_report_digest()
-> Result<(), ArtifactError> {
    let fixture = amiss_fixtures::publication_audit(true).ok_or(ArtifactError::Corrupt)?;
    let mut report: serde_json::Value =
        serde_json::from_slice(&fixture.report).map_err(|_defect| ArtifactError::Corrupt)?;
    let repository = &report["payload"]["evaluation"]["repository"];
    let positional =
        serde_json::json!([repository["host"], repository["owner"], repository["name"]]);
    report["payload"]["evaluation"]["repository"] = positional;
    assert!(matches!(
        accepted_report(&rebind(&mut report)?),
        Err(ArtifactError::Corrupt)
    ));
    Ok(())
}

fn rebind(report: &mut serde_json::Value) -> Result<Vec<u8>, ArtifactError> {
    let digest = amiss_wire::codec::digest(amiss_wire::report::PAYLOAD_SCHEMA, &report["payload"])
        .map_err(|_defect| ArtifactError::Corrupt)?;
    report["payload_digest"] = serde_json::Value::String(digest.to_string());
    amiss_wire::codec::canonical(report).map_err(|_defect| ArtifactError::Corrupt)
}
