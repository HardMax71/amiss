#![cfg(test)]

use amiss_wire::digest::{hb, sha256};
use amiss_wire::report::PAYLOAD_SCHEMA;
use amiss_wire::requests::CANDIDATE_IDENTITY_DOMAIN;
use serde_json::{Value, json};

use super::accepted_report;
use crate::ArtifactError;

#[test]
fn audit_acceptance_keeps_the_commit_pair_and_identity_constraints() -> Result<(), ArtifactError> {
    let fixture = amiss_fixtures::publication_audit(true).ok_or(ArtifactError::Corrupt)?;
    for (path, invalid) in [
        ("/mode", json!("index")),
        ("/event_kind", json!("pull-request")),
        ("/finality", json!("pr-synthetic-merge")),
        ("/materialization", json!("index")),
        ("/skip_worktree_paths", json!(1)),
        ("/index_only_materialized_paths", json!(1)),
        ("/repository", Value::Null),
        ("/repository/host", json!("host/part")),
        ("/repository/owner", json!("../owner")),
        ("/repository/name", json!("../repo")),
        ("/target_ref", json!("refs/tags/v1")),
        ("/base/kind", json!("index")),
        ("/candidate/kind", json!("index")),
        ("/base/object_format", json!("sha256")),
        ("/candidate/object_format", json!("sha256")),
        ("/base/commit_oid", json!("a".repeat(64))),
        ("/base/tree_oid", json!("b".repeat(64))),
        ("/candidate/commit_oid", json!("c".repeat(64))),
        ("/candidate/tree_oid", json!("d".repeat(64))),
    ] {
        let bytes = edited_report(&fixture.report, |evaluation| {
            *evaluation.pointer_mut(path).ok_or(ArtifactError::Corrupt)? = invalid;
            Ok(())
        })?;
        assert!(
            matches!(accepted_report(&bytes), Err(ArtifactError::Corrupt)),
            "accepted {path}"
        );
    }
    for schema in [Value::Null, json!(CANDIDATE_IDENTITY_DOMAIN), json!(false)] {
        let bytes = edited_report(&fixture.report, |evaluation| {
            evaluation["schema"] = schema;
            Ok(())
        })?;
        assert!(matches!(
            accepted_report(&bytes),
            Err(ArtifactError::Corrupt)
        ));
    }
    let mixed = edited_report(&fixture.report, |evaluation| {
        evaluation["candidate"]["object_format"] = json!("sha256");
        evaluation["candidate"]["commit_oid"] = json!("c".repeat(64));
        evaluation["candidate"]["tree_oid"] = json!("d".repeat(64));
        Ok(())
    })?;
    assert!(matches!(
        accepted_report(&mixed),
        Err(ArtifactError::Corrupt)
    ));
    let sha256_report = edited_report(&mixed, |evaluation| {
        evaluation["base"]["object_format"] = json!("sha256");
        evaluation["base"]["commit_oid"] = json!("a".repeat(64));
        evaluation["base"]["tree_oid"] = json!("b".repeat(64));
        evaluation["target_ref"] = json!("refs/heads/main");
        Ok(())
    })?;
    let accepted = accepted_report(&sha256_report)?;
    assert_eq!(accepted.base.commit.as_str(), "a".repeat(64));
    assert_eq!(accepted.base.tree.as_str(), "b".repeat(64));
    assert_eq!(accepted.candidate.commit.as_str(), "c".repeat(64));
    assert_eq!(accepted.candidate.tree.as_str(), "d".repeat(64));
    assert_eq!(
        accepted
            .target_ref
            .as_ref()
            .map(amiss_wire::model::BranchRef::as_str),
        Some("refs/heads/main")
    );
    Ok(())
}

#[test]
fn additive_evaluation_fields_remain_bound_but_time_does_not() -> Result<(), ArtifactError> {
    let fixture = amiss_fixtures::publication_audit(true).ok_or(ArtifactError::Corrupt)?;
    let original = accepted_report(&fixture.report)?;
    let timed = edited_report(&fixture.report, |evaluation| {
        evaluation["evaluation_instant"] = json!("2026-09-05T12:00:00Z");
        evaluation["trusted_time"] = json!(true);
        Ok(())
    })?;
    let accepted = accepted_report(&timed)?;
    assert_eq!(
        accepted.candidate_identity_digest,
        original.candidate_identity_digest
    );
    assert_ne!(accepted.payload_digest, original.payload_digest);

    for path in ["", "/base", "/candidate"] {
        let extended = edited_report(&fixture.report, |evaluation| {
            evaluation.pointer_mut(path).ok_or(ArtifactError::Corrupt)?["additive"] =
                json!({"\u{1f600}": [null, true, -7], "\u{e000}": "extra"});
            Ok(())
        })?;
        let mut decoded: Value =
            serde_json::from_slice(&extended).map_err(|_defect| ArtifactError::Corrupt)?;
        let mut identity = decoded["payload"]["evaluation"].take();
        let members = identity.as_object_mut().ok_or(ArtifactError::Corrupt)?;
        members.remove("evaluation_instant");
        members.remove("trusted_time");
        members.insert("schema".to_owned(), json!(CANDIDATE_IDENTITY_DOMAIN));
        let preimage = serde_json_canonicalizer::to_vec(&identity)
            .map_err(|_defect| ArtifactError::Corrupt)?;
        let accepted = accepted_report(&extended)?;
        assert_eq!(accepted.report_digest, sha256(&extended));
        assert_eq!(
            accepted.candidate_identity_digest,
            hb(CANDIDATE_IDENTITY_DOMAIN, &preimage)
        );
        assert_ne!(
            accepted.candidate_identity_digest, original.candidate_identity_digest,
            "{path}"
        );
        assert_ne!(accepted.payload_digest, original.payload_digest, "{path}");
    }
    Ok(())
}

fn edited_report(
    bytes: &[u8],
    edit: impl FnOnce(&mut Value) -> Result<(), ArtifactError>,
) -> Result<Vec<u8>, ArtifactError> {
    let mut report: Value =
        serde_json::from_slice(bytes).map_err(|_defect| ArtifactError::Corrupt)?;
    edit(&mut report["payload"]["evaluation"])?;
    let payload = serde_json_canonicalizer::to_vec(&report["payload"])
        .map_err(|_defect| ArtifactError::Corrupt)?;
    report["payload_digest"] = json!(hb(PAYLOAD_SCHEMA, &payload));
    serde_json_canonicalizer::to_vec(&report).map_err(|_defect| ArtifactError::Corrupt)
}
