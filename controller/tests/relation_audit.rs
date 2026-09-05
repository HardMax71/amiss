use amiss_controller::{
    ArtifactError, RelationAuditBundle, relation_audit_plan, validate_relation_audit,
};
use amiss_controller_fixtures::relation::{RelationAuditFixture, relation_audit};
use amiss_wire::digest::{hj, sha256};
use amiss_wire::json::{self, Value};
use amiss_wire::model::ArtifactId;
use amiss_wire::relation::{RelationVerdict, assess, parse_assessment, parse_plan, plan};

#[test]
fn one_exact_chain_binds_every_byte_to_the_trigger_and_operator_plan() -> Result<(), ArtifactError>
{
    let fixture = relation_audit(true).ok_or(ArtifactError::Corrupt)?;
    assert_eq!(
        relation_audit_plan(&fixture.transition, &fixture.report)?,
        fixture.plan
    );
    let audit = validate_relation_audit(bundle(&fixture))?;

    assert_eq!(audit.report_digest, sha256(&fixture.report));
    assert_eq!(audit.plan_digest, sha256(&fixture.plan));
    assert_eq!(
        audit.evidence_digest,
        fixture.evidence.as_deref().map(sha256)
    );
    assert_eq!(audit.assessment_digest, sha256(&fixture.assessment));
    assert_eq!(audit.verdict, RelationVerdict::IntroducedDrift);
    Ok(())
}

#[test]
fn absent_evidence_remains_a_replayable_unproven_audit() -> Result<(), ArtifactError> {
    let fixture = relation_audit(false).ok_or(ArtifactError::Corrupt)?;
    let audit = validate_relation_audit(bundle(&fixture))?;

    assert_eq!(audit.evidence_digest, None);
    assert_eq!(audit.verdict, RelationVerdict::Unproven);
    Ok(())
}

#[test]
fn changed_transition_report_and_assessment_bindings_are_refused() -> Result<(), ArtifactError> {
    let exact = relation_audit(true).ok_or(ArtifactError::Corrupt)?;
    let mut changed_transition = relation_audit(true).ok_or(ArtifactError::Corrupt)?;
    changed_transition.transition.subjects[1].trees.candidate =
        changed_transition.transition.subjects[1].trees.base.clone();
    assert!(matches!(
        relation_audit_plan(&changed_transition.transition, &changed_transition.report),
        Err(ArtifactError::Corrupt)
    ));
    assert!(matches!(
        validate_relation_audit(bundle(&changed_transition)),
        Err(ArtifactError::Corrupt)
    ));

    let mut changed_coordination = relation_audit(true).ok_or(ArtifactError::Corrupt)?;
    changed_coordination.transition.coordination =
        ArtifactId::new("workflow/release-43".to_owned()).ok_or(ArtifactError::Corrupt)?;
    assert!(matches!(
        validate_relation_audit(bundle(&changed_coordination)),
        Err(ArtifactError::Corrupt)
    ));

    let foreign_report = amiss_fixtures::publication_audit(true)
        .ok_or(ArtifactError::Corrupt)?
        .report;
    assert!(matches!(
        validate_relation_audit(RelationAuditBundle {
            report: &foreign_report,
            ..bundle(&exact)
        }),
        Err(ArtifactError::Corrupt)
    ));

    let unproven = relation_audit(false).ok_or(ArtifactError::Corrupt)?;
    assert!(matches!(
        validate_relation_audit(RelationAuditBundle {
            assessment: &unproven.assessment,
            ..bundle(&exact)
        }),
        Err(ArtifactError::Corrupt)
    ));
    Ok(())
}

#[test]
fn the_trigger_report_must_name_the_registered_target_ref() -> Result<(), ArtifactError> {
    let fixture = relation_audit(false).ok_or(ArtifactError::Corrupt)?;
    let fixture = with_null_report_target(fixture)?;

    assert!(matches!(
        relation_audit_plan(&fixture.transition, &fixture.report),
        Err(ArtifactError::Corrupt)
    ));
    assert!(matches!(
        validate_relation_audit(bundle(&fixture)),
        Err(ArtifactError::Corrupt)
    ));
    Ok(())
}

#[test]
fn oversized_relation_documents_are_refused_before_parsing() -> Result<(), ArtifactError> {
    let fixture = relation_audit(true).ok_or(ArtifactError::Corrupt)?;
    let oversized = vec![
        b' ';
        usize::try_from(amiss_wire::relation::RELATION_DOCUMENT_BYTES)
            .map_err(|_defect| ArtifactError::Corrupt)?
            .saturating_add(1)
    ];
    assert!(matches!(
        validate_relation_audit(RelationAuditBundle {
            plan: &oversized,
            ..bundle(&fixture)
        }),
        Err(ArtifactError::TooLarge)
    ));
    Ok(())
}

fn bundle(fixture: &RelationAuditFixture) -> RelationAuditBundle<'_> {
    RelationAuditBundle {
        transition: &fixture.transition,
        report: &fixture.report,
        plan: &fixture.plan,
        evidence: fixture.evidence.as_deref(),
        assessment: &fixture.assessment,
    }
}

fn with_null_report_target(
    mut fixture: RelationAuditFixture,
) -> Result<RelationAuditFixture, ArtifactError> {
    let recorded =
        parse_assessment(&fixture.assessment).map_err(|_defect| ArtifactError::Corrupt)?;
    let mut report = json::parse(&fixture.report).map_err(|_defect| ArtifactError::Corrupt)?;
    let Value::Object(envelope) = &mut report else {
        return Err(ArtifactError::Corrupt);
    };
    let payload = envelope
        .iter_mut()
        .find_map(|(key, value)| (key == "payload").then_some(value))
        .ok_or(ArtifactError::Corrupt)?;
    let Value::Object(payload_members) = payload else {
        return Err(ArtifactError::Corrupt);
    };
    let evaluation = payload_members
        .iter_mut()
        .find_map(|(key, value)| (key == "evaluation").then_some(value))
        .ok_or(ArtifactError::Corrupt)?;
    let Value::Object(evaluation_members) = evaluation else {
        return Err(ArtifactError::Corrupt);
    };
    *evaluation_members
        .iter_mut()
        .find_map(|(key, value)| (key == "target_ref").then_some(value))
        .ok_or(ArtifactError::Corrupt)? = Value::Null;
    let report_payload_digest = hj(amiss_wire::report::PAYLOAD_SCHEMA, payload);
    *envelope
        .iter_mut()
        .find_map(|(key, value)| (key == "payload_digest").then_some(value))
        .ok_or(ArtifactError::Corrupt)? = Value::string(report_payload_digest.to_string());
    fixture.report = json::canonical(&report);

    let mut rebound = parse_plan(&fixture.plan).map_err(|_defect| ArtifactError::Corrupt)?;
    rebound.payload.report_payload_digest = report_payload_digest;
    fixture.plan = plan(&rebound.payload).map_err(|_defect| ArtifactError::Corrupt)?;
    let rebound = parse_plan(&fixture.plan).map_err(|_defect| ArtifactError::Corrupt)?;
    fixture.assessment = assess(
        &rebound,
        None,
        &recorded.payload.engine.engine_version,
        recorded.payload.engine.engine_digest,
    )
    .map_err(|_defect| ArtifactError::Corrupt)?;
    Ok(fixture)
}
