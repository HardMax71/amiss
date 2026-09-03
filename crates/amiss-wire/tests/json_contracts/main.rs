use std::{fs, path::Path};

use amiss_wire::{controls, json, locale, publication, relation, requests, semantic};

#[path = "../support/relation.rs"]
mod relation_fixture;

#[test]
fn sidecar_examples_match_their_typed_sources() {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/examples");
    let contract = relation_fixture::relation_contract();
    let generated_plan = relation::plan(&contract.plan).unwrap();
    let committed_plan = fs::read(examples.join("relation-plan.json")).unwrap();
    assert_eq!(
        relation::parse_plan(&committed_plan).unwrap().payload,
        contract.plan
    );
    assert_eq!(
        json::canonical(&generated_plan),
        json::canonical(&json::parse(&committed_plan).unwrap())
    );

    let generated_evidence = relation::evidence(&contract.evidence).unwrap();
    let committed_evidence = fs::read(examples.join("relation-evidence.json")).unwrap();
    assert_eq!(
        relation::parse_evidence(&committed_evidence)
            .unwrap()
            .payload,
        contract.evidence
    );
    assert_eq!(
        json::canonical(&generated_evidence),
        json::canonical(&json::parse(&committed_evidence).unwrap())
    );

    let publication_plan_bytes = fs::read(examples.join("publication-plan.json")).unwrap();
    let publication_plan = publication::parse_plan(&publication_plan_bytes).unwrap();
    assert_eq!(
        json::canonical(&publication::plan(&publication_plan.payload).unwrap()),
        json::canonical(&json::parse(&publication_plan_bytes).unwrap())
    );

    let publication_evidence_bytes = fs::read(examples.join("publication-evidence.json")).unwrap();
    let publication_evidence = publication::parse_evidence(&publication_evidence_bytes).unwrap();
    assert_eq!(
        json::canonical(&publication::evidence(&publication_evidence.payload).unwrap()),
        json::canonical(&json::parse(&publication_evidence_bytes).unwrap())
    );

    let publication_assessment_bytes =
        fs::read(examples.join("publication-assessment.json")).unwrap();
    let publication_assessment =
        publication::parse_assessment(&publication_assessment_bytes).unwrap();
    let replayed = publication::assess(
        &publication_plan,
        Some(&publication_evidence),
        &publication_assessment.payload.engine.engine_version,
        publication_assessment.payload.engine.engine_digest,
    )
    .unwrap();
    assert_eq!(
        json::canonical(&replayed),
        json::canonical(&json::parse(&publication_assessment_bytes).unwrap())
    );

    let locale_plan_bytes = fs::read(examples.join("locale-coverage-plan.json")).unwrap();
    let locale_plan = locale::parse_plan(&locale_plan_bytes).unwrap();
    assert_eq!(
        json::canonical(&locale::plan(&locale_plan.payload).unwrap()),
        json::canonical(&json::parse(&locale_plan_bytes).unwrap())
    );

    let locale_evidence_bytes = fs::read(examples.join("locale-coverage-evidence.json")).unwrap();
    let locale_evidence = locale::parse_evidence(&locale_evidence_bytes).unwrap();
    assert_eq!(
        json::canonical(&locale::evidence(&locale_evidence.payload).unwrap()),
        json::canonical(&json::parse(&locale_evidence_bytes).unwrap())
    );

    let locale_assessment_bytes =
        fs::read(examples.join("locale-coverage-assessment.json")).unwrap();
    let locale_assessment = locale::parse_assessment(&locale_assessment_bytes).unwrap();
    let replayed = locale::assess(
        &locale_plan,
        Some(&locale_evidence),
        &locale_assessment.payload.engine.engine_version,
        locale_assessment.payload.engine.engine_digest,
    )
    .unwrap();
    assert_eq!(
        json::canonical(&replayed),
        json::canonical(&json::parse(&locale_assessment_bytes).unwrap())
    );

    let record_input_bytes = fs::read(examples.join("scanner-record-set-input.json")).unwrap();
    let record_input = semantic::record::parse_input(&record_input_bytes).unwrap();
    assert_eq!(
        serde_json_canonicalizer::to_vec(&record_input).unwrap(),
        json::canonical(&json::parse(&record_input_bytes).unwrap())
    );

    let semantic_evidence_bytes =
        fs::read(examples.join("scanner-semantic-evidence.json")).unwrap();
    let semantic_evidence = semantic::parse(&semantic_evidence_bytes).unwrap();
    assert_eq!(
        serde_json_canonicalizer::to_vec(&semantic_evidence).unwrap(),
        json::canonical(&json::parse(&semantic_evidence_bytes).unwrap())
    );

    let semantic_template_bytes =
        fs::read(examples.join("scanner-semantic-template.json")).unwrap();
    let semantic_template = semantic::parse_template(&semantic_template_bytes).unwrap();
    assert_eq!(
        serde_json_canonicalizer::to_vec(&semantic_template).unwrap(),
        json::canonical(&json::parse(&semantic_template_bytes).unwrap())
    );
}

#[test]
fn sealed_request_examples_match_their_typed_sources() {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/examples");
    let evaluation_bytes = fs::read(examples.join("scanner-evaluation-request.json")).unwrap();
    let evaluation = requests::EvaluationRequest::parse(&evaluation_bytes).unwrap();
    assert_eq!(
        serde_json_canonicalizer::to_vec(&evaluation).unwrap(),
        json::canonical(&json::parse(&evaluation_bytes).unwrap())
    );
    let snapshot_bytes = fs::read(examples.join("scanner-snapshot-request.json")).unwrap();
    let snapshot = requests::SnapshotRequest::parse(&snapshot_bytes).unwrap();
    assert_eq!(
        serde_json_canonicalizer::to_vec(&snapshot).unwrap(),
        json::canonical(&json::parse(&snapshot_bytes).unwrap())
    );
    let bytes = fs::read(examples.join("scanner-controls-request.json")).unwrap();
    let request = requests::ControlsRequest::parse(&bytes).unwrap();
    assert_eq!(
        serde_json_canonicalizer::to_vec(&request).unwrap(),
        json::canonical(&json::parse(&bytes).unwrap())
    );
    let time_bytes = fs::read(examples.join("scanner-trusted-time-statement.json")).unwrap();
    let statement = controls::parse_trusted_time(&time_bytes).unwrap();
    assert_eq!(
        controls::canonical_trusted_time(&statement).unwrap().0,
        json::canonical(&json::parse(&time_bytes).unwrap())
    );
    let constraint_bytes = fs::read(examples.join("scanner-execution-constraint.json")).unwrap();
    let constraint = controls::parse_execution_constraint(&constraint_bytes).unwrap();
    assert_eq!(
        controls::canonical_execution_constraint(&constraint)
            .unwrap()
            .0,
        json::canonical(&json::parse(&constraint_bytes).unwrap())
    );
}

#[test]
fn exception_control_examples_match_their_typed_sources() {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/examples");
    let debt_bytes = fs::read(examples.join("debt-snapshot.json")).unwrap();
    let debt = controls::parse_debt_snapshot(&debt_bytes).unwrap();
    assert_eq!(
        controls::canonical_debt_snapshot(&debt).unwrap().0,
        json::canonical(&json::parse(&debt_bytes).unwrap())
    );

    let waiver_bytes = fs::read(examples.join("waiver-bundle.json")).unwrap();
    let waiver = controls::parse_waiver_bundle(&waiver_bytes).unwrap();
    assert_eq!(
        controls::canonical_waiver_bundle(&waiver).unwrap().0,
        json::canonical(&json::parse(&waiver_bytes).unwrap())
    );
}

#[test]
fn candidate_identity_examples_match_their_typed_source() {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/examples");
    for name in ["candidate-identity.json", "candidate-identity-index.json"] {
        let bytes = fs::read(examples.join(name)).unwrap();
        let identity: requests::CandidateIdentity = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            serde_json_canonicalizer::to_vec(&identity).unwrap(),
            json::canonical(&json::parse(&bytes).unwrap())
        );
    }
}
