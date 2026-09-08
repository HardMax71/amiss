#![expect(
    clippy::expect_used,
    reason = "test assertions over constructed values"
)]

use amiss_wire::de::ErrorKind;
use amiss_wire::digest::{Digest, hb};
use amiss_wire::external::{
    ASSESSMENT_PAYLOAD_SCHEMA, AssessDefect, EVIDENCE_SCHEMA, ExternalAssessmentEnvelope,
    ExternalDestination, ExternalEvidence, ExternalEvidenceProducer, ExternalEvidenceRow,
    ExternalEvidenceSchema, ExternalPlanEnvelope, ExternalPlanEnvelopeSchema, ExternalRepository,
    ForgeRepository, ForgeTail, PLAN_PAYLOAD_SCHEMA, PlanDefect, ProbeFailure, ProbeMethod, assess,
    parse_evidence, parse_plan, plan,
};
use amiss_wire::json::Value;
use amiss_wire::report::{
    PAYLOAD_SCHEMA,
    model::{ReportEnvelope, ReportStatus},
    validate_envelope,
};

const REPORT: &[u8] = include_bytes!("../../../../spec/examples/scanner-report.canonical.json");

fn object(members: Vec<(&str, Value)>) -> Value {
    let mut members: Vec<(String, Value)> = members
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value))
        .collect();
    members.sort_by(|left, right| left.0.cmp(&right.0));
    Value::object(members)
}

fn string(value: &str) -> Value {
    Value::string(value)
}

fn external_occurrence(document: &str, destination: &str) -> Value {
    object(vec![
        ("document", string(document)),
        ("external_destination", string(destination)),
        ("intent", object(vec![("external_scheme", string("https"))])),
        (
            "resolution",
            object(vec![
                ("kind", string("external")),
                ("reason", string("url")),
            ]),
        ),
    ])
}

fn resolved_occurrence(document: &str) -> Value {
    object(vec![
        ("document", string(document)),
        ("resolution", object(vec![("kind", string("resolved"))])),
    ])
}

fn row(base: Value, candidate: Value) -> Value {
    object(vec![("base", base), ("candidate", candidate)])
}

fn report(observations: Vec<Value>) -> ReportEnvelope {
    let mut document: serde_json::Value =
        serde_json::from_slice(REPORT).expect("the report example is valid JSON");
    let examples = document
        .pointer("/payload/observations")
        .and_then(serde_json::Value::as_array)
        .expect("the report example has observations");
    let resolved = examples
        .first()
        .and_then(|row| row.get("candidate"))
        .expect("the report example has a resolved occurrence")
        .clone();
    let external = examples
        .get(1)
        .expect("the report example has an external comparison")
        .clone();
    let external_occurrence = external
        .get("candidate")
        .expect("the external comparison has a candidate")
        .clone();
    let rows = observations
        .into_iter()
        .map(|row| {
            let supplied = serde_json::to_value(&row).expect("the test comparison is JSON");
            let mut comparison = external.clone();
            for side in ["base", "candidate"] {
                let expanded = expand_occurrence(
                    supplied.get(side).expect("the comparison has both sides"),
                    &resolved,
                    &external_occurrence,
                );
                *comparison
                    .get_mut(side)
                    .expect("the example comparison has both sides") = expanded;
            }
            comparison
        })
        .collect();
    *document
        .pointer_mut("/payload/observations")
        .expect("the report example has observations") = serde_json::Value::Array(rows);
    let bytes = refresh_payload_digest(&mut document, PAYLOAD_SCHEMA);
    validate_envelope(&bytes)
        .expect("the completed test report is accepted")
        .0
}

fn expand_occurrence(
    supplied: &serde_json::Value,
    resolved: &serde_json::Value,
    external: &serde_json::Value,
) -> serde_json::Value {
    if supplied.is_null() {
        return serde_json::Value::Null;
    }
    let is_resolved = supplied
        .pointer("/resolution/kind")
        .and_then(|kind| kind.as_str())
        == Some("resolved");
    let mut occurrence = if is_resolved {
        resolved.clone()
    } else {
        external.clone()
    };
    let supplied = supplied
        .as_object()
        .expect("the supplied occurrence is an object");
    let occurrence_object = occurrence
        .as_object_mut()
        .expect("the example occurrence is an object");
    occurrence_object.insert(
        "document".to_owned(),
        supplied
            .get("document")
            .expect("the supplied occurrence has a document")
            .clone(),
    );
    match supplied.get("external_destination") {
        Some(destination) => {
            occurrence_object.insert("external_destination".to_owned(), destination.clone());
        }
        None => {
            occurrence_object.remove("external_destination");
        }
    }
    if let Some(intent) = supplied
        .get("intent")
        .and_then(serde_json::Value::as_object)
    {
        let target = occurrence_object
            .get_mut("intent")
            .and_then(serde_json::Value::as_object_mut)
            .expect("the example occurrence has an intent");
        target.extend(intent.clone());
    }
    if !is_resolved {
        occurrence_object.insert(
            "resolution".to_owned(),
            supplied
                .get("resolution")
                .expect("the supplied occurrence has a resolution")
                .clone(),
        );
    }
    occurrence
}

fn sample_digest() -> Digest {
    hb(
        PAYLOAD_SCHEMA,
        &serde_json_canonicalizer::to_vec(&Value::Null).expect("fixture JSON"),
    )
}

fn refresh_payload_digest(document: &mut serde_json::Value, domain: &str) -> Vec<u8> {
    let payload = document
        .get("payload")
        .expect("the plan document holds its payload");
    let canonical = serde_json_canonicalizer::to_vec(payload)
        .expect("the plan payload is canonically serializable");
    let digest = hb(domain, &canonical).to_string();
    let recorded = document
        .get_mut("payload_digest")
        .expect("the plan document holds its digest");
    *recorded = serde_json::Value::String(digest);
    serde_json_canonicalizer::to_vec(document).expect("the plan document is serializable")
}

fn destinations(rows: &[ExternalDestination]) -> Vec<(String, Vec<String>)> {
    rows.iter()
        .map(|row| (row.destination.clone(), row.documents.clone()))
        .collect()
}

#[test]
fn the_delta_is_set_wise_and_document_attributed() {
    let plan = plan(
        &report(vec![
            row(
                external_occurrence("docs/a.md", "https://old.example/g"),
                Value::Null,
            ),
            row(
                Value::Null,
                external_occurrence("docs/a.md", "https://new.example/n"),
            ),
            row(
                Value::Null,
                external_occurrence("docs/b.md", "https://new.example/n"),
            ),
            row(
                external_occurrence("docs/a.md", "https://kept.example/k"),
                external_occurrence("docs/a.md", "https://kept.example/k"),
            ),
            row(resolved_occurrence("docs/a.md"), Value::Null),
        ]),
        "0.0.0",
        sample_digest(),
    )
    .expect("the report yields a plan");
    assert_eq!(
        destinations(&plan.payload.introduced),
        vec![(
            "https://new.example/n".to_owned(),
            vec!["docs/a.md".to_owned(), "docs/b.md".to_owned()]
        )],
    );
    assert_eq!(
        destinations(&plan.payload.removed),
        vec![(
            "https://old.example/g".to_owned(),
            vec!["docs/a.md".to_owned()]
        )],
    );
    assert_eq!(plan.payload.retained_count, 1);
}

#[test]
fn trusted_semantic_resolutions_never_enter_the_network_plan() {
    let observations = [
        ("docs/a.md", "intersphinx-inventory"),
        ("docs/b.md", "site-build"),
    ]
    .into_iter()
    .map(|(document, reason)| {
        row(
            Value::Null,
            object(vec![
                ("document", string(document)),
                (
                    "resolution",
                    object(vec![
                        ("kind", string("external")),
                        ("reason", string(reason)),
                    ]),
                ),
            ]),
        )
    })
    .collect();
    let plan =
        plan(&report(observations), "0.0.0", sample_digest()).expect("the report yields a plan");
    assert_eq!(destinations(&plan.payload.introduced), Vec::new());
    assert_eq!(plan.payload.retained_count, 0);
}

/// A destination that only moved between documents is retained, never
/// introduced: membership is repository-wide, attribution is per document.
#[test]
fn a_destination_moving_documents_is_retained() {
    let plan = plan(
        &report(vec![
            row(
                external_occurrence("docs/a.md", "https://kept.example/k"),
                Value::Null,
            ),
            row(
                Value::Null,
                external_occurrence("docs/b.md", "https://kept.example/k"),
            ),
        ]),
        "0.0.0",
        sample_digest(),
    )
    .expect("the report yields a plan");
    assert_eq!(destinations(&plan.payload.introduced), Vec::new());
    assert_eq!(destinations(&plan.payload.removed), Vec::new());
    assert_eq!(plan.payload.retained_count, 1);
}

#[test]
fn unavailable_exact_history_enters_the_same_setwise_plan() {
    let destination =
        "https://github.com/acme/widgets/blob/0123456789012345678901234567890123456789/docs/a.md";
    let historical = object(vec![
        ("document", string("docs/a.md")),
        ("external_destination", string(destination)),
        ("intent", object(Vec::new())),
        (
            "resolution",
            object(vec![
                ("kind", string("unsupported-version")),
                (
                    "scope",
                    object(vec![
                        ("kind", string("known-commit")),
                        (
                            "commit_oid",
                            string("0123456789012345678901234567890123456789"),
                        ),
                        ("path", string("docs/a.md")),
                    ]),
                ),
            ]),
        ),
    ]);
    let introduced_plan = plan(
        &report(vec![row(Value::Null, historical.clone())]),
        "0.0.0",
        sample_digest(),
    )
    .expect("the report yields a plan");
    assert_eq!(
        destinations(&introduced_plan.payload.introduced),
        vec![(destination.to_owned(), vec!["docs/a.md".to_owned()])]
    );
    let introduced = &introduced_plan.payload.introduced[0];
    assert_eq!(introduced.scheme, "https");
    assert_eq!(
        String::from_utf8(serde_json_canonicalizer::to_vec(&introduced.repository).unwrap())
            .expect("canonical utf-8"),
        r#"{"dialect":"github","form":"blob","host":"github.com","name":"widgets","owner":"acme","tail":"0123456789012345678901234567890123456789/docs/a.md"}"#
    );

    let retained_plan = plan(
        &report(vec![row(historical.clone(), historical.clone())]),
        "0.0.0",
        sample_digest(),
    )
    .expect("the report yields a plan");
    assert_eq!(destinations(&retained_plan.payload.introduced), Vec::new());
    assert_eq!(destinations(&retained_plan.payload.removed), Vec::new());
    assert_eq!(retained_plan.payload.retained_count, 1);

    let Value::Object(historical) = historical else {
        panic!("the occurrence is an object");
    };
    let mut historical = historical.into_vec();
    historical.retain(|(name, _)| name != "external_destination");
    let source = report(vec![row(Value::Null, Value::object(historical))]);
    assert_eq!(
        plan(&source, "0.0.0", sample_digest()),
        Err(PlanDefect::MalformedExternal)
    );
}

#[test]
fn the_envelope_binds_the_source_digest_and_its_own() {
    let source = report(Vec::new());
    let derived = plan(&source, "0.0.0", sample_digest()).expect("an empty report yields a plan");
    assert_eq!(derived.schema, ExternalPlanEnvelopeSchema::Current);
    let recomputed = hb(
        PLAN_PAYLOAD_SCHEMA,
        &serde_json_canonicalizer::to_vec(&derived.payload).expect("fixture JSON"),
    );
    assert_eq!(
        derived.payload_digest, recomputed,
        "the plan digest is recomputable from its payload"
    );
    assert_eq!(
        derived.payload.report.payload_digest, source.payload_digest,
        "the plan binds the digest of the report it read"
    );
}

#[test]
fn the_plan_model_reads_the_checked_writer() {
    let written = plan(
        &report(introduced("https://github.com/acme/widgets")),
        "0.0.0",
        sample_digest(),
    )
    .expect("the report yields a plan");
    let bytes = serde_json_canonicalizer::to_vec(&written).unwrap();
    let parsed = parse_plan(&bytes).expect("the written plan clears the typed reader");
    assert_eq!(parsed.payload.introduced.len(), 1);
    assert_eq!(
        serde_json_canonicalizer::to_vec(&parsed).expect("the model is serializable"),
        bytes,
    );
}

#[test]
fn known_optional_plan_fields_do_not_accept_null() {
    let written = plan(
        &report(introduced("https://github.com/acme/widgets/blob/main/a.md")),
        "0.0.0",
        sample_digest(),
    )
    .expect("the report yields a plan");
    let mut document = serde_json::to_value(&written).expect("the written plan is JSON");
    let repository = document
        .pointer_mut("/payload/introduced/0/repository")
        .and_then(serde_json::Value::as_object_mut)
        .expect("the introduced destination has a repository shape");
    repository.insert("form".to_owned(), serde_json::Value::Null);
    let error =
        parse_plan(&refresh_payload_digest(&mut document, PLAN_PAYLOAD_SCHEMA)).unwrap_err();
    assert_eq!(error.kind, ErrorKind::WrongType);
    assert_eq!(error.path, "$.payload.introduced[0].repository.form");
}

#[test]
fn malformed_known_plan_fields_are_refused_after_binding() {
    let written = plan(
        &report(introduced("https://example.com/manual")),
        "0.0.0",
        sample_digest(),
    )
    .expect("the report yields a plan");
    let mut document = serde_json::to_value(&written).expect("the written plan is JSON");
    let destination = document
        .pointer_mut("/payload/introduced/0/destination")
        .expect("the introduced row holds a destination");
    *destination = serde_json::Value::String(String::new());
    let error =
        parse_plan(&refresh_payload_digest(&mut document, PLAN_PAYLOAD_SCHEMA)).unwrap_err();
    assert_eq!(error.kind, ErrorKind::InvalidValue);
    assert_eq!(error.path, "$.payload.introduced[0].destination");
}

#[test]
fn a_tampered_payload_is_refused() {
    let mut envelope: ReportEnvelope = serde_json::from_slice(REPORT).unwrap();
    envelope.payload.result.finding_count += 1;
    let wire = String::from_utf8(serde_json_canonicalizer::to_vec(&envelope).unwrap()).unwrap();
    assert_eq!(
        plan(&envelope, "0.0.0", sample_digest()),
        Err(PlanDefect::DigestMismatch)
    );
    let result = serde_json::to_string(&envelope.payload.result).unwrap();
    let malformed = wire.replace(&format!("\"result\":{result}"), "\"result\":null");
    assert_ne!(malformed, wire);
    assert_eq!(
        validate_envelope(malformed.as_bytes()).map(drop),
        Err(PlanDefect::NotAReport)
    );
}

#[test]
fn an_incomplete_report_is_refused() {
    let mut envelope = report(Vec::new());
    envelope.payload.result.complete = false;
    envelope.payload.result.status = ReportStatus::Incomplete;
    envelope.payload.result.exit_code = 2;
    envelope.payload_digest = hb(
        PAYLOAD_SCHEMA,
        &serde_json_canonicalizer::to_vec(&envelope.payload).unwrap(),
    );
    assert_eq!(
        plan(&envelope, "0.0.0", sample_digest()),
        Err(PlanDefect::Incomplete)
    );
}

#[test]
fn a_foreign_value_is_not_a_report() {
    assert_eq!(
        validate_envelope(b"null").map(drop),
        Err(PlanDefect::NotAReport)
    );
    assert_eq!(
        validate_envelope(br#"{"schema":"amiss/something-else"}"#).map(drop),
        Err(PlanDefect::NotAReport)
    );
}

fn repository_of<'a>(
    planned: &'a ExternalPlanEnvelope,
    destination: &str,
) -> Option<&'a ExternalRepository> {
    planned
        .payload
        .introduced
        .iter()
        .find(|row| row.destination == destination)
        .and_then(|row| row.repository.as_ref())
}

fn introduced(destination: &str) -> Vec<Value> {
    vec![row(
        Value::Null,
        external_occurrence("docs/a.md", destination),
    )]
}

#[test]
fn a_known_host_destination_carries_its_forge_shape() {
    let cases = [
        (
            "https://github.com/acme/widgets/blob/feature/x/docs/a.md",
            r#"{"dialect":"github","form":"blob","host":"github.com","name":"widgets","owner":"acme","tail":"feature/x/docs/a.md"}"#,
        ),
        (
            "https://github.com/acme/widgets",
            r#"{"dialect":"github","host":"github.com","name":"widgets","owner":"acme"}"#,
        ),
        (
            "https://gitlab.com/group/sub/widgets/-/blob/main/a.md",
            r#"{"dialect":"gitlab","form":"blob","host":"gitlab.com","name":"widgets","owner":"group/sub","tail":"main/a.md"}"#,
        ),
        (
            "https://gitlab.com/acme/widgets",
            r#"{"dialect":"gitlab","host":"gitlab.com","name":"widgets","owner":"acme"}"#,
        ),
        (
            "https://github.com/acme/widgets/tree/main/docs/",
            r#"{"dialect":"github","form":"tree","host":"github.com","name":"widgets","owner":"acme","tail":"main/docs/"}"#,
        ),
        (
            "https://codeberg.org/acme/widgets/src/branch/main/a.md",
            r#"{"dialect":"gitea","form":"src","host":"codeberg.org","name":"widgets","owner":"acme","tail":"branch/main/a.md"}"#,
        ),
        (
            "https://bitbucket.org/acme/widgets/src/main/a.md",
            r#"{"dialect":"bitbucket-cloud","form":"src","host":"bitbucket.org","name":"widgets","owner":"acme","tail":"main/a.md"}"#,
        ),
        (
            "https://github.com/acme/widgets/blob/main/f.md#L10",
            r#"{"dialect":"github","form":"blob","host":"github.com","name":"widgets","owner":"acme","tail":"main/f.md"}"#,
        ),
        (
            "https://github.com/acme/widgets?tab=readme",
            r#"{"dialect":"github","host":"github.com","name":"widgets","owner":"acme"}"#,
        ),
    ];
    for (destination, expected) in cases {
        let plan = plan(&report(introduced(destination)), "0.0.0", sample_digest())
            .expect("the report yields a plan");
        let repository = repository_of(&plan, destination)
            .unwrap_or_else(|| panic!("{destination} carries no shape"));
        assert_eq!(
            String::from_utf8(serde_json_canonicalizer::to_vec(&repository).unwrap())
                .expect("canonical utf-8"),
            expected,
            "{destination}"
        );
    }
}

#[test]
fn an_unrecognizable_destination_stays_unshaped() {
    for destination in [
        "https://example.com/manual",
        "http://github.com/acme/widgets",
        "https://GitHub.com/acme/widgets",
        "https://github.com/acme",
        "https://github.com//widgets/blob/main/a.md",
        "https://gitlab.com/group/-/blob/main/a.md",
        "https://github.com",
        "https://gitlab.com/acme/widgets/blob/main/a.md",
        "https://gitlab.com/group/sub/widgets",
    ] {
        let plan = plan(&report(introduced(destination)), "0.0.0", sample_digest())
            .expect("the report yields a plan");
        assert_eq!(
            repository_of(&plan, destination),
            None,
            "{destination} must stay unshaped"
        );
    }
}

/// The report's own declared identity extends recognition to its host, with
/// the dialect the evaluation already names.
#[test]
fn the_declared_host_is_recognized_with_its_declared_dialect() {
    let cases = [
        (
            "ghes.corp.example",
            "github",
            "https://ghes.corp.example/other/repo/blob/main/x.md",
            r#"{"dialect":"github","form":"blob","host":"ghes.corp.example","name":"repo","owner":"other","tail":"main/x.md"}"#,
        ),
        (
            "bitbucket.corp.example",
            "bitbucket-data-center",
            "https://bitbucket.corp.example/bitbucket/projects/ACME/repos/widgets/browse/docs/a.md?at=refs%2Fheads%2Fmain",
            r#"{"dialect":"bitbucket-data-center","form":"browse","host":"bitbucket.corp.example","name":"widgets","owner":"ACME","tail":"docs/a.md"}"#,
        ),
        (
            "bitbucket.corp.example",
            "bitbucket-data-center",
            "https://bitbucket.corp.example/bitbucket/users/alice/repos/widgets/browse/docs/a.md",
            r#"{"dialect":"bitbucket-data-center","form":"browse","host":"bitbucket.corp.example","name":"widgets","owner":"alice","tail":"docs/a.md"}"#,
        ),
        (
            "bitbucket.corp.example",
            "bitbucket-data-center",
            "https://bitbucket.corp.example/projects/OTHER/repos/else/browse/projects/ACME/repos/widgets/browse/docs/a.md",
            r#"{"dialect":"bitbucket-data-center","form":"browse","host":"bitbucket.corp.example","name":"else","owner":"OTHER","tail":"projects/ACME/repos/widgets/browse/docs/a.md"}"#,
        ),
    ];
    for (host, dialect, destination, expected) in cases {
        let mut document: serde_json::Value = serde_json::from_slice(
            &serde_json_canonicalizer::to_vec(&report(introduced(destination))).unwrap(),
        )
        .expect("the complete report is JSON");
        document["payload"]["evaluation"]["forge"] = serde_json::Value::String(dialect.to_owned());
        document["payload"]["evaluation"]["repository"]["host"] =
            serde_json::Value::String(host.to_owned());
        let (envelope, _) =
            validate_envelope(&refresh_payload_digest(&mut document, PAYLOAD_SCHEMA))
                .expect("the declared-host report is accepted");
        let derived = plan(&envelope, "0.0.0", sample_digest())
            .expect("the declared-host report yields a plan");
        let repository = repository_of(&derived, destination).expect("the declared host is shaped");
        assert_eq!(
            String::from_utf8(serde_json_canonicalizer::to_vec(&repository).unwrap())
                .expect("canonical utf-8"),
            expected,
            "{destination}"
        );
    }
}

fn evidence(plan: &ExternalPlanEnvelope, rows: Vec<ExternalEvidenceRow>) -> ExternalEvidence {
    ExternalEvidence {
        schema: ExternalEvidenceSchema::Current,
        plan_payload_digest: plan.payload_digest,
        producer: ExternalEvidenceProducer {
            name: "amiss-probe".to_owned(),
            version: "0.0.0".to_owned(),
        },
        rows,
    }
}

fn probe(destination: &str, method: ProbeMethod, status: u16) -> ExternalEvidenceRow {
    ExternalEvidenceRow::HttpProbe {
        destination: destination.to_owned(),
        method,
        status: Some(status),
        failure: None,
        final_destination: None,
        redirect_chain_permanent: None,
        checked_at: "t0".to_owned(),
    }
}

fn forge_row(
    destination: &str,
    repository: ForgeRepository,
    tail: Option<ForgeTail>,
) -> ExternalEvidenceRow {
    ExternalEvidenceRow::ForgeApi {
        destination: destination.to_owned(),
        repository,
        tail,
        checked_at: "t0".to_owned(),
    }
}

#[test]
fn evidence_bytes_preserve_escaping_and_round_trip() {
    let document = ExternalEvidence {
        schema: ExternalEvidenceSchema::Current,
        plan_payload_digest: sample_digest(),
        producer: ExternalEvidenceProducer {
            name: "probe \"quoted\" \\ \n \t 😀".to_owned(),
            version: "0.0.0".to_owned(),
        },
        rows: vec![ExternalEvidenceRow::HttpProbe {
            destination: "https://example.com/é".to_owned(),
            method: ProbeMethod::Get,
            status: Some(200),
            failure: None,
            final_destination: None,
            redirect_chain_permanent: None,
            checked_at: "t0".to_owned(),
        }],
    };
    let bytes = amiss_wire::external::evidence(&document).expect("the evidence encodes");
    let (parsed, _digest) = parse_evidence(&bytes).expect("the evidence parses");
    assert_eq!(parsed, document);
    assert_eq!(bytes, serde_json_canonicalizer::to_vec(&parsed).unwrap(),);
    assert!(!bytes.ends_with(b"\n"));
}

#[test]
fn derived_validation_rejects_invalid_evidence_shapes() {
    let row = ExternalEvidenceRow::HttpProbe {
        destination: "https://example.com/a".to_owned(),
        method: ProbeMethod::Get,
        status: Some(42),
        failure: Some(ProbeFailure::Tls),
        final_destination: None,
        redirect_chain_permanent: Some(false),
        checked_at: String::new(),
    };
    let document = ExternalEvidence {
        schema: ExternalEvidenceSchema::Current,
        plan_payload_digest: sample_digest(),
        producer: ExternalEvidenceProducer {
            name: String::new(),
            version: String::new(),
        },
        rows: vec![row.clone(), row],
    };
    assert!(amiss_wire::external::evidence(&document).is_err());
}

fn verdicts_of(assessment: &ExternalAssessmentEnvelope) -> Vec<(String, String, String)> {
    assessment
        .payload
        .verdicts
        .iter()
        .map(|row| {
            (
                row.destination.clone(),
                row.verdict.to_string(),
                row.reason
                    .map(|reason| reason.to_string())
                    .unwrap_or_default(),
            )
        })
        .collect()
}

#[test]
fn the_judgment_policy_is_conservative() {
    let destinations = [
        (
            "https://a.example/gone",
            probe("https://a.example/gone", ProbeMethod::Get, 410),
        ),
        (
            "https://b.example/head404",
            probe("https://b.example/head404", ProbeMethod::Head, 404),
        ),
        (
            "https://c.example/ok",
            probe("https://c.example/ok", ProbeMethod::Head, 200),
        ),
        (
            "https://d.example/wall",
            probe("https://d.example/wall", ProbeMethod::Get, 403),
        ),
        (
            "https://e.example/limit",
            probe("https://e.example/limit", ProbeMethod::Get, 429),
        ),
    ];
    let observations = destinations
        .iter()
        .map(|(destination, _)| row(Value::Null, external_occurrence("docs/a.md", destination)))
        .chain(std::iter::once(row(
            Value::Null,
            external_occurrence("docs/a.md", "https://f.example/quiet"),
        )))
        .collect();
    let plan =
        plan(&report(observations), "0.0.0", sample_digest()).expect("the report yields a plan");
    let evidence = evidence(
        &plan,
        destinations.iter().map(|(_, row)| row.clone()).collect(),
    );
    let assessment =
        assess(&plan, &evidence, "0.0.0", sample_digest()).expect("the pair yields an assessment");
    assert_eq!(
        verdicts_of(&assessment),
        vec![
            (
                "https://a.example/gone".into(),
                "refuted".into(),
                "gone".into()
            ),
            (
                "https://b.example/head404".into(),
                "unproven".into(),
                "unconfirmed".into()
            ),
            (
                "https://c.example/ok".into(),
                "reachable".into(),
                String::new()
            ),
            (
                "https://d.example/wall".into(),
                "unproven".into(),
                "denied".into()
            ),
            (
                "https://e.example/limit".into(),
                "unproven".into(),
                "rate-limited".into()
            ),
            (
                "https://f.example/quiet".into(),
                "unproven".into(),
                "unexamined".into()
            ),
        ],
    );
}

#[test]
fn only_a_proved_permanent_redirect_becomes_a_retarget() {
    let permanent = "https://a.example/old";
    let temporary = "https://b.example/old";
    let permanent_target = "https://a.example/current";
    let temporary_target = "https://b.example/current";
    let plan = plan(
        &report(vec![
            row(Value::Null, external_occurrence("docs/a.md", permanent)),
            row(Value::Null, external_occurrence("docs/a.md", temporary)),
        ]),
        "0.0.0",
        sample_digest(),
    )
    .expect("the report yields a plan");
    let observed = evidence(
        &plan,
        [
            (permanent, permanent_target, Some(true)),
            (temporary, temporary_target, None),
        ]
        .into_iter()
        .map(
            |(destination, target, redirect_chain_permanent)| ExternalEvidenceRow::HttpProbe {
                destination: destination.to_owned(),
                method: ProbeMethod::Head,
                status: Some(200),
                failure: None,
                final_destination: Some(target.to_owned()),
                redirect_chain_permanent,
                checked_at: "t0".to_owned(),
            },
        )
        .collect(),
    );
    let assessment =
        assess(&plan, &observed, "0.0.0", sample_digest()).expect("the redirects are evidence");
    let verdicts = &assessment.payload.verdicts;
    let verdict = |destination: &str| {
        verdicts
            .iter()
            .find(|row| row.destination == destination)
            .expect("the plan destination has one verdict")
    };
    assert_eq!(
        verdict(permanent).retarget.as_deref(),
        Some(permanent_target)
    );
    assert_eq!(verdict(temporary).retarget.as_deref(), None);

    for (final_destination, redirect_chain_permanent) in [
        (None, Some(true)),
        (Some(permanent_target.to_owned()), Some(false)),
    ] {
        let malformed = ExternalEvidenceRow::HttpProbe {
            destination: permanent.to_owned(),
            method: ProbeMethod::Head,
            status: Some(200),
            failure: None,
            final_destination,
            redirect_chain_permanent,
            checked_at: "t0".to_owned(),
        };
        assert!(matches!(
            assess(
                &plan,
                &evidence(&plan, vec![malformed]),
                "0.0.0",
                sample_digest()
            ),
            Err(AssessDefect::Evidence(_))
        ));
    }
}

#[test]
fn forge_facts_refute_only_after_visibility_and_resolution() {
    let shaped = |name: &str| format!("https://github.com/acme/{name}/blob/main/a.md");
    let observations = ["one", "two", "three"]
        .iter()
        .map(|name| row(Value::Null, external_occurrence("docs/a.md", &shaped(name))))
        .collect();
    let plan =
        plan(&report(observations), "0.0.0", sample_digest()).expect("the report yields a plan");
    let evidence = evidence(
        &plan,
        vec![
            forge_row(
                &shaped("one"),
                ForgeRepository::Readable,
                Some(ForgeTail::PathMissing),
            ),
            forge_row(&shaped("two"), ForgeRepository::Missing, None),
            forge_row(&shaped("three"), ForgeRepository::Readable, None),
        ],
    );
    let assessment =
        assess(&plan, &evidence, "0.0.0", sample_digest()).expect("the pair yields an assessment");
    assert_eq!(
        verdicts_of(&assessment),
        vec![
            (shaped("one"), "refuted".into(), "path-missing".into()),
            (shaped("three"), "unproven".into(), "unconfirmed".into()),
            (shaped("two"), "unproven".into(), "repository-unseen".into()),
        ],
    );
}

#[test]
fn stray_or_repeated_evidence_invalidates_the_assessment() {
    let plan = plan(
        &report(introduced("https://a.example/x")),
        "0.0.0",
        sample_digest(),
    )
    .expect("the report yields a plan");
    for rows in [
        vec![probe("https://other.example/y", ProbeMethod::Get, 200)],
        vec![
            probe("https://a.example/x", ProbeMethod::Get, 200),
            probe("https://a.example/x", ProbeMethod::Head, 200),
        ],
        vec![forge_row(
            "https://a.example/x",
            ForgeRepository::Readable,
            None,
        )],
    ] {
        assert!(matches!(
            assess(&plan, &evidence(&plan, rows), "0.0.0", sample_digest(),),
            Err(AssessDefect::UnboundEvidence)
        ));
    }
    let mut foreign = evidence(&plan, Vec::new());
    foreign.plan_payload_digest = sample_digest();
    assert!(matches!(
        assess(&plan, &foreign, "0.0.0", sample_digest()),
        Err(AssessDefect::UnboundEvidence)
    ));
}

#[test]
fn malformed_evidence_rows_are_refused() {
    let plan = plan(
        &report(introduced("https://a.example/x")),
        "0.0.0",
        sample_digest(),
    )
    .expect("the report yields a plan");
    for (status, failure) in [
        (Some(200), Some(ProbeFailure::Tls)),
        (None, None),
        (Some(42), None),
        (Some(1000), None),
    ] {
        let bad = ExternalEvidenceRow::HttpProbe {
            destination: "https://a.example/x".to_owned(),
            method: ProbeMethod::Get,
            status,
            failure,
            final_destination: None,
            redirect_chain_permanent: None,
            checked_at: "t0".to_owned(),
        };
        assert!(matches!(
            assess(&plan, &evidence(&plan, vec![bad]), "0.0.0", sample_digest()),
            Err(AssessDefect::Evidence(_))
        ));
    }
}

/// The published contract's own bounds hold at the judge too: an empty
/// producer version or a plan row the assessment schema would reject never
/// becomes an artifact.
#[test]
fn the_judge_is_no_laxer_than_its_contracts() {
    let plan = plan(
        &report(introduced("https://a.example/x")),
        "0.0.0",
        sample_digest(),
    )
    .expect("the report yields a plan");
    let mut unnamed = evidence(&plan, Vec::new());
    unnamed.producer.version.clear();
    assert!(matches!(
        assess(&plan, &unnamed, "0.0.0", sample_digest(),),
        Err(AssessDefect::Evidence(_))
    ));

    let mut invalid_plan = plan;
    invalid_plan.payload.introduced[0].documents.clear();
    invalid_plan.payload_digest = hb(
        PLAN_PAYLOAD_SCHEMA,
        &serde_json_canonicalizer::to_vec(&invalid_plan.payload).unwrap(),
    );
    assert!(matches!(
        assess(&invalid_plan, &unnamed, "0.0.0", sample_digest(),),
        Err(AssessDefect::Plan(_))
    ));
}

/// A tail resolution for a bare repository shape is evidence about nothing
/// the plan asked for.
#[test]
fn a_tail_resolution_needs_a_tail_in_the_shape() {
    let bare = "https://github.com/acme/widgets";
    let plan = plan(&report(introduced(bare)), "0.0.0", sample_digest())
        .expect("the report yields a plan");
    assert!(matches!(
        assess(
            &plan,
            &evidence(
                &plan,
                vec![forge_row(
                    bare,
                    ForgeRepository::Readable,
                    Some(ForgeTail::Resolved)
                )]
            ),
            "0.0.0",
            sample_digest()
        ),
        Err(AssessDefect::UnboundEvidence)
    ));
    let visibility_only = assess(
        &plan,
        &evidence(
            &plan,
            vec![forge_row(bare, ForgeRepository::Readable, None)],
        ),
        "0.0.0",
        sample_digest(),
    )
    .expect("visibility-only evidence judges a bare shape");
    assert_eq!(
        verdicts_of(&visibility_only),
        vec![(bare.to_owned(), "reachable".to_owned(), String::new())],
    );
}

#[test]
fn the_assessment_binds_the_whole_chain() {
    let plan = plan(
        &report(introduced("https://a.example/x")),
        "0.0.0",
        sample_digest(),
    )
    .expect("the report yields a plan");
    let rows = vec![probe("https://a.example/x", ProbeMethod::Get, 200)];
    let evidence = evidence(&plan, rows);
    let assessment =
        assess(&plan, &evidence, "0.0.0", sample_digest()).expect("the pair yields an assessment");
    let subject = &assessment.payload.subject;
    assert_eq!(subject.plan_payload_digest, plan.payload_digest);
    assert_eq!(
        subject.evidence_digest,
        hb(
            EVIDENCE_SCHEMA,
            &serde_json_canonicalizer::to_vec(&evidence).unwrap()
        )
    );
    let payload = &assessment.payload;
    assert_eq!(
        assessment.payload_digest,
        hb(
            ASSESSMENT_PAYLOAD_SCHEMA,
            &serde_json_canonicalizer::to_vec(payload).expect("fixture JSON")
        )
    );
}

#[test]
fn an_external_occurrence_missing_its_promise_is_refused() {
    let Value::Object(occurrence) = external_occurrence("docs/a.md", "https://x.example/a") else {
        panic!("the occurrence is an object");
    };
    let mut occurrence = occurrence.into_vec();
    occurrence.retain(|(name, _)| name != "external_destination");
    let source = report(vec![row(Value::Null, Value::object(occurrence))]);
    assert_eq!(
        plan(&source, "0.0.0", sample_digest()),
        Err(PlanDefect::MalformedExternal)
    );
}
