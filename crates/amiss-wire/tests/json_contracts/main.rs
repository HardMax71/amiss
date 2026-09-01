use std::{fs, path::Path};

use amiss_wire::{json, relation};

#[path = "../support/relation.rs"]
mod relation_fixture;

#[test]
fn relation_examples_match_their_typed_source() {
    let contract = relation_fixture::relation_contract();
    let generated_plan = relation::plan(&contract.plan).unwrap();
    let committed_plan = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/examples/relation-plan.json"),
    )
    .unwrap();
    assert_eq!(
        relation::parse_plan(&committed_plan).unwrap().payload,
        contract.plan
    );
    assert_eq!(
        json::canonical(&generated_plan),
        json::canonical(&json::parse(&committed_plan).unwrap())
    );

    let generated_evidence = relation::evidence(&contract.evidence).unwrap();
    let committed_evidence = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/examples/relation-evidence.json"),
    )
    .unwrap();
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
}
