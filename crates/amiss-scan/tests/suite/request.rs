#![expect(
    clippy::expect_used,
    reason = "integration assertions over the external-control request gate"
)]

use std::borrow::Cow;

use amiss_fixtures::{SiteObservation, site_observation};
use amiss_scan::request::controls;
use amiss_wire::assessment::Nullable;
use amiss_wire::digest::{Digest, hb};
use amiss_wire::model::ArtifactId;
use amiss_wire::report::AnalysisErrorCode;
use amiss_wire::requests::{
    ControlsRequest, ControlsRequestSchema, RequestTrust, SuppliedControl,
    SuppliedSemanticEvidence, SuppliedTime,
};
use amiss_wire::semantic::{
    PayloadSchema, SemanticEvidence, SemanticProducer, SemanticProducerKind, SemanticSubject,
    observation::{Observation, SiteBuildObservation, SphinxLabelKind, SphinxLabelObservation},
    record,
};

const FLOOR: &str = r#"{
  "schema": "amiss/organization-floor",
  "floor_id": "acme/scanner-floor-2026-07",
  "repository": { "host": "github.com", "owner": "acme", "name": "docs" },
  "ref": "refs/heads/main",
  "minimum_profile": "observe",
  "minimum_dispositions": [],
  "protected_inventory": [],
  "protected_control_paths": [],
  "waivable_finding_kinds": [],
  "authorized_debt_owners": [],
  "authorized_waiver_issuers": [],
  "resource_limits": []
}"#;

const TIME: &str = r#"{
  "schema": "amiss/scanner-trusted-time-statement",
  "controller": "external-required-check-clock",
  "repository": { "host": "gitlab.com", "owner": "platform/security", "name": "docs" },
  "ref": "refs/heads/main",
  "candidate_identity_digest": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
  "provider": "gitlab-ci",
  "provider_run_id": "pipeline/01J2Z9-7",
  "provider_run_attempt": 2,
  "evaluation_instant": "2026-07-12T10:00:00Z",
  "valid_until": "2026-07-12T10:10:00Z"
}"#;

const CONSTRAINT: &str = r#"{
  "schema": "amiss/scanner-execution-constraint",
  "action_repository": { "host": "github.com", "owner": "acme", "name": "amiss-action" },
  "action_object_format": "sha1",
  "action_commit_oid": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "action_tree_oid": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
  "manifest_path": "release/manifest.json",
  "release_manifest_digest": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
  "selected_platform": "linux-x86_64",
  "required_status_name": "amiss / documentation assurance",
  "bootstrap_contract": "amiss-action-bootstrap",
  "bootstrap_digest": "sha256:3333333333333333333333333333333333333333333333333333333333333333"
}"#;

const fn empty() -> ControlsRequest {
    ControlsRequest {
        schema: ControlsRequestSchema::Current,
        organization_floor: None,
        debt_snapshot: None,
        waiver_bundle: None,
        trusted_time: None,
        execution_constraint: None,
        semantic_evidence: Vec::new(),
    }
}

fn supplied<T: serde::de::DeserializeOwned>(doc: &str, expected: Digest) -> SuppliedControl<T> {
    SuppliedControl {
        value: serde_json::from_str(doc).expect("the fixture is JSON"),
        expected_digest: expected,
        trust_source: RequestTrust::OrganizationPolicy,
    }
}

fn semantic_evidence(
    producer_kind: SemanticProducerKind,
    producer_version: &str,
    input_digest: Digest,
    source_report_payload_digest: Option<Digest>,
    observations: Vec<Observation>,
) -> SemanticEvidence<'static> {
    SemanticEvidence {
        schema: PayloadSchema::Current,
        subject: SemanticSubject {
            candidate_identity_digest: hb("test/candidate", b"candidate"),
            source_report_payload_digest: source_report_payload_digest
                .map_or(Nullable::Null, Nullable::Value),
        },
        producer: SemanticProducer {
            kind: producer_kind,
            identity: ArtifactId::new("amiss-test".to_owned())
                .expect("the producer identity is valid"),
            version: producer_version.to_owned(),
            context_digest: input_digest,
            input_digest,
        },
        complete: true,
        observations: observations.into_iter().map(Cow::Owned).collect(),
    }
}

fn supplied_semantic(evidence: SemanticEvidence<'static>) -> SuppliedSemanticEvidence {
    let expected_context_digest = evidence.producer.context_digest;
    let (value, _bytes) = amiss_wire::semantic::envelope(evidence)
        .expect("the envelope contains known observation models");
    SuppliedSemanticEvidence {
        value,
        expected_context_digest,
    }
}

#[test]
fn a_verified_floor_lands_typed() {
    let floor =
        amiss_wire::controls::parse_organization_floor(FLOOR.as_bytes()).expect("fixture parses");
    let digest = amiss_wire::controls::canonical_organization_floor(&floor)
        .unwrap()
        .1;
    let mut request = empty();
    request.organization_floor = Some(supplied(FLOOR, digest));
    let original_allocation = request
        .organization_floor
        .as_ref()
        .unwrap()
        .value
        .floor_id
        .as_str()
        .as_ptr();
    let inputs = controls(request).expect("a matching digest passes the gate");
    let landed = inputs.floor.expect("the floor lands typed");
    assert_eq!(landed.floor.floor_id.as_str().as_ptr(), original_allocation);
    assert_eq!(landed.floor, floor);
    assert_eq!(landed.digest, digest);
    assert!(inputs.time.is_none() && inputs.debt.is_none());
}

#[test]
fn a_wrong_floor_digest_is_refused() {
    let mut request = empty();
    request.organization_floor = Some(supplied(FLOOR, hb("test/other", b"not the floor")));
    let error = controls(request).expect_err("a foreign digest never passes");
    assert_eq!(error.code, AnalysisErrorCode::DigestMismatch);
}

#[test]
fn a_verified_time_statement_lands_with_its_run_context() {
    let statement =
        amiss_wire::controls::parse_trusted_time(TIME.as_bytes()).expect("fixture parses");
    let (_, digest) = amiss_wire::controls::canonical_trusted_time(&statement).unwrap();
    let mut request = empty();
    request.trusted_time = Some(SuppliedTime {
        value: statement.clone(),
        expected_digest: digest,
        provider: "gitlab-ci".to_owned(),
        provider_run_id: "pipeline/01J2Z9-7".to_owned(),
        provider_run_attempt: 2,
    });
    let provider_allocation = request.trusted_time.as_ref().unwrap().provider.as_ptr();
    let inputs = controls(request).expect("a matching digest passes the gate");
    let landed = inputs.time.expect("the statement lands typed");
    assert_eq!(landed.provider.as_ptr(), provider_allocation);
    assert_eq!(landed.statement, statement);
    assert_eq!(landed.provider, "gitlab-ci");
    assert_eq!(landed.provider_run_id, "pipeline/01J2Z9-7");
    assert_eq!(landed.provider_run_attempt, 2);
}

#[test]
fn a_wrong_time_digest_is_refused() {
    let mut request = empty();
    request.trusted_time = Some(SuppliedTime {
        value: serde_json::from_str(TIME).expect("the fixture is JSON"),
        expected_digest: hb("test/other", b"not the statement"),
        provider: "gitlab-ci".to_owned(),
        provider_run_id: "pipeline/01J2Z9-7".to_owned(),
        provider_run_attempt: 2,
    });
    let error = controls(request).expect_err("a foreign digest never passes");
    assert_eq!(error.code, AnalysisErrorCode::DigestMismatch);
}

#[test]
fn typed_time_still_requires_semantic_validation() {
    let valid = amiss_wire::controls::parse_trusted_time(TIME.as_bytes()).unwrap();
    let (_, expected_digest) = amiss_wire::controls::canonical_trusted_time(&valid).unwrap();
    for value in [
        amiss_wire::controls::TrustedTimeStatement {
            provider: "bad provider!".to_owned(),
            ..valid.clone()
        },
        amiss_wire::controls::TrustedTimeStatement {
            valid_until: valid.evaluation_instant.clone(),
            ..valid
        },
    ] {
        let request = ControlsRequest {
            trusted_time: Some(SuppliedTime {
                value,
                expected_digest,
                provider: "gitlab-ci".to_owned(),
                provider_run_id: "pipeline/01J2Z9-7".to_owned(),
                provider_run_attempt: 2,
            }),
            ..empty()
        };
        let error = controls(request).expect_err("typed fields do not establish semantic validity");
        assert_eq!(error.code, AnalysisErrorCode::ConfigurationInvalid);
    }
}

#[test]
fn a_verified_constraint_lands_through_the_shared_gate() {
    let descriptor = amiss_wire::controls::parse_execution_constraint(CONSTRAINT.as_bytes())
        .expect("fixture parses");
    let (_, digest) = amiss_wire::controls::canonical_execution_constraint(&descriptor).unwrap();
    let mut request = empty();
    request.execution_constraint = Some(supplied(CONSTRAINT, digest));
    let inputs = controls(request).expect("a matching digest passes the gate");
    let landed = inputs.constraint.expect("the descriptor lands typed");
    assert_eq!(landed.descriptor, descriptor);
}

#[test]
fn a_wrong_constraint_digest_is_refused() {
    let mut request = empty();
    request.execution_constraint = Some(supplied(CONSTRAINT, hb("test/other", b"not the plan")));
    let error = controls(request).expect_err("a foreign digest never passes");
    assert_eq!(error.code, AnalysisErrorCode::DigestMismatch);
}

#[test]
fn incomplete_or_invalid_inventory_evidence_never_becomes_input() {
    let valid = semantic_evidence(
        SemanticProducerKind::SphinxInventorySet,
        "1",
        hb("test/inventory", b"inventory"),
        None,
        vec![Observation::Sphinx(SphinxLabelObservation {
            kind: SphinxLabelKind::Current,
            inventory: "python".parse().unwrap(),
            name: "except_star".to_owned(),
            destination: "https://docs.python.org/3/reference/".to_owned(),
        })],
    );
    let mut incomplete = valid.clone();
    incomplete.complete = false;
    let mut unsupported = valid.clone();
    unsupported.producer.version = "2".to_owned();
    let mut malformed = valid;
    malformed.observations[0] = Cow::Owned(Observation::Sphinx(SphinxLabelObservation {
        kind: SphinxLabelKind::Current,
        inventory: "python".parse().unwrap(),
        name: "except_star".to_owned(),
        destination: "https:///missing-authority".to_owned(),
    }));

    for evidence in [incomplete, unsupported, malformed] {
        let mut request = empty();
        request.semantic_evidence = vec![supplied_semantic(evidence)];
        let error = controls(request).expect_err("the inventory consumer fails closed");
        assert_eq!(error.code, AnalysisErrorCode::ConfigurationInvalid);
    }
}

#[test]
fn record_sets_accept_complete_empty_and_partial_typed_rows() {
    for (complete, records) in [
        (true, Vec::new()),
        (true, vec![("amiss::check", "pub fn check()")]),
        (false, vec![("amiss::check", "pub fn check()")]),
    ] {
        let mut evidence = semantic_evidence(
            SemanticProducerKind::RecordSet,
            "1",
            hb("test/records", b"rust public api"),
            None,
            vec![Observation::Record(record::Observation {
                kind: record::ObservationKind::Current,
                name: "rust/public-api".parse().unwrap(),
                records: records
                    .iter()
                    .map(|(key, value)| record::Record {
                        key: (*key).to_owned(),
                        value: (*value).to_owned(),
                    })
                    .collect(),
            })],
        );
        evidence.complete = complete;
        let mut request = empty();
        request.semantic_evidence = vec![supplied_semantic(evidence)];
        controls(request).expect("one typed record set reaches the projection context");
    }
}

#[test]
fn malformed_record_sets_fail_closed() {
    let valid = semantic_evidence(
        SemanticProducerKind::RecordSet,
        "1",
        hb("test/records", b"rust public api"),
        None,
        vec![Observation::Record(record::Observation {
            kind: record::ObservationKind::Current,
            name: "rust/public-api".parse().unwrap(),
            records: vec![record::Record {
                key: "amiss::check".to_owned(),
                value: "pub fn check()".to_owned(),
            }],
        })],
    );
    let mut wrong_version = valid.clone();
    wrong_version.producer.version = "2".to_owned();
    let mut report_derived = valid.clone();
    report_derived.subject.source_report_payload_digest =
        Nullable::Value(hb("test/report", b"report"));
    let mut multiple_sets = valid.clone();
    multiple_sets
        .observations
        .push(Cow::Owned(Observation::Record(record::Observation {
            kind: record::ObservationKind::Current,
            name: "rust/other".parse().unwrap(),
            records: Vec::new(),
        })));
    let mut invalid = vec![wrong_version, report_derived, multiple_sets];
    for rows in [
        &[("b", "B"), ("a", "A")][..],
        &[("a", "A"), ("a", "B")][..],
        &[("a", "line\nbreak")][..],
        &[("a", "")][..],
    ] {
        let mut evidence = valid.clone();
        evidence.observations = vec![Cow::Owned(Observation::Record(record::Observation {
            kind: record::ObservationKind::Current,
            name: "rust/public-api".parse().unwrap(),
            records: rows
                .iter()
                .map(|(key, value)| record::Record {
                    key: (*key).to_owned(),
                    value: (*value).to_owned(),
                })
                .collect(),
        }))];
        invalid.push(evidence);
    }

    for evidence in invalid {
        let mut request = empty();
        request.semantic_evidence = vec![supplied_semantic(evidence)];
        let error = controls(request).expect_err("malformed record evidence stays unavailable");
        assert!(matches!(
            error.code,
            AnalysisErrorCode::ConfigurationInvalid | AnalysisErrorCode::NoncanonicalArray
        ));
    }
}

#[test]
fn two_envelopes_cannot_claim_the_same_record_set() {
    let mut supplied = [("left", "a", "A"), ("right", "b", "B")]
        .into_iter()
        .map(|(lane, key, value)| {
            let mut evidence = semantic_evidence(
                SemanticProducerKind::RecordSet,
                "1",
                hb("test/records", lane.as_bytes()),
                None,
                vec![Observation::Record(record::Observation {
                    kind: record::ObservationKind::Current,
                    name: "rust/public-api".parse().unwrap(),
                    records: vec![record::Record {
                        key: key.to_owned(),
                        value: value.to_owned(),
                    }],
                })],
            );
            evidence.producer.context_digest = hb("test/record-context", lane.as_bytes());
            supplied_semantic(evidence)
        })
        .collect::<Vec<_>>();
    supplied.sort_by_key(|item| {
        amiss_wire::semantic::parse(&serde_json::to_vec(&item.value).expect("envelope JSON"))
            .expect("the generic envelopes are valid")
            .payload_digest
    });
    let mut request = empty();
    request.semantic_evidence = supplied;
    let error = controls(request).expect_err("one set name has one evidence authority");
    assert_eq!(error.code, AnalysisErrorCode::NoncanonicalArray);
}

#[test]
fn semantic_evidence_must_match_the_independently_supplied_context() {
    let evidence = semantic_evidence(
        SemanticProducerKind::SphinxInventorySet,
        "1",
        hb("test/inventory", b"inventory"),
        None,
        Vec::new(),
    );
    let mut request = empty();
    let (_document, bytes) =
        amiss_wire::semantic::envelope(evidence).expect("the generic envelope is valid");
    request.semantic_evidence = vec![SuppliedSemanticEvidence {
        value: serde_json::from_slice(&bytes).expect("the envelope is JSON"),
        expected_context_digest: hb("test/inventory", b"another inventory"),
    }];

    let error = controls(request).expect_err("a foreign context never reaches a consumer");
    assert_eq!(error.code, AnalysisErrorCode::DigestMismatch);
}

#[test]
fn incomplete_or_invalid_site_build_evidence_never_becomes_input() {
    let valid = semantic_evidence(
        SemanticProducerKind::SiteBuild,
        "0.5.1",
        hb("test/site-output", b"site output"),
        Some(hb("test/report", b"source report")),
        vec![
            site_observation(
                "/guide/",
                SiteObservation::Page("docs/guide.md", &["details", "intro"]),
            )
            .unwrap(),
            Observation::Site(SiteBuildObservation::Navigation {
                root: Nullable::Value("docs".parse().unwrap()),
                manifest: "docs/SUMMARY.md".parse().unwrap(),
                entrypoints: vec!["/guide/".to_owned()],
                reachable: vec!["docs/guide.md".parse().unwrap()],
            }),
        ],
    );
    let mut incomplete = valid.clone();
    incomplete.complete = false;
    let mut unsupported = valid.clone();
    unsupported.producer.version = "0.2.0".to_owned();
    let mut invalid_route = valid.clone();
    invalid_route.observations = vec![Cow::Owned(
        site_observation(
            "//other.example/guide",
            SiteObservation::Page("docs/guide.md", &["intro"]),
        )
        .unwrap(),
    )];
    let mut unsorted_anchors = valid.clone();
    unsorted_anchors.observations = vec![Cow::Owned(
        site_observation(
            "/guide/",
            SiteObservation::Page("docs/guide.md", &["intro", "details"]),
        )
        .unwrap(),
    )];
    let mut duplicate_anchors = valid;
    duplicate_anchors.observations = vec![Cow::Owned(
        site_observation(
            "/guide/",
            SiteObservation::Page("docs/guide.md", &["intro", "intro"]),
        )
        .unwrap(),
    )];
    let malformed_fragment_redirect = semantic_evidence(
        SemanticProducerKind::SiteBuild,
        "0.5.1",
        hb("test/site-output", b"site output"),
        Some(hb("test/report", b"source report")),
        vec![
            site_observation(
                "/legacy/",
                SiteObservation::Redirect("docs/redirects.toml", "/guide/#bad%fragment"),
            )
            .unwrap(),
        ],
    );
    for destination in [
        "/guide/?language=en",
        "//other.example/guide/",
        "/legacy/#intro",
    ] {
        let mut invalid = malformed_fragment_redirect.clone();
        invalid.observations = vec![Cow::Owned(
            site_observation(
                "/legacy/",
                SiteObservation::Redirect("docs/redirects.toml", destination),
            )
            .unwrap(),
        )];
        let mut request = empty();
        request.semantic_evidence = vec![supplied_semantic(invalid)];
        assert_eq!(
            controls(request).unwrap_err().code,
            AnalysisErrorCode::ConfigurationInvalid
        );
    }
    let mut accepted = empty();
    accepted.semantic_evidence = vec![supplied_semantic(malformed_fragment_redirect)];
    controls(accepted).expect("a literal percent sign remains a valid redirect fragment");

    for (evidence, expected) in [
        (incomplete, AnalysisErrorCode::ConfigurationInvalid),
        (unsupported, AnalysisErrorCode::ConfigurationInvalid),
        (invalid_route, AnalysisErrorCode::ConfigurationInvalid),
        (unsorted_anchors, AnalysisErrorCode::NoncanonicalArray),
        (duplicate_anchors, AnalysisErrorCode::NoncanonicalArray),
    ] {
        let mut request = empty();
        request.semantic_evidence = vec![supplied_semantic(evidence)];
        let error = controls(request).expect_err("the site-build consumer fails closed");
        assert_eq!(error.code, expected);
    }
}

#[test]
fn site_claims_require_valid_explicit_source_attribution() {
    for observation in [
        r#"{"destination":"/guide/","kind":"site-redirect","route":"/legacy/"}"#,
        r#"{"anchors":[],"kind":"site-route","route":"/guide/","source":null}"#,
        r#"{"destination":"/guide/","kind":"site-redirect","route":"/legacy/","source":null}"#,
        r#"{"anchors":[],"kind":"site-generated-route","route":"/generated/"}"#,
        r#"{"anchors":["intro"],"kind":"site-route","route":"/guide/","source":"../guide.md"}"#,
    ] {
        let evidence = semantic_evidence(
            SemanticProducerKind::SiteBuild,
            "0.5.1",
            hb("test/site-output", b"site output"),
            Some(hb("test/report", b"source report")),
            Vec::new(),
        );
        let mut request = empty();
        request.semantic_evidence = vec![supplied_semantic(evidence.clone())];
        let original = serde_json::to_string(&request.semantic_evidence[0].value).unwrap();
        let encoded = serde_json::to_string(&request).unwrap();
        let payload = String::from_utf8(serde_json_canonicalizer::to_vec(&evidence).unwrap())
            .unwrap()
            .replace(
                r#""observations":[]"#,
                &format!(r#""observations":[{observation}]"#),
            );
        let digest = hb(amiss_wire::semantic::PAYLOAD_SCHEMA, payload.as_bytes());
        let envelope = format!(
            r#"{{"schema":"amiss/semantic-evidence-envelope","payload":{payload},"payload_digest":"{digest}"}}"#
        );
        let malformed = encoded.replace(&original, &envelope);
        assert_ne!(malformed, encoded);
        assert!(
            ControlsRequest::parse(malformed.as_bytes()).is_err(),
            "{observation}"
        );
    }
}

#[test]
fn generated_site_claims_admit_absent_repository_attribution() {
    let evidence = semantic_evidence(
        SemanticProducerKind::SiteBuild,
        "0.5.1",
        hb("test/site-output", b"site output"),
        Some(hb("test/report", b"source report")),
        vec![
            site_observation("/generated/", SiteObservation::Generated(None, &["intro"])).unwrap(),
            Observation::Site(SiteBuildObservation::Navigation {
                root: Nullable::Value("docs".parse().unwrap()),
                manifest: "docs/SUMMARY.md".parse().unwrap(),
                entrypoints: vec!["/generated/".to_owned()],
                reachable: vec![],
            }),
        ],
    );
    let mut request = empty();
    request.semantic_evidence = vec![supplied_semantic(evidence)];

    controls(request).expect("explicit null attribution is a valid generated-page claim");
}

#[test]
fn inconsistent_site_navigation_never_becomes_input() {
    let page = site_observation(
        "/guide/",
        SiteObservation::Page("docs/guide.md", &["intro"]),
    )
    .unwrap();
    let cases = [
        Observation::Site(SiteBuildObservation::Navigation {
            root: Nullable::Value("docs".parse().unwrap()),
            manifest: "other/SUMMARY.md".parse().unwrap(),
            entrypoints: vec!["/guide/".to_owned()],
            reachable: vec!["docs/guide.md".parse().unwrap()],
        }),
        Observation::Site(SiteBuildObservation::Navigation {
            root: Nullable::Value("docs".parse().unwrap()),
            manifest: "docs/SUMMARY.md".parse().unwrap(),
            entrypoints: vec!["/missing/".to_owned()],
            reachable: vec!["docs/guide.md".parse().unwrap()],
        }),
        Observation::Site(SiteBuildObservation::Navigation {
            root: Nullable::Value("docs".parse().unwrap()),
            manifest: "docs/SUMMARY.md".parse().unwrap(),
            entrypoints: vec!["/guide/".to_owned()],
            reachable: vec!["docs/missing.md".parse().unwrap()],
        }),
    ];
    for navigation in cases {
        let evidence = semantic_evidence(
            SemanticProducerKind::SiteBuild,
            "0.5.1",
            hb("test/site-output", b"site output"),
            None,
            vec![page.clone(), navigation],
        );
        let mut request = empty();
        request.semantic_evidence = vec![supplied_semantic(evidence)];
        let error = controls(request).expect_err("inconsistent navigation fails closed");
        assert_eq!(error.code, AnalysisErrorCode::ConfigurationInvalid);
    }
}
