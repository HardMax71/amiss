use std::{collections::BTreeSet, fs, path::Path};

use amiss_wire::{
    controls, external, json, locale, manifest, publication, relation, report, requests, semantic,
};
use strum::IntoEnumIterator;

#[path = "../support/relation.rs"]
mod relation_fixture;

#[test]
fn document_classifications_match_the_report_schema() {
    let schema: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../../spec/scanner-report.schema.json"
    ))
    .unwrap();
    let declared: BTreeSet<_> =
        schema["$defs"]["DocumentResult"]["properties"]["classification"]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap().to_owned())
            .collect();
    let generated: BTreeSet<_> = report::model::DocumentClassification::iter()
        .map(|classification| {
            let value = serde_json::to_value(classification).unwrap();
            assert_eq!(value.as_str(), Some(classification.as_ref()));
            classification.as_ref().to_owned()
        })
        .collect();
    assert_eq!(declared, generated);
}

#[test]
fn resolver_reasons_fill_report_rows_without_changing_the_contract() {
    use amiss_wire::report::model::{
        ExternalResolutionKind, InvalidResolutionKind, RepoPath, Resolution,
        UnsupportedTargetResolutionKind,
    };
    use amiss_wire::resolution::{ExternalReference, InvalidReference, UnsupportedTargetTag};

    let schema: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../../spec/scanner-report.schema.json"
    ))
    .unwrap();
    let path = RepoPath::Text("docs/target.md".parse().unwrap());
    for (definition, rows) in [
        (
            "InvalidResolution",
            InvalidReference::iter()
                .map(|reason| {
                    (
                        reason.as_ref().to_owned(),
                        Resolution::Invalid {
                            kind: InvalidResolutionKind::Invalid,
                            reason,
                        },
                    )
                })
                .collect::<Vec<_>>(),
        ),
        (
            "ExternalResolution",
            ExternalReference::iter()
                .map(|reason| {
                    (
                        reason.as_ref().to_owned(),
                        Resolution::External {
                            kind: ExternalResolutionKind::External,
                            reason,
                        },
                    )
                })
                .collect(),
        ),
        (
            "UnsupportedTargetResolution",
            UnsupportedTargetTag::iter()
                .map(|reason| {
                    (
                        reason.as_ref().to_owned(),
                        Resolution::UnsupportedTarget {
                            kind: UnsupportedTargetResolutionKind::UnsupportedTarget,
                            path: path.clone(),
                            reason,
                        },
                    )
                })
                .collect(),
        ),
    ] {
        let declared: BTreeSet<_> = schema["$defs"][definition]["properties"]["reason"]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap().to_owned())
            .collect();
        let mut generated = BTreeSet::new();
        for (name, row) in rows {
            let bytes = serde_json::to_vec(&row).unwrap();
            assert_eq!(bytes, serde_json_canonicalizer::to_vec(&row).unwrap());
            assert_eq!(serde_json::from_slice::<Resolution>(&bytes).unwrap(), row);
            let mut value = serde_json::to_value(&row).unwrap();
            assert_eq!(value["reason"].as_str(), Some(name.as_str()));
            generated.insert(name);
            for invalid in [serde_json::Value::Null, "unknown-reason".into(), 0.into()] {
                value["reason"] = invalid;
                assert!(serde_json::from_value::<Resolution>(value.clone()).is_err());
            }
        }
        assert_eq!(declared, generated, "{definition}");
    }
}

#[test]
fn report_examples_match_their_typed_source() {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/examples");
    for name in ["scanner-report.canonical.json", "scanner-report.json"] {
        let bytes = fs::read(examples.join(name)).unwrap();
        let _: report::model::ReportEnvelope =
            serde_json::from_slice(&bytes).unwrap_or_else(|error| panic!("{name}: {error}"));
        let value = json::parse(&bytes).unwrap();
        let (payload, payload_digest, _verdict) =
            report::validate_envelope(&value).unwrap_or_else(|error| panic!("{name}: {error}"));
        let envelope = report::model::ReportEnvelope {
            payload,
            payload_digest,
            schema: report::model::ReportEnvelopeSchema::Current,
        };
        assert_eq!(
            serde_json_canonicalizer::to_vec(&envelope).unwrap(),
            json::canonical(&value),
            "{name}",
        );
    }

    for name in [
        "scanner-report.frozen-1.json",
        "scanner-report.last-released.json",
    ] {
        let bytes = fs::read(examples.join(name)).unwrap();
        let value = json::parse(&bytes).unwrap();
        report::validate_envelope(&value).unwrap_or_else(|error| panic!("{name}: {error}"));
    }
}

#[test]
fn optional_report_members_preserve_digest_bound_presence() {
    for document in [
        r#"{"reason":"path-not-found","near":null,"path":"docs/missing.md"}"#,
        r#"{"reason":"path-not-found","near":null,"path":"docs/missing.md","same_object_at":null}"#,
        r#"{"reason":"path-not-found","near":null,"path":"docs/missing.md","same_object_at":"docs/moved.md"}"#,
    ] {
        let resolution: report::model::MissingResolution = serde_json::from_str(document).unwrap();
        assert_eq!(
            serde_json_canonicalizer::to_vec(&resolution).unwrap(),
            json::canonical(&json::parse(document.as_bytes()).unwrap()),
        );
    }
}

#[test]
fn additive_report_fields_are_digest_bound_but_inert() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/examples/scanner-report.canonical.json");
    let mut document: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    document["payload"]["engine"]["future_provenance"] = serde_json::Value::Bool(true);
    let payload = serde_json_canonicalizer::to_vec(&document["payload"]).unwrap();
    document["payload_digest"] = serde_json::Value::String(
        amiss_wire::digest::hb(report::PAYLOAD_SCHEMA, &payload).to_string(),
    );
    let bytes = serde_json_canonicalizer::to_vec(&document).unwrap();
    let strict = json::parse(&bytes).unwrap();
    let (payload, _digest, _verdict) = report::validate_envelope(&strict).unwrap();
    assert!(!payload.engine.engine_version.is_empty());
}

#[test]
fn the_release_manifest_example_matches_its_typed_source() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/examples/scanner-release-manifest.json");
    let committed = fs::read(path).unwrap();
    let parsed_json = json::parse(&committed).unwrap();
    let release_manifest = manifest::parse_release_manifest(&committed).unwrap();
    let (generated, digest) = manifest::canonical_release_manifest(&release_manifest).unwrap();
    assert_eq!(generated, json::canonical(&parsed_json));
    assert_eq!(
        digest,
        amiss_wire::digest::hj(manifest::MANIFEST_DOMAIN, &parsed_json)
    );
}

#[test]
fn the_external_examples_match_their_typed_sources() {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/examples");
    let external_plan_bytes = fs::read(examples.join("scanner-external-plan.json")).unwrap();
    let external_plan = external::parse_plan(&external_plan_bytes).unwrap();
    assert_eq!(
        serde_json_canonicalizer::to_vec(&external_plan).unwrap(),
        json::canonical(&json::parse(&external_plan_bytes).unwrap())
    );
    let external_evidence_bytes =
        fs::read(examples.join("scanner-external-evidence.json")).unwrap();
    let external_evidence = external::parse_evidence(&external_evidence_bytes).unwrap();
    assert_eq!(
        external::evidence(&external_evidence).unwrap(),
        json::canonical(&json::parse(&external_evidence_bytes).unwrap())
    );
    let external_assessment_bytes =
        fs::read(examples.join("scanner-external-assessment.json")).unwrap();
    let external_assessment = external::parse_assessment(&external_assessment_bytes).unwrap();
    assert_eq!(
        serde_json_canonicalizer::to_vec(&external_assessment).unwrap(),
        json::canonical(&json::parse(&external_assessment_bytes).unwrap())
    );
}

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
        generated_plan,
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
        generated_evidence,
        json::canonical(&json::parse(&committed_evidence).unwrap())
    );

    let publication_plan_bytes = fs::read(examples.join("publication-plan.json")).unwrap();
    let publication_plan = publication::parse_plan(&publication_plan_bytes).unwrap();
    assert_eq!(
        publication::plan(&publication_plan.payload).unwrap(),
        json::canonical(&json::parse(&publication_plan_bytes).unwrap())
    );

    let publication_evidence_bytes = fs::read(examples.join("publication-evidence.json")).unwrap();
    let publication_evidence = publication::parse_evidence(&publication_evidence_bytes).unwrap();
    assert_eq!(
        publication::evidence(&publication_evidence.payload).unwrap(),
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
        replayed,
        json::canonical(&json::parse(&publication_assessment_bytes).unwrap())
    );

    let locale_plan_bytes = fs::read(examples.join("locale-coverage-plan.json")).unwrap();
    let locale_plan = locale::parse_plan(&locale_plan_bytes).unwrap();
    assert_eq!(
        locale::plan(&locale_plan.payload).unwrap(),
        json::canonical(&json::parse(&locale_plan_bytes).unwrap())
    );

    let locale_evidence_bytes = fs::read(examples.join("locale-coverage-evidence.json")).unwrap();
    let locale_evidence = locale::parse_evidence(&locale_evidence_bytes).unwrap();
    assert_eq!(
        locale::evidence(&locale_evidence.payload).unwrap(),
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
        replayed,
        json::canonical(&json::parse(&locale_assessment_bytes).unwrap())
    );
}

#[test]
fn semantic_examples_match_the_actual_typed_producers() {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/examples");
    let record_input_bytes = fs::read(examples.join("scanner-record-set-input.json")).unwrap();
    let record_input = semantic::record::parse_input(&record_input_bytes).unwrap();
    assert_eq!(
        serde_json_canonicalizer::to_vec(&record_input).unwrap(),
        json::canonical(&json::parse(&record_input_bytes).unwrap())
    );

    let semantic_evidence_bytes =
        fs::read(examples.join("scanner-semantic-evidence.json")).unwrap();
    semantic::parse(&semantic_evidence_bytes).unwrap();
    let typed: semantic::SemanticEvidenceEnvelope<semantic::observation::SiteBuildObservation> =
        serde_json::from_slice(&semantic_evidence_bytes).unwrap();
    assert_eq!(
        semantic::envelope(typed.payload).unwrap(),
        json::canonical(&json::parse(&semantic_evidence_bytes).unwrap())
    );

    let semantic_template_bytes =
        fs::read(examples.join("scanner-semantic-template.json")).unwrap();
    let semantic_template: semantic::SemanticEvidenceTemplate<semantic::record::Observation> =
        serde_json::from_slice(&semantic_template_bytes).unwrap();
    let generated_template = semantic::template(semantic_template).unwrap();
    assert_eq!(
        generated_template,
        json::canonical(&json::parse(&semantic_template_bytes).unwrap())
    );
    assert_eq!(
        semantic::record::template(record_input).unwrap(),
        generated_template
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
fn control_examples_match_their_typed_sources() {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/examples");
    let policy_bytes = fs::read(examples.join("scanner-policy.json")).unwrap();
    let policy = controls::parse_scanner_policy(&policy_bytes).unwrap();
    assert_eq!(
        controls::canonical_scanner_policy(&policy).unwrap().0,
        json::canonical(&json::parse(&policy_bytes).unwrap())
    );

    let floor_bytes = fs::read(examples.join("organization-floor.json")).unwrap();
    let floor = controls::parse_organization_floor(&floor_bytes).unwrap();
    assert_eq!(
        controls::canonical_organization_floor(&floor).unwrap().0,
        json::canonical(&json::parse(&floor_bytes).unwrap())
    );

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
            serde_json::to_vec(&identity).unwrap(),
            json::canonical(&json::parse(&bytes).unwrap())
        );
    }
}
