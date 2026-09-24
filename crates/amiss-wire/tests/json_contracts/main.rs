use amiss_wire::controls::DebtSnapshot;
use amiss_wire::controls::ExecutionConstraintDescriptor;
use amiss_wire::controls::OrganizationFloor;
use amiss_wire::controls::ScannerPolicy;
use amiss_wire::controls::TrustedTimeStatement;
use amiss_wire::controls::WaiverBundle;
use amiss_wire::de::Document as _;
use amiss_wire::envelope::Envelope;
use amiss_wire::repo_path_text;
use amiss_wire::semantic::SemanticEvidence;
use amiss_wire::semantic::record::Input;
use std::{collections::BTreeSet, fs, path::Path};

use amiss_wire::envelope::Payload as _;
use amiss_wire::locale::{LocaleCoverageAssessment, LocaleCoverageEvidence, LocaleCoveragePlan};
use amiss_wire::publication::{PublicationAssessment, PublicationEvidence, PublicationPlan};
use amiss_wire::{locale, publication, report, requests, semantic};
use strum::IntoEnumIterator;

use super::relation_fixture;

mod control_inputs;
mod control_tags;
mod external_evidence;
mod external_reader;
mod policy_presence;
mod report_controls;
mod report_counts;
mod report_details;
mod report_findings;
mod report_identity;
mod report_ingress;
mod report_keys;
mod report_projections;
mod report_reader;
mod report_resolutions;
mod report_rows;
mod semantic_observations;
mod semantic_producers;
mod semantic_reader;
mod semantic_requests;
mod string_tags;
mod trusted_time;

#[test]
fn document_row_enums_match_the_report_schema() {
    let schema: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../../spec/scanner-report.schema.json"
    ))
    .unwrap();
    let declared = |definition: &serde_json::Value| -> BTreeSet<String> {
        definition["enum"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap().to_owned())
            .collect()
    };
    let classifications: BTreeSet<_> = report::model::DocumentClassification::iter()
        .map(|classification| {
            let value = serde_json::to_value(classification).unwrap();
            assert_eq!(value.as_str(), Some(classification.as_ref()));
            classification.as_ref().to_owned()
        })
        .collect();
    assert_eq!(
        declared(&schema["$defs"]["DocumentResult"]["properties"]["classification"]),
        classifications
    );
    let described = schema["$defs"]["UnsupportedReason"]["description"]
        .as_str()
        .unwrap();
    for reason in [
        report::model::UnsupportedReason::SymlinkDocument,
        report::model::UnsupportedReason::GitlinkDocument,
        report::model::UnsupportedReason::LfsPointer,
        report::model::UnsupportedReason::UnsupportedDocumentFormat,
        report::model::UnsupportedReason::UndecodableDocument,
        report::model::UnsupportedReason::ResourceCeilingCrossed,
    ] {
        let spelling = serde_json::to_value(&reason).unwrap();
        let spelling = spelling.as_str().unwrap();
        assert!(
            described.contains(spelling),
            "the schema names every reason this engine writes, and it does not name {spelling}"
        );
    }
}

#[test]
fn resolver_reasons_fill_report_rows_without_changing_the_contract() {
    use amiss_wire::model::RepoPath;
    use amiss_wire::report::model::Resolution;
    use amiss_wire::resolution::{ExternalReference, InvalidReference, UnsupportedTargetTag};

    let schema: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../../spec/scanner-report.schema.json"
    ))
    .unwrap();
    let path = RepoPath::from(&repo_path_text!("docs/target.md"));
    for (definition, rows) in [
        (
            "InvalidResolution",
            InvalidReference::iter()
                .map(|reason| (reason.as_ref().to_owned(), Resolution::Invalid { reason }))
                .collect::<Vec<_>>(),
        ),
        (
            "ExternalResolution",
            ExternalReference::iter()
                .map(|reason| (reason.as_ref().to_owned(), Resolution::External { reason }))
                .collect(),
        ),
        (
            "UnsupportedTargetResolution",
            UnsupportedTargetTag::iter()
                .map(|reason| {
                    (
                        reason.as_ref().to_owned(),
                        Resolution::UnsupportedTarget {
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
        let value = serde_json::from_slice::<serde_json::Value>(&bytes).unwrap();
        let envelope = <report::model::ReportPayload>::parse(&bytes)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(
            serde_json_canonicalizer::to_vec(&envelope).unwrap(),
            serde_json_canonicalizer::to_vec(&value).unwrap(),
            "{name}",
        );
    }

    let frozen = fs::read(examples.join("scanner-report.frozen-3.json")).unwrap();
    <report::model::ReportPayload>::parse(&frozen)
        .unwrap_or_else(|error| panic!("frozen-3: {error}"));

    // The last released example keeps the previous major until the release refreshes it.
    let released = fs::read(examples.join("scanner-report.last-released.json")).unwrap();
    let released_value: serde_json::Value = serde_json::from_slice(&released).unwrap();
    if released_value
        .pointer("/payload/compatibility")
        .and_then(serde_json::Value::as_str)
        == Some(report::COMPATIBILITY)
    {
        <report::model::ReportPayload>::parse(&released)
            .unwrap_or_else(|error| panic!("last-released: {error}"));
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
            serde_json_canonicalizer::to_vec(
                &serde_json::from_slice::<serde_json::Value>(document.as_bytes()).unwrap()
            )
            .unwrap(),
        );
    }
}

#[test]
fn sidecar_examples_match_their_typed_sources() {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/examples");
    let contract = relation_fixture::relation_contract();
    let publication_plan =
        PublicationPlan::parse(&fs::read(examples.join("publication-plan.json")).unwrap()).unwrap();
    let publication_evidence =
        PublicationEvidence::parse(&fs::read(examples.join("publication-evidence.json")).unwrap())
            .unwrap();
    let publication_assessment = PublicationAssessment::parse(
        &fs::read(examples.join("publication-assessment.json")).unwrap(),
    )
    .unwrap();
    let locale_plan =
        LocaleCoveragePlan::parse(&fs::read(examples.join("locale-coverage-plan.json")).unwrap())
            .unwrap();
    let locale_evidence = LocaleCoverageEvidence::parse(
        &fs::read(examples.join("locale-coverage-evidence.json")).unwrap(),
    )
    .unwrap();
    let locale_assessment = LocaleCoverageAssessment::parse(
        &fs::read(examples.join("locale-coverage-assessment.json")).unwrap(),
    )
    .unwrap();
    for (name, generated) in [
        ("relation-plan.json", contract.plan.emit().unwrap()),
        ("relation-evidence.json", contract.evidence.emit().unwrap()),
        (
            "publication-plan.json",
            publication_plan.payload.emit().unwrap(),
        ),
        (
            "publication-evidence.json",
            publication_evidence.payload.emit().unwrap(),
        ),
        (
            "publication-assessment.json",
            publication::assess(
                &publication_plan,
                Some(&publication_evidence),
                &publication_assessment.payload.engine.engine_version,
                publication_assessment.payload.engine.engine_digest,
            )
            .unwrap(),
        ),
        (
            "locale-coverage-plan.json",
            locale_plan.payload.emit().unwrap(),
        ),
        (
            "locale-coverage-evidence.json",
            locale_evidence.payload.emit().unwrap(),
        ),
        (
            "locale-coverage-assessment.json",
            locale::assess(
                &locale_plan,
                Some(&locale_evidence),
                &locale_assessment.payload.engine.engine_version,
                locale_assessment.payload.engine.engine_digest,
            )
            .unwrap(),
        ),
    ] {
        let committed = fs::read(examples.join(name)).unwrap();
        let mut deserializer = serde_json::Deserializer::from_slice(&committed);
        let canonical =
            serde_json_canonicalizer::to_vec(&serde_transcode::Transcoder::new(&mut deserializer))
                .unwrap();
        assert_eq!(generated, canonical, "{name}");
    }
}

#[test]
fn semantic_examples_match_the_actual_typed_producers() {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/examples");
    let record_input_bytes = fs::read(examples.join("scanner-record-set-input.json")).unwrap();
    let record_input = Input::parse(&record_input_bytes).unwrap();
    assert_eq!(
        serde_json_canonicalizer::to_vec(&record_input).unwrap(),
        serde_json_canonicalizer::to_vec(
            &serde_json::from_slice::<serde_json::Value>(&record_input_bytes).unwrap()
        )
        .unwrap()
    );

    let semantic_evidence_bytes =
        fs::read(examples.join("scanner-semantic-evidence.json")).unwrap();
    SemanticEvidence::parse(&semantic_evidence_bytes).unwrap();
    let typed: Envelope<SemanticEvidence<'static>> =
        serde_json::from_slice(&semantic_evidence_bytes).unwrap();
    let generated = semantic::envelope(typed.payload.clone()).unwrap();
    let mut canonical = Vec::new();
    serde_json_canonicalizer::to_writer(&generated, &mut canonical).unwrap();
    assert_eq!(generated, typed);
    assert_eq!(
        canonical,
        serde_json_canonicalizer::to_vec(
            &serde_json::from_slice::<serde_json::Value>(&semantic_evidence_bytes).unwrap()
        )
        .unwrap()
    );

    let semantic_template_bytes =
        fs::read(examples.join("scanner-semantic-template.json")).unwrap();
    let semantic_template: semantic::SemanticEvidenceTemplate<'static> =
        serde_json::from_slice(&semantic_template_bytes).unwrap();
    let generated_template = semantic::template(semantic_template).unwrap();
    assert_eq!(
        generated_template,
        serde_json_canonicalizer::to_vec(
            &serde_json::from_slice::<serde_json::Value>(&semantic_template_bytes).unwrap()
        )
        .unwrap()
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
        serde_json_canonicalizer::to_vec(
            &serde_json::from_slice::<serde_json::Value>(&evaluation_bytes).unwrap()
        )
        .unwrap()
    );
    let snapshot_bytes = fs::read(examples.join("scanner-snapshot-request.json")).unwrap();
    let snapshot = serde_json::from_slice::<requests::SnapshotRequest>(&snapshot_bytes).unwrap();
    assert_eq!(
        serde_json_canonicalizer::to_vec(&snapshot).unwrap(),
        serde_json_canonicalizer::to_vec(
            &serde_json::from_slice::<serde_json::Value>(&snapshot_bytes).unwrap()
        )
        .unwrap()
    );
    let bytes = fs::read(examples.join("scanner-controls-request.json")).unwrap();
    let request = requests::ControlsRequest::parse(&bytes).unwrap();
    assert_eq!(
        serde_json_canonicalizer::to_vec(&request).unwrap(),
        serde_json_canonicalizer::to_vec(
            &serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()
        )
        .unwrap()
    );
    let time_bytes = fs::read(examples.join("scanner-trusted-time-statement.json")).unwrap();
    let statement = TrustedTimeStatement::parse(&time_bytes).unwrap();
    assert_eq!(
        serde_json_canonicalizer::to_vec(&statement).unwrap(),
        serde_json_canonicalizer::to_vec(
            &serde_json::from_slice::<serde_json::Value>(&time_bytes).unwrap()
        )
        .unwrap()
    );
    let constraint_bytes = fs::read(examples.join("scanner-execution-constraint.json")).unwrap();
    let constraint = ExecutionConstraintDescriptor::parse(&constraint_bytes).unwrap();
    assert_eq!(
        serde_json_canonicalizer::to_vec(&constraint).unwrap(),
        serde_json_canonicalizer::to_vec(
            &serde_json::from_slice::<serde_json::Value>(&constraint_bytes).unwrap()
        )
        .unwrap()
    );
}

#[test]
fn control_examples_match_their_typed_sources() {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/examples");
    let policy_bytes = fs::read(examples.join("scanner-policy.json")).unwrap();
    let policy = ScannerPolicy::parse(&policy_bytes).unwrap();
    assert_eq!(
        serde_json_canonicalizer::to_vec(&policy).unwrap(),
        serde_json_canonicalizer::to_vec(
            &serde_json::from_slice::<serde_json::Value>(&policy_bytes).unwrap()
        )
        .unwrap()
    );

    let floor_bytes = fs::read(examples.join("organization-floor.json")).unwrap();
    let floor = OrganizationFloor::parse(&floor_bytes).unwrap();
    assert_eq!(
        serde_json_canonicalizer::to_vec(&floor).unwrap(),
        serde_json_canonicalizer::to_vec(
            &serde_json::from_slice::<serde_json::Value>(&floor_bytes).unwrap()
        )
        .unwrap()
    );

    let debt_bytes = fs::read(examples.join("debt-snapshot.json")).unwrap();
    let debt = DebtSnapshot::parse(&debt_bytes).unwrap();
    assert_eq!(
        serde_json_canonicalizer::to_vec(&debt).unwrap(),
        serde_json_canonicalizer::to_vec(
            &serde_json::from_slice::<serde_json::Value>(&debt_bytes).unwrap()
        )
        .unwrap()
    );

    let waiver_bytes = fs::read(examples.join("waiver-bundle.json")).unwrap();
    let waiver = WaiverBundle::parse(&waiver_bytes).unwrap();
    assert_eq!(
        serde_json_canonicalizer::to_vec(&waiver).unwrap(),
        serde_json_canonicalizer::to_vec(
            &serde_json::from_slice::<serde_json::Value>(&waiver_bytes).unwrap()
        )
        .unwrap()
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
            serde_json_canonicalizer::to_vec(
                &serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()
            )
            .unwrap()
        );
    }
}
