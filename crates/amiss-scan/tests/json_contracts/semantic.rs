use amiss_fixtures::{SiteObservation, site_observation};
use amiss_git::Repository;
use amiss_scan::{SetupShell, pipeline::commit_pair, report::RequestDigests, semantic::Input};
use amiss_wire::{
    controls::Profile,
    digest::hb,
    model::{ObjectFormat, Oid},
    report::{EngineProvenance, FindingKind},
    requests::{
        ControlsRequest, EvaluationRequest, SuppliedSemanticEvidence,
        commit_candidate_identity_digest,
    },
    semantic::{SemanticEvidenceTemplate, SemanticProducer, TemplateSchema, bind_template},
};

#[test]
fn template_and_captured_evidence_produce_identical_scanner_reports() {
    let fixture = amiss_fixtures::commit_pair(
        &[("README.md", "# Readme\n")],
        &[("README.md", "# Readme\n\nChanged.\n")],
    )
    .unwrap();
    let repo = Repository::open(std::path::Path::new(&fixture.repo), ObjectFormat::Sha1).unwrap();
    let base = Oid::new(ObjectFormat::Sha1, fixture.base).unwrap();
    let candidate = Oid::new(ObjectFormat::Sha1, fixture.candidate).unwrap();
    let identity = commit_candidate_identity_digest(
        &EvaluationRequest::commit_pair(
            Profile::Observe,
            ObjectFormat::Sha1,
            base.clone(),
            candidate.clone(),
        ),
        &Oid::new(ObjectFormat::Sha1, fixture.base_tree).unwrap(),
        &Oid::new(ObjectFormat::Sha1, fixture.candidate_tree).unwrap(),
    )
    .unwrap();
    let template = SemanticEvidenceTemplate {
        schema: TemplateSchema::Current,
        producer: SemanticProducer {
            kind: amiss_wire::semantic::SemanticProducerKind::SiteBuild,
            identity: "fixture".parse().unwrap(),
            version: amiss_wire::semantic::observation::SITE_BUILD_VERSION.to_owned(),
            context_digest: hb("test", b"context"),
            input_digest: hb("test", b"input"),
        },
        complete: true,
        observations: vec![
            site_observation(
                "/broken/",
                SiteObservation::Redirect("README.md", "/absent/"),
            ),
            serde_json::json!({"kind": "future-fact", "extra": {"é": [null, true, "\"\n"]}}),
        ]
        .into(),
    };
    let (envelope, bytes) = bind_template(&template, identity).unwrap();
    let request = ControlsRequest {
        semantic_evidence: vec![SuppliedSemanticEvidence {
            value: serde_json::from_slice(&bytes).unwrap(),
            expected_context_digest: template.producer.context_digest,
        }],
        ..ControlsRequest::default()
    };
    let inputs = amiss_scan::request::controls(&request).unwrap();
    let expected_digest = envelope.payload_digest;
    let engine = EngineProvenance {
        version: "test".to_owned(),
        digest: hb("test", b"engine"),
    };
    let mut reports = Vec::new();
    for semantic in [Input::Template(template), Input::Bound(inputs.semantic)] {
        let setup = SetupShell {
            engine: engine.clone(),
            profile: Profile::Observe,
            repository: None,
            forge: None,
            candidate_ref: None,
            target_ref: None,
            default_branch_ref: None,
            floor: None,
            debt: None,
            waiver: None,
            time: None,
            constraint: None,
            semantic,
            requests: RequestDigests::default(),
            external_defect: None,
            errors_retained: 64,
        };
        let built = commit_pair(&repo, &engine, None, &setup, &base, &candidate).unwrap();
        let bytes = amiss_scan::report::wire(&built).unwrap();
        let (payload, _, _) = amiss_wire::report::validate_envelope(&bytes).unwrap();
        assert!(payload.errors.is_empty(), "{:?}", payload.errors);
        let amiss_wire::report::model::Controls::Resolved(controls) = payload.controls else {
            panic!("controls must resolve");
        };
        assert_eq!(
            controls.semantic_evidence.unwrap()[0].payload_digest,
            expected_digest
        );
        assert!(
            payload
                .findings
                .iter()
                .any(|finding| finding.kind == FindingKind::SiteBuildDefect)
        );
        reports.push(bytes);
    }
    assert_eq!(reports[0], reports[1]);
}
