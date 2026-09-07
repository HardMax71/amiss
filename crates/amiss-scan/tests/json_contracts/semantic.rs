use std::borrow::Cow;

use amiss_fixtures::{SiteObservation, site_observation};
use amiss_git::Repository;
use amiss_scan::{SetupShell, pipeline::commit_pair, report::RequestDigests, semantic::Input};
use amiss_wire::{
    assessment::Nullable,
    controls::Profile,
    digest::hb,
    model::{ObjectFormat, Oid},
    report::{AnalysisErrorCode, EngineProvenance, FindingKind},
    requests::{
        ControlsRequest, EvaluationRequest, SuppliedSemanticEvidence,
        commit_candidate_identity_digest,
    },
    semantic::{
        self, SemanticEvidenceEnvelope, SemanticEvidenceTemplate, SemanticProducer,
        SemanticProducerKind, TemplateSchema, bind_template,
        observation::{Observation, SiteBuildObservation, SphinxLabelKind, SphinxLabelObservation},
        record,
    },
};

#[derive(serde::Serialize)]
struct ExtendedObservation<'a> {
    #[serde(flatten)]
    observation: &'a Observation,
    unexpected: bool,
}

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
            kind: SemanticProducerKind::SiteBuild,
            identity: "fixture".parse().unwrap(),
            version: semantic::observation::SITE_BUILD_VERSION.to_owned(),
            context_digest: hb("test", b"context"),
            input_digest: hb("test", b"input"),
        },
        complete: true,
        observations: vec![
            site_observation(
                "/broken/",
                SiteObservation::Redirect("README.md", "/absent/"),
            )
            .unwrap(),
            site_observation("/generated/", SiteObservation::Generated(None, &["intro"])).unwrap(),
        ]
        .into_iter()
        .map(Cow::Owned)
        .collect(),
    };
    let (envelope, bytes) = bind_template(&template, identity).unwrap();
    let request = ControlsRequest {
        semantic_evidence: vec![SuppliedSemanticEvidence {
            value: serde_json::from_slice(&bytes).unwrap(),
            expected_context_digest: template.producer.context_digest,
        }],
        ..ControlsRequest::default()
    };
    let inputs = amiss_scan::request::controls(request).unwrap();
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

#[test]
fn semantic_consumers_refuse_unknown_or_foreign_observations_with_correct_digests() {
    let original: SemanticEvidenceEnvelope<'static> = serde_json::from_slice(include_bytes!(
        "../../../../spec/examples/scanner-semantic-evidence.json"
    ))
    .unwrap();
    let cases = [
        (
            SemanticProducerKind::SiteBuild,
            semantic::observation::SITE_BUILD_VERSION,
            Observation::Site(SiteBuildObservation::GeneratedRoute {
                route: "/index".to_owned(),
                source: Nullable::Null,
                anchors: Vec::new(),
            }),
        ),
        (
            SemanticProducerKind::SphinxInventorySet,
            semantic::observation::SPHINX_INVENTORY_VERSION,
            Observation::Sphinx(SphinxLabelObservation {
                kind: SphinxLabelKind::Current,
                inventory: "python".parse().unwrap(),
                name: "context managers".to_owned(),
                destination: "https://docs.python.org/reference/datamodel.html".to_owned(),
            }),
        ),
        (
            SemanticProducerKind::RecordSet,
            record::PRODUCER_VERSION,
            Observation::Record(record::Observation {
                kind: record::ObservationKind::Current,
                name: "rust/api".parse().unwrap(),
                records: Vec::new(),
            }),
        ),
    ];
    for (kind, version, observation) in &cases {
        let mut payload = original.payload.clone();
        payload.producer.kind = *kind;
        payload.producer.version = (*version).to_owned();
        payload.subject.source_report_payload_digest = Nullable::Null;
        payload.observations = vec![Cow::Owned(observation.clone())];
        let (document, bytes) = semantic::envelope(payload).unwrap();
        let request = ControlsRequest {
            semantic_evidence: vec![SuppliedSemanticEvidence {
                value: serde_json::from_slice(&bytes).unwrap(),
                expected_context_digest: document.payload.producer.context_digest,
            }],
            ..ControlsRequest::default()
        };
        let request_bytes = String::from_utf8(request.canonical_bytes().unwrap()).unwrap();
        assert!(amiss_scan::request::controls(request).is_ok(), "{kind}");
        let extended = ExtendedObservation {
            observation,
            unexpected: true,
        };
        let observation =
            String::from_utf8(serde_json_canonicalizer::to_vec(observation).unwrap()).unwrap();
        let payload =
            String::from_utf8(serde_json_canonicalizer::to_vec(&document.payload).unwrap())
                .unwrap();
        let envelope = String::from_utf8(bytes).unwrap();
        let mut invalids = vec![
            (br#"{"kind":"future-fact"}"#.to_vec(), false),
            (serde_json_canonicalizer::to_vec(&extended).unwrap(), false),
        ];
        invalids.extend(
            cases
                .iter()
                .filter(|(other_kind, _, _)| other_kind != kind)
                .map(|(_, _, other)| (serde_json_canonicalizer::to_vec(other).unwrap(), true)),
        );
        for (invalid, typed) in invalids {
            let invalid = String::from_utf8(invalid).unwrap();
            let changed = payload.replace(&observation, &invalid);
            assert_ne!(payload, changed);
            let digest = hb(semantic::PAYLOAD_SCHEMA, changed.as_bytes());
            let encoded = envelope
                .replace(&payload, &changed)
                .replace(&document.payload_digest.to_string(), &digest.to_string());
            let altered = request_bytes.replace(&envelope, &encoded);
            assert_ne!(altered, request_bytes);
            let parsed = ControlsRequest::parse(altered.as_bytes());
            if typed {
                assert_eq!(
                    amiss_scan::request::controls(parsed.unwrap())
                        .unwrap_err()
                        .code,
                    AnalysisErrorCode::ConfigurationInvalid,
                    "{kind}: {invalid}"
                );
            } else {
                assert!(parsed.is_err(), "{kind}: {invalid}");
            }
        }
    }
}
