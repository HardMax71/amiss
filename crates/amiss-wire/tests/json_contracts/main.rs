use std::{fs, path::Path};

use amiss_wire::{json, publication, relation};

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
}
