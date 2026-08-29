#![expect(
    clippy::expect_used,
    reason = "integration assertions over the external-control request gate"
)]

use amiss_fixtures::{SiteObservation, record_set, site_navigation, site_observation};
use amiss_scan::request::controls;
use amiss_wire::digest::{Digest, hb};
use amiss_wire::json::{Value, parse};
use amiss_wire::model::ArtifactId;
use amiss_wire::report::AnalysisErrorCode;
use amiss_wire::requests::{
    ControlsRequest, RequestTrust, SuppliedControl, SuppliedSemanticEvidence, SuppliedTime,
};
use amiss_wire::semantic::SemanticEvidence;

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
        organization_floor: None,
        debt_snapshot: None,
        waiver_bundle: None,
        trusted_time: None,
        execution_constraint: None,
        semantic_evidence: Vec::new(),
    }
}

fn supplied(doc: &str, expected: Digest) -> SuppliedControl {
    SuppliedControl {
        value: parse(doc.as_bytes()).expect("the fixture is JSON"),
        expected_digest: expected,
        trust_source: RequestTrust::OrganizationPolicy,
    }
}

fn semantic_evidence(
    producer_kind: &str,
    producer_version: &str,
    input_digest: Digest,
    source_report_payload_digest: Option<Digest>,
    observations: Vec<Value>,
) -> SemanticEvidence {
    SemanticEvidence {
        candidate_identity_digest: hb("test/candidate", b"candidate"),
        source_report_payload_digest,
        producer_kind: ArtifactId::new(producer_kind.to_owned())
            .expect("the producer kind is valid"),
        producer_identity: ArtifactId::new("amiss-test".to_owned())
            .expect("the producer identity is valid"),
        producer_version: producer_version.to_owned(),
        context_digest: input_digest,
        input_digest,
        complete: true,
        observations,
    }
}

fn supplied_semantic(evidence: SemanticEvidence) -> SuppliedSemanticEvidence {
    let expected_context_digest = evidence.context_digest;
    SuppliedSemanticEvidence {
        value: amiss_wire::semantic::envelope(evidence)
            .expect("the generic envelope admits producer-defined semantics"),
        expected_context_digest,
    }
}

#[test]
fn a_verified_floor_lands_typed() {
    let floor =
        amiss_wire::controls::OrganizationFloor::parse(FLOOR.as_bytes()).expect("fixture parses");
    let mut request = empty();
    request.organization_floor = Some(supplied(FLOOR, floor.digest()));
    let inputs = controls(&request).expect("a matching digest passes the gate");
    let landed = inputs.floor.expect("the floor lands typed");
    assert_eq!(landed.floor.digest(), floor.digest());
    assert!(inputs.time.is_none() && inputs.debt.is_none());
}

#[test]
fn a_wrong_floor_digest_is_refused() {
    let mut request = empty();
    request.organization_floor = Some(supplied(FLOOR, hb("test/other", b"not the floor")));
    let error = controls(&request).expect_err("a foreign digest never passes");
    assert_eq!(error.code, AnalysisErrorCode::DigestMismatch);
}

#[test]
fn a_verified_time_statement_lands_with_its_run_context() {
    let statement =
        amiss_wire::controls::TrustedTimeStatement::parse(TIME.as_bytes()).expect("fixture parses");
    let mut request = empty();
    request.trusted_time = Some(SuppliedTime {
        value: parse(TIME.as_bytes()).expect("the fixture is JSON"),
        expected_digest: statement.digest(),
        provider: "gitlab-ci".to_owned(),
        provider_run_id: "pipeline/01J2Z9-7".to_owned(),
        provider_run_attempt: 2,
    });
    let inputs = controls(&request).expect("a matching digest passes the gate");
    let landed = inputs.time.expect("the statement lands typed");
    assert_eq!(landed.statement.digest(), statement.digest());
    assert_eq!(landed.provider, "gitlab-ci");
    assert_eq!(landed.provider_run_id, "pipeline/01J2Z9-7");
    assert_eq!(landed.provider_run_attempt, 2);
}

#[test]
fn a_wrong_time_digest_is_refused() {
    let mut request = empty();
    request.trusted_time = Some(SuppliedTime {
        value: parse(TIME.as_bytes()).expect("the fixture is JSON"),
        expected_digest: hb("test/other", b"not the statement"),
        provider: "gitlab-ci".to_owned(),
        provider_run_id: "pipeline/01J2Z9-7".to_owned(),
        provider_run_attempt: 2,
    });
    let error = controls(&request).expect_err("a foreign digest never passes");
    assert_eq!(error.code, AnalysisErrorCode::DigestMismatch);
}

#[test]
fn a_verified_constraint_lands_through_the_shared_gate() {
    let descriptor =
        amiss_wire::controls::ExecutionConstraintDescriptor::parse(CONSTRAINT.as_bytes())
            .expect("fixture parses");
    let mut request = empty();
    request.execution_constraint = Some(supplied(CONSTRAINT, descriptor.digest()));
    let inputs = controls(&request).expect("a matching digest passes the gate");
    let landed = inputs.constraint.expect("the descriptor lands typed");
    assert_eq!(landed.descriptor.digest(), descriptor.digest());
}

#[test]
fn a_wrong_constraint_digest_is_refused() {
    let mut request = empty();
    request.execution_constraint = Some(supplied(CONSTRAINT, hb("test/other", b"not the plan")));
    let error = controls(&request).expect_err("a foreign digest never passes");
    assert_eq!(error.code, AnalysisErrorCode::DigestMismatch);
}

#[test]
fn incomplete_or_invalid_inventory_evidence_never_becomes_input() {
    let valid = semantic_evidence(
        "sphinx-inventory-set",
        "1",
        hb("test/inventory", b"inventory"),
        None,
        vec![Value::object(vec![
            ("kind".to_owned(), Value::string("sphinx-label".to_owned())),
            ("inventory".to_owned(), Value::string("python".to_owned())),
            ("name".to_owned(), Value::string("except_star".to_owned())),
            (
                "destination".to_owned(),
                Value::string("https://docs.python.org/3/reference/".to_owned()),
            ),
        ])],
    );
    let mut incomplete = valid.clone();
    incomplete.complete = false;
    let mut unsupported = valid.clone();
    unsupported.producer_version = "2".to_owned();
    let mut malformed = valid;
    malformed.observations[0] = Value::object(vec![
        ("kind".to_owned(), Value::string("sphinx-label".to_owned())),
        ("inventory".to_owned(), Value::string("python".to_owned())),
        ("name".to_owned(), Value::string("except_star".to_owned())),
        (
            "destination".to_owned(),
            Value::string("https:///missing-authority".to_owned()),
        ),
    ]);

    for evidence in [incomplete, unsupported, malformed] {
        let mut request = empty();
        request.semantic_evidence = vec![supplied_semantic(evidence)];
        let error = controls(&request).expect_err("the inventory consumer fails closed");
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
            "record-set",
            "1",
            hb("test/records", b"rust public api"),
            None,
            vec![record_set("rust/public-api", &records)],
        );
        evidence.complete = complete;
        let mut request = empty();
        request.semantic_evidence = vec![supplied_semantic(evidence)];
        controls(&request).expect("one typed record set reaches the projection context");
    }
}

#[test]
fn malformed_record_sets_fail_closed() {
    let valid = semantic_evidence(
        "record-set",
        "1",
        hb("test/records", b"rust public api"),
        None,
        vec![record_set(
            "rust/public-api",
            &[("amiss::check", "pub fn check()")],
        )],
    );
    let mut wrong_version = valid.clone();
    wrong_version.producer_version = "2".to_owned();
    let mut report_derived = valid.clone();
    report_derived.source_report_payload_digest = Some(hb("test/report", b"report"));
    let mut multiple_sets = valid.clone();
    multiple_sets
        .observations
        .push(record_set("rust/other", &[]));
    let mut unsorted = valid.clone();
    unsorted.observations = vec![record_set("rust/public-api", &[("b", "B"), ("a", "A")])];
    let mut duplicate = valid.clone();
    duplicate.observations = vec![record_set("rust/public-api", &[("a", "A"), ("a", "B")])];
    let mut control_value = valid.clone();
    control_value.observations = vec![record_set("rust/public-api", &[("a", "line\nbreak")])];
    let mut empty_value = valid;
    empty_value.observations = vec![record_set("rust/public-api", &[("a", "")])];

    for evidence in [
        wrong_version,
        report_derived,
        multiple_sets,
        unsorted,
        duplicate,
        control_value,
        empty_value,
    ] {
        let mut request = empty();
        request.semantic_evidence = vec![supplied_semantic(evidence)];
        let error = controls(&request).expect_err("malformed record evidence stays unavailable");
        assert!(matches!(
            error.code,
            AnalysisErrorCode::ConfigurationInvalid | AnalysisErrorCode::NoncanonicalArray
        ));
    }
}

#[test]
fn two_envelopes_cannot_claim_the_same_record_set() {
    let mut left = semantic_evidence(
        "record-set",
        "1",
        hb("test/records", b"left"),
        None,
        vec![record_set("rust/public-api", &[("a", "A")])],
    );
    left.context_digest = hb("test/record-context", b"left");
    let mut right = semantic_evidence(
        "record-set",
        "1",
        hb("test/records", b"right"),
        None,
        vec![record_set("rust/public-api", &[("b", "B")])],
    );
    right.context_digest = hb("test/record-context", b"right");
    let mut supplied = vec![supplied_semantic(left), supplied_semantic(right)];
    supplied.sort_by_key(|item| {
        amiss_wire::semantic::parse(&amiss_wire::json::canonical(&item.value))
            .expect("the generic envelopes are valid")
            .payload_digest
    });
    let mut request = empty();
    request.semantic_evidence = supplied;
    let error = controls(&request).expect_err("one set name has one evidence authority");
    assert_eq!(error.code, AnalysisErrorCode::NoncanonicalArray);
}

#[test]
fn semantic_evidence_must_match_the_independently_supplied_context() {
    let evidence = semantic_evidence(
        "sphinx-inventory-set",
        "1",
        hb("test/inventory", b"inventory"),
        None,
        Vec::new(),
    );
    let mut request = empty();
    request.semantic_evidence = vec![SuppliedSemanticEvidence {
        value: amiss_wire::semantic::envelope(evidence).expect("the generic envelope is valid"),
        expected_context_digest: hb("test/inventory", b"another inventory"),
    }];

    let error = controls(&request).expect_err("a foreign context never reaches a consumer");
    assert_eq!(error.code, AnalysisErrorCode::DigestMismatch);
}

#[test]
fn incomplete_or_invalid_site_build_evidence_never_becomes_input() {
    let valid = semantic_evidence(
        "site-build",
        "0.5.1",
        hb("test/site-output", b"site output"),
        Some(hb("test/report", b"source report")),
        vec![
            site_observation(
                "/guide/",
                SiteObservation::Page("docs/guide.md", &["details", "intro"]),
            ),
            site_navigation(
                Some("docs"),
                "docs/SUMMARY.md",
                &["/guide/"],
                &["docs/guide.md"],
            ),
        ],
    );
    let mut incomplete = valid.clone();
    incomplete.complete = false;
    let mut unsupported = valid.clone();
    unsupported.producer_version = "0.2.0".to_owned();
    let mut invalid_route = valid.clone();
    invalid_route.observations = vec![site_observation(
        "//other.example/guide",
        SiteObservation::Page("docs/guide.md", &["intro"]),
    )];
    let mut invalid_source = valid.clone();
    invalid_source.observations = vec![site_observation(
        "/guide/",
        SiteObservation::Page("../guide.md", &["intro"]),
    )];
    let mut unsorted_anchors = valid.clone();
    unsorted_anchors.observations = vec![site_observation(
        "/guide/",
        SiteObservation::Page("docs/guide.md", &["intro", "details"]),
    )];
    let mut duplicate_anchors = valid;
    duplicate_anchors.observations = vec![site_observation(
        "/guide/",
        SiteObservation::Page("docs/guide.md", &["intro", "intro"]),
    )];
    let malformed_fragment_redirect = semantic_evidence(
        "site-build",
        "0.5.1",
        hb("test/site-output", b"site output"),
        Some(hb("test/report", b"source report")),
        vec![site_observation(
            "/legacy/",
            SiteObservation::Redirect("docs/redirects.toml", "/guide/#bad%fragment"),
        )],
    );
    let mut query_redirect = malformed_fragment_redirect.clone();
    let mut foreign_redirect = malformed_fragment_redirect.clone();
    let mut self_redirect = malformed_fragment_redirect.clone();
    let mut accepted = empty();
    accepted.semantic_evidence = vec![supplied_semantic(malformed_fragment_redirect)];
    controls(&accepted).expect("a literal percent sign remains a valid redirect fragment");
    query_redirect.observations = vec![site_observation(
        "/legacy/",
        SiteObservation::Redirect("docs/redirects.toml", "/guide/?language=en"),
    )];
    foreign_redirect.observations = vec![site_observation(
        "/legacy/",
        SiteObservation::Redirect("docs/redirects.toml", "//other.example/guide/"),
    )];
    self_redirect.observations = vec![site_observation(
        "/legacy/",
        SiteObservation::Redirect("docs/redirects.toml", "/legacy/#intro"),
    )];

    for (evidence, expected) in [
        (incomplete, AnalysisErrorCode::ConfigurationInvalid),
        (unsupported, AnalysisErrorCode::ConfigurationInvalid),
        (invalid_route, AnalysisErrorCode::ConfigurationInvalid),
        (invalid_source, AnalysisErrorCode::ConfigurationInvalid),
        (unsorted_anchors, AnalysisErrorCode::NoncanonicalArray),
        (duplicate_anchors, AnalysisErrorCode::NoncanonicalArray),
        (query_redirect, AnalysisErrorCode::ConfigurationInvalid),
        (foreign_redirect, AnalysisErrorCode::ConfigurationInvalid),
        (self_redirect, AnalysisErrorCode::ConfigurationInvalid),
    ] {
        let mut request = empty();
        request.semantic_evidence = vec![supplied_semantic(evidence)];
        let error = controls(&request).expect_err("the site-build consumer fails closed");
        assert_eq!(error.code, expected);
    }
}

#[test]
fn site_claims_require_explicit_source_attribution() {
    for observation in [
        Value::object(vec![
            (
                "destination".to_owned(),
                Value::string("/guide/".to_owned()),
            ),
            ("kind".to_owned(), Value::string("site-redirect".to_owned())),
            ("route".to_owned(), Value::string("/legacy/".to_owned())),
        ]),
        Value::object(vec![
            ("anchors".to_owned(), Value::array(Vec::new())),
            ("kind".to_owned(), Value::string("site-route".to_owned())),
            ("route".to_owned(), Value::string("/guide/".to_owned())),
            ("source".to_owned(), Value::Null),
        ]),
        Value::object(vec![
            (
                "destination".to_owned(),
                Value::string("/guide/".to_owned()),
            ),
            ("kind".to_owned(), Value::string("site-redirect".to_owned())),
            ("route".to_owned(), Value::string("/legacy/".to_owned())),
            ("source".to_owned(), Value::Null),
        ]),
        Value::object(vec![
            ("anchors".to_owned(), Value::array(Vec::new())),
            (
                "kind".to_owned(),
                Value::string("site-generated-route".to_owned()),
            ),
            ("route".to_owned(), Value::string("/generated/".to_owned())),
        ]),
    ] {
        let evidence = semantic_evidence(
            "site-build",
            "0.5.1",
            hb("test/site-output", b"site output"),
            Some(hb("test/report", b"source report")),
            vec![observation],
        );
        let mut request = empty();
        request.semantic_evidence = vec![supplied_semantic(evidence)];
        let error = controls(&request).expect_err("a claim without its source fails closed");
        assert_eq!(error.code, AnalysisErrorCode::ConfigurationInvalid);
    }
}

#[test]
fn generated_site_claims_admit_absent_repository_attribution() {
    let evidence = semantic_evidence(
        "site-build",
        "0.5.1",
        hb("test/site-output", b"site output"),
        Some(hb("test/report", b"source report")),
        vec![
            site_observation("/generated/", SiteObservation::Generated(None, &["intro"])),
            site_navigation(Some("docs"), "docs/SUMMARY.md", &["/generated/"], &[]),
        ],
    );
    let mut request = empty();
    request.semantic_evidence = vec![supplied_semantic(evidence)];

    controls(&request).expect("explicit null attribution is a valid generated-page claim");
}

#[test]
fn inconsistent_site_navigation_never_becomes_input() {
    let page = site_observation(
        "/guide/",
        SiteObservation::Page("docs/guide.md", &["intro"]),
    );
    let cases = [
        site_navigation(
            Some("docs"),
            "other/SUMMARY.md",
            &["/guide/"],
            &["docs/guide.md"],
        ),
        site_navigation(
            Some("docs"),
            "docs/SUMMARY.md",
            &["/missing/"],
            &["docs/guide.md"],
        ),
        site_navigation(
            Some("docs"),
            "docs/SUMMARY.md",
            &["/guide/"],
            &["docs/missing.md"],
        ),
    ];
    for navigation in cases {
        let evidence = semantic_evidence(
            "site-build",
            "0.5.1",
            hb("test/site-output", b"site output"),
            None,
            vec![page.clone(), navigation],
        );
        let mut request = empty();
        request.semantic_evidence = vec![supplied_semantic(evidence)];
        let error = controls(&request).expect_err("inconsistent navigation fails closed");
        assert_eq!(error.code, AnalysisErrorCode::ConfigurationInvalid);
    }
}
