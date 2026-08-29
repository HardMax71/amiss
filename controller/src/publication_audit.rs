mod tests;

use amiss_wire::digest::{Digest, hj, sha256};
use amiss_wire::json::{self, Value};
use amiss_wire::model::{ObjectFormat, Oid, RepositoryIdentity};
use amiss_wire::publication::{
    DocsCandidate, PUBLICATION_DOCUMENT_BYTES, PublicationVerdict, assess, parse_assessment,
    parse_evidence, parse_plan,
};

use crate::ArtifactError;

#[derive(Clone, Copy)]
pub struct PublicationAuditBundle<'a> {
    pub report: &'a [u8],
    pub plan: &'a [u8],
    pub evidence: Option<&'a [u8]>,
    pub assessment: &'a [u8],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PublicationAuditDigests {
    pub report_digest: Digest,
    pub plan_digest: Digest,
    pub evidence_digest: Option<Digest>,
    pub assessment_digest: Digest,
    pub verdict: PublicationVerdict,
}

/// Validates one complete, report-bound publication audit before retention.
///
/// The plan must describe the repository candidate in the accepted report,
/// and the assessment must replay exactly from the supplied plan and optional
/// evidence. No provider material is acquired or interpreted here.
///
/// # Errors
///
/// Returns [`ArtifactError::TooLarge`] when a component crosses its contract
/// ceiling and [`ArtifactError::Corrupt`] for every malformed or inconsistent
/// chain.
pub fn validate_publication_audit(
    bundle: PublicationAuditBundle<'_>,
) -> Result<PublicationAuditDigests, ArtifactError> {
    if u64::try_from(bundle.report.len()).unwrap_or(u64::MAX)
        > amiss_wire::report::MACHINE_JSON_BYTES
        || [bundle.plan, bundle.assessment]
            .into_iter()
            .chain(bundle.evidence)
            .any(|bytes| {
                u64::try_from(bytes.len()).unwrap_or(u64::MAX) > PUBLICATION_DOCUMENT_BYTES
            })
    {
        return Err(ArtifactError::TooLarge);
    }
    let report = json::parse(bundle.report).map_err(|_defect| ArtifactError::Corrupt)?;
    let (_, report_payload_digest, verdict) =
        amiss_wire::report::validate_envelope(&report).map_err(|_defect| ArtifactError::Corrupt)?;
    if verdict == amiss_wire::ExitClass::Failure {
        return Err(ArtifactError::Corrupt);
    }
    let report_payload_digest =
        Digest::from_wire(report_payload_digest).ok_or(ArtifactError::Corrupt)?;
    let Value::Object(envelope) = report else {
        return Err(ArtifactError::Corrupt);
    };
    let payload = envelope
        .into_vec()
        .into_iter()
        .find_map(|(key, value)| (key == "payload").then_some(value))
        .ok_or(ArtifactError::Corrupt)?;
    let report_docs = report_docs(payload).ok_or(ArtifactError::Corrupt)?;
    let plan = parse_plan(bundle.plan).map_err(|_defect| ArtifactError::Corrupt)?;
    if plan.payload.report_payload_digest != report_payload_digest
        || plan.payload.docs != report_docs
    {
        return Err(ArtifactError::Corrupt);
    }
    let evidence = bundle
        .evidence
        .map(parse_evidence)
        .transpose()
        .map_err(|_defect| ArtifactError::Corrupt)?;
    let assessment =
        parse_assessment(bundle.assessment).map_err(|_defect| ArtifactError::Corrupt)?;
    let replayed = assess(
        &plan,
        evidence.as_ref(),
        &assessment.payload.engine_version,
        assessment.payload.engine_digest,
    )
    .map_err(|_defect| ArtifactError::Corrupt)?;
    if replayed.text("payload_digest") != Some(&assessment.payload_digest.to_string()) {
        return Err(ArtifactError::Corrupt);
    }
    Ok(PublicationAuditDigests {
        report_digest: sha256(bundle.report),
        plan_digest: sha256(bundle.plan),
        evidence_digest: bundle.evidence.map(sha256),
        assessment_digest: sha256(bundle.assessment),
        verdict: assessment.payload.verdict,
    })
}

fn report_docs(payload: Value) -> Option<DocsCandidate> {
    let Value::Object(payload) = payload else {
        return None;
    };
    let evaluation = payload
        .into_vec()
        .into_iter()
        .find_map(|(key, value)| (key == "evaluation").then_some(value))?;
    if evaluation.text("mode") != Some("commit-pair")
        || evaluation.text("event_kind") != Some("explicit-commit-pair")
        || evaluation.text("finality") != Some("explicit-replay")
        || evaluation.text("materialization") != Some("git-objects")
        || evaluation.member("skip_worktree_paths") != Some(&Value::Integer(0))
        || evaluation.member("index_only_materialized_paths") != Some(&Value::Integer(0))
        || evaluation.member("schema").is_some()
    {
        return None;
    }
    let repository = evaluation.member("repository")?;
    let repository = RepositoryIdentity::new(
        repository.text("host")?.to_owned(),
        repository.text("owner")?.to_owned(),
        repository.text("name")?.to_owned(),
    )?;
    let (base_format, _base_commit, _base_tree) = snapshot(evaluation.member("base")?)?;
    let (object_format, commit, tree) = snapshot(evaluation.member("candidate")?)?;
    if base_format != object_format {
        return None;
    }
    let Value::Object(members) = evaluation else {
        return None;
    };
    let mut identity = vec![(
        "schema".to_owned(),
        Value::string(amiss_wire::requests::CANDIDATE_IDENTITY_DOMAIN.to_owned()),
    )];
    identity.extend(
        members
            .into_vec()
            .into_iter()
            .filter(|(key, _value)| !matches!(key.as_str(), "evaluation_instant" | "trusted_time")),
    );
    let identity = Value::object(identity);
    Some(DocsCandidate {
        repository,
        commit,
        tree,
        candidate_identity_digest: hj(amiss_wire::requests::CANDIDATE_IDENTITY_DOMAIN, &identity),
    })
}

fn snapshot(value: &Value) -> Option<(ObjectFormat, Oid, Oid)> {
    if value.text("kind") != Some("git-commit") {
        return None;
    }
    let object_format = value.text("object_format")?.parse().ok()?;
    Some((
        object_format,
        Oid::new(object_format, value.text("commit_oid")?.to_owned())?,
        Oid::new(object_format, value.text("tree_oid")?.to_owned())?,
    ))
}
