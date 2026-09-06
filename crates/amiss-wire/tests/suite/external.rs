#![expect(
    clippy::expect_used,
    reason = "test assertions over constructed values"
)]

use amiss_wire::de::ErrorKind;
use amiss_wire::digest::{Digest, hb};
use amiss_wire::external::{
    ASSESSMENT_PAYLOAD_SCHEMA, AssessDefect, AssessmentDefect, EVIDENCE_SCHEMA, ExternalEvidence,
    ExternalEvidenceProducer, ExternalEvidenceRow, ExternalEvidenceSchema, ExternalVerdict,
    PLAN_ENVELOPE_SCHEMA, PLAN_PAYLOAD_SCHEMA, PlanDefect, ProbeMethod, assess, parse_assessment,
    parse_evidence, parse_plan, plan,
};
use amiss_wire::json::Value;
use amiss_wire::report::PAYLOAD_SCHEMA;

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

fn field<'v>(value: &'v Value, name: &str) -> &'v Value {
    if let Value::Object(members) = value {
        members
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value)
    } else {
        None
    }
    .expect("the value holds the field")
}

fn array(value: &Value) -> &[Value] {
    if let Value::Array(items) = value {
        Some(items)
    } else {
        None
    }
    .expect("an array value")
}

fn text(value: &Value) -> &str {
    if let Value::String(text) = value {
        Some(text)
    } else {
        None
    }
    .expect("a string value")
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

fn report(observations: Vec<Value>) -> Value {
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
    amiss_wire::json::parse(&bytes).expect("the completed test report is strict JSON")
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

fn planned(observations: Vec<Value>) -> Value {
    let bytes = plan(
        &serde_json_canonicalizer::to_vec(&report(observations)).expect("fixture JSON"),
        "0.0.0",
        sample_digest(),
    )
    .expect("the report yields a plan");
    amiss_wire::json::parse(&bytes).expect("the plan is strict JSON")
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

fn destinations(planned: &Value, side: &str) -> Vec<(String, Vec<String>)> {
    array(field(field(planned, "payload"), side))
        .iter()
        .map(|row| {
            (
                text(field(row, "destination")).to_owned(),
                array(field(row, "documents"))
                    .iter()
                    .map(|document| text(document).to_owned())
                    .collect(),
            )
        })
        .collect()
}

fn retained(planned: &Value) -> i64 {
    if let Value::Integer(count) = field(field(planned, "payload"), "retained_count") {
        Some(*count)
    } else {
        None
    }
    .expect("a retained count")
}

#[test]
fn the_delta_is_set_wise_and_document_attributed() {
    let plan = planned(vec![
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
    ]);
    assert_eq!(
        destinations(&plan, "introduced"),
        vec![(
            "https://new.example/n".to_owned(),
            vec!["docs/a.md".to_owned(), "docs/b.md".to_owned()]
        )],
    );
    assert_eq!(
        destinations(&plan, "removed"),
        vec![(
            "https://old.example/g".to_owned(),
            vec!["docs/a.md".to_owned()]
        )],
    );
    assert_eq!(retained(&plan), 1);
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
    let plan = planned(observations);
    assert_eq!(destinations(&plan, "introduced"), Vec::new());
    assert_eq!(retained(&plan), 0);
}

/// A destination that only moved between documents is retained, never
/// introduced: membership is repository-wide, attribution is per document.
#[test]
fn a_destination_moving_documents_is_retained() {
    let plan = planned(vec![
        row(
            external_occurrence("docs/a.md", "https://kept.example/k"),
            Value::Null,
        ),
        row(
            Value::Null,
            external_occurrence("docs/b.md", "https://kept.example/k"),
        ),
    ]);
    assert_eq!(destinations(&plan, "introduced"), Vec::new());
    assert_eq!(destinations(&plan, "removed"), Vec::new());
    assert_eq!(retained(&plan), 1);
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
    let introduced_plan = planned(vec![row(Value::Null, historical.clone())]);
    assert_eq!(
        destinations(&introduced_plan, "introduced"),
        vec![(destination.to_owned(), vec!["docs/a.md".to_owned()])]
    );
    let introduced = &array(field(field(&introduced_plan, "payload"), "introduced"))[0];
    assert_eq!(text(field(introduced, "scheme")), "https");
    assert_eq!(
        String::from_utf8(
            serde_json_canonicalizer::to_vec(field(introduced, "repository")).unwrap()
        )
        .expect("canonical utf-8"),
        r#"{"dialect":"github","form":"blob","host":"github.com","name":"widgets","owner":"acme","tail":"0123456789012345678901234567890123456789/docs/a.md"}"#
    );

    let retained_plan = planned(vec![row(historical.clone(), historical.clone())]);
    assert_eq!(destinations(&retained_plan, "introduced"), Vec::new());
    assert_eq!(destinations(&retained_plan, "removed"), Vec::new());
    assert_eq!(retained(&retained_plan), 1);

    let Value::Object(historical) = historical else {
        panic!("the occurrence is an object");
    };
    let mut historical = historical.into_vec();
    historical.retain(|(name, _)| name != "external_destination");
    let source = report(vec![row(Value::Null, Value::object(historical))]);
    assert_eq!(
        plan(
            &serde_json_canonicalizer::to_vec(&source).unwrap(),
            "0.0.0",
            sample_digest()
        ),
        Err(PlanDefect::MalformedExternal)
    );
}

#[test]
fn the_envelope_binds_the_source_digest_and_its_own() {
    let source = report(Vec::new());
    let derived = plan(
        &serde_json_canonicalizer::to_vec(&source).unwrap(),
        "0.0.0",
        sample_digest(),
    )
    .expect("an empty report yields a plan");
    let derived = amiss_wire::json::parse(&derived).expect("the plan is strict JSON");
    assert_eq!(field(&derived, "schema"), &string(PLAN_ENVELOPE_SCHEMA));
    let payload = field(&derived, "payload");
    let recomputed = hb(
        PLAN_PAYLOAD_SCHEMA,
        &serde_json_canonicalizer::to_vec(payload).expect("fixture JSON"),
    )
    .to_string();
    assert_eq!(
        field(&derived, "payload_digest"),
        &string(&recomputed),
        "the plan digest is recomputable from its payload"
    );
    assert_eq!(
        field(field(payload, "report"), "payload_digest"),
        field(&source, "payload_digest"),
        "the plan binds the digest of the report it read"
    );
}

#[test]
fn the_plan_model_reads_the_checked_writer() {
    let written = planned(introduced("https://github.com/acme/widgets"));
    let bytes = serde_json_canonicalizer::to_vec(&written).unwrap();
    let parsed = parse_plan(&bytes).expect("the written plan clears the typed reader");
    assert_eq!(parsed.payload.introduced.len(), 1);
    assert_eq!(
        serde_json_canonicalizer::to_vec(&parsed).expect("the model is serializable"),
        bytes,
    );
}

#[test]
fn plan_snapshot_objects_stay_extensible_but_never_accept_scalars() {
    let written = plan(
        &serde_json_canonicalizer::to_vec(&report(Vec::new())).unwrap(),
        "0.0.0",
        sample_digest(),
    )
    .expect("valid report");
    let document: serde_json::Value = serde_json::from_slice(&written).expect("valid plan");
    for side in ["base", "candidate"] {
        let mut extended = document.clone();
        extended["payload"]["report"][side] = serde_json::json!({
            "future_kind": {"😀": "quoted \" \\ \n", "\u{e000}": [null, true, 42]}
        });
        let bytes = refresh_payload_digest(&mut extended, PLAN_PAYLOAD_SCHEMA);
        let parsed = parse_plan(&bytes).expect("snapshot objects are an open contract");
        assert_eq!(
            serde_json_canonicalizer::to_vec(&parsed).expect("canonical plan"),
            bytes
        );
        for value in [
            serde_json::Value::Null,
            serde_json::json!(true),
            serde_json::json!(42),
            serde_json::json!("snapshot"),
            serde_json::json!([]),
        ] {
            extended["payload"]["report"][side] = value;
            let bytes = refresh_payload_digest(&mut extended, PLAN_PAYLOAD_SCHEMA);
            let error = parse_plan(&bytes).expect_err("a snapshot must be an object");
            assert_eq!(error.kind, ErrorKind::WrongType);
            assert_eq!(error.path, format!("$.payload.report.{side}"));
        }
    }
}

#[test]
fn additive_plan_fields_are_digest_bound_but_inert() {
    let written = planned(Vec::new());
    let mut document = serde_json::to_value(&written).expect("the written plan is JSON");
    document
        .get_mut("payload")
        .and_then(serde_json::Value::as_object_mut)
        .expect("the plan payload is an object")
        .insert("future_fact".to_owned(), serde_json::Value::Bool(true));
    let parsed = parse_plan(&refresh_payload_digest(&mut document, PLAN_PAYLOAD_SCHEMA))
        .expect("an additive field remains compatible");
    assert!(parsed.payload.introduced.is_empty());
}

#[test]
fn known_optional_plan_fields_do_not_accept_null() {
    let written = planned(introduced("https://github.com/acme/widgets/blob/main/a.md"));
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
    let written = planned(introduced("https://example.com/manual"));
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
    let Value::Object(envelope) = report(Vec::new()) else {
        panic!("the report is an object");
    };
    let mut envelope = envelope.into_vec();
    for (key, value) in &mut envelope {
        if key == "payload"
            && let Value::Object(payload) = std::mem::replace(value, Value::Null)
        {
            let mut payload = payload.into_vec();
            payload.retain(|(name, _)| name != "result");
            *value = Value::object(payload);
        }
    }
    assert_eq!(
        plan(
            &serde_json_canonicalizer::to_vec(&Value::object(envelope)).unwrap(),
            "0.0.0",
            sample_digest()
        ),
        Err(PlanDefect::DigestMismatch)
    );
}

#[test]
fn an_incomplete_report_is_refused() {
    let mut document =
        serde_json::to_value(report(Vec::new())).expect("the complete report is JSON");
    document["payload"]["result"]["complete"] = serde_json::Value::Bool(false);
    document["payload"]["result"]["status"] = serde_json::Value::String("incomplete".to_owned());
    document["payload"]["result"]["exit_code"] = serde_json::Value::Number(2.into());
    let envelope = refresh_payload_digest(&mut document, PAYLOAD_SCHEMA);
    assert_eq!(
        plan(&envelope, "0.0.0", sample_digest()),
        Err(PlanDefect::Incomplete)
    );
}

#[test]
fn a_foreign_value_is_not_a_report() {
    assert_eq!(
        plan(b"null", "0.0.0", sample_digest()),
        Err(PlanDefect::NotAReport)
    );
    assert_eq!(
        plan(
            br#"{"schema":"amiss/something-else"}"#,
            "0.0.0",
            sample_digest()
        ),
        Err(PlanDefect::NotAReport)
    );
}

fn repository_of(planned: &Value, destination: &str) -> Option<Value> {
    array(field(field(planned, "payload"), "introduced"))
        .iter()
        .find(|row| text(field(row, "destination")) == destination)
        .and_then(|row| {
            if let Value::Object(members) = row {
                members
                    .iter()
                    .find(|(key, _)| key == "repository")
                    .map(|(_, value)| value.clone())
            } else {
                None
            }
        })
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
        let plan = planned(introduced(destination));
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
        let plan = planned(introduced(destination));
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
        let envelope =
            amiss_wire::json::parse(&refresh_payload_digest(&mut document, PAYLOAD_SCHEMA))
                .expect("the declared-host report is strict JSON");
        let derived = plan(
            &serde_json_canonicalizer::to_vec(&envelope).unwrap(),
            "0.0.0",
            sample_digest(),
        )
        .expect("the declared-host report yields a plan");
        let derived = amiss_wire::json::parse(&derived).expect("the plan is strict JSON");
        let repository = repository_of(&derived, destination).expect("the declared host is shaped");
        assert_eq!(
            String::from_utf8(serde_json_canonicalizer::to_vec(&repository).unwrap())
                .expect("canonical utf-8"),
            expected,
            "{destination}"
        );
    }
}

fn evidence(plan: &Value, rows: Vec<Value>) -> Vec<u8> {
    serde_json_canonicalizer::to_vec(&object(vec![
        ("schema", string(EVIDENCE_SCHEMA)),
        ("plan_payload_digest", field(plan, "payload_digest").clone()),
        (
            "producer",
            object(vec![
                ("name", string("amiss-probe")),
                ("version", string("0.0.0")),
            ]),
        ),
        ("rows", Value::array(rows)),
    ]))
    .expect("fixture JSON")
}

fn probe(destination: &str, method: &str, status: i64) -> Value {
    object(vec![
        ("kind", string("http-probe")),
        ("destination", string(destination)),
        ("method", string(method)),
        ("status", Value::Integer(status)),
        ("checked_at", string("t0")),
    ])
}

fn forge_row(destination: &str, repository: &str, tail: Option<&str>) -> Value {
    let mut members = vec![
        ("kind", string("forge-api")),
        ("destination", string(destination)),
        ("repository", string(repository)),
        ("checked_at", string("t0")),
    ];
    if let Some(tail) = tail {
        members.push(("tail", string(tail)));
    }
    object(members)
}

#[test]
fn additive_evidence_fields_are_inert_but_known_nulls_are_refused() {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../spec/examples/scanner-external-evidence.json"
    ))
    .expect("the evidence example is readable");
    let mut document: serde_json::Value = serde_json::from_slice(&bytes).expect("valid JSON");
    document
        .as_object_mut()
        .expect("the evidence is an object")
        .insert("future_fact".to_owned(), serde_json::Value::Bool(true));
    assert!(
        parse_evidence(&serde_json_canonicalizer::to_vec(&document).expect("canonical JSON"))
            .is_ok()
    );
    document
        .pointer_mut("/rows/0")
        .and_then(serde_json::Value::as_object_mut)
        .expect("the evidence has one row")
        .insert("failure".to_owned(), serde_json::Value::Null);
    assert!(
        parse_evidence(&serde_json_canonicalizer::to_vec(&document).expect("canonical JSON"))
            .is_err()
    );
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
    assert_eq!(
        bytes,
        serde_json_canonicalizer::to_vec(&amiss_wire::json::parse(&bytes).expect("strict JSON"))
            .unwrap(),
    );
    assert!(!bytes.ends_with(b"\n"));
}

#[test]
fn assessment_evidence_bytes_bind_additive_fields_and_ignore_whitespace() {
    let plan = planned(introduced("https://a.example/x"));
    let mut document: serde_json::Value = serde_json::from_slice(&evidence(
        &plan,
        vec![probe("https://a.example/x", "get", 200)],
    ))
    .expect("the evidence is JSON");
    document["future_field"] = serde_json::json!({"😀": "\t", "\u{e000}": null});
    let canonical = serde_json_canonicalizer::to_vec(&document).expect("canonical evidence");
    let pretty = serde_json::to_vec_pretty(&document).expect("formatted evidence");
    let assessment = assess(
        &serde_json_canonicalizer::to_vec(&plan).unwrap(),
        &pretty,
        "0.0.0",
        sample_digest(),
    )
    .expect("valid evidence");
    let assessment = amiss_wire::json::parse(&assessment).expect("the assessment is strict JSON");
    let subject = field(field(&assessment, "payload"), "subject");
    assert_eq!(
        text(field(subject, "evidence_digest")),
        hb(EVIDENCE_SCHEMA, &canonical).to_string(),
    );
    let (typed, _digest) = parse_evidence(&canonical).expect("valid evidence");
    assert_ne!(
        hb(EVIDENCE_SCHEMA, &canonical),
        hb(
            EVIDENCE_SCHEMA,
            &amiss_wire::external::evidence(&typed).expect("typed evidence")
        ),
    );
    let mut trailing = pretty;
    trailing.extend_from_slice(b" null");
    assert!(matches!(
        assess(
            &serde_json_canonicalizer::to_vec(&plan).unwrap(),
            &trailing,
            "0.0.0",
            sample_digest()
        ),
        Err(AssessDefect::Evidence(_)),
    ));
}

#[test]
fn derived_validation_rejects_invalid_evidence_shapes() {
    let row = ExternalEvidenceRow::HttpProbe {
        destination: "https://example.com/a".to_owned(),
        method: ProbeMethod::Get,
        status: Some(42),
        failure: Some(amiss_wire::external::ProbeFailure::Tls),
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

#[test]
fn assessment_fields_are_digest_bound_and_derived_validation_is_complete() {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../spec/examples/scanner-external-assessment.json"
    ))
    .expect("the assessment example is readable");
    let document: serde_json::Value = serde_json::from_slice(&bytes).expect("valid JSON");

    let mut extended = document.clone();
    extended
        .get_mut("payload")
        .and_then(serde_json::Value::as_object_mut)
        .expect("the assessment payload is an object")
        .insert("future_fact".to_owned(), serde_json::Value::Bool(true));
    let extended_bytes = refresh_payload_digest(&mut extended, ASSESSMENT_PAYLOAD_SCHEMA);
    let parsed = parse_assessment(&extended_bytes).expect("an additive field remains compatible");
    assert_eq!(
        parsed.payload.verdicts.first().map(|row| row.verdict),
        Some(ExternalVerdict::Refuted)
    );

    extended
        .pointer_mut("/payload/future_fact")
        .map(|value| *value = serde_json::Value::Bool(false))
        .expect("the additive field is present");
    let tampered = serde_json_canonicalizer::to_vec(&extended).expect("canonical JSON");
    let Err(AssessmentDefect::Wire(error)) = parse_assessment(&tampered) else {
        panic!("changing an additive field must break its digest");
    };
    assert_eq!(error.kind, ErrorKind::DigestMismatch);

    for field in ["reason", "retarget"] {
        let mut null = document.clone();
        null.pointer_mut("/payload/verdicts/0")
            .and_then(serde_json::Value::as_object_mut)
            .expect("the assessment has one verdict")
            .insert(field.to_owned(), serde_json::Value::Null);
        let bytes = refresh_payload_digest(&mut null, ASSESSMENT_PAYLOAD_SCHEMA);
        let defect = parse_assessment(&bytes);
        assert!(
            matches!(defect, Err(AssessmentDefect::Wire(_))),
            "{defect:?}"
        );
    }

    let mut inconsistent = document.clone();
    *inconsistent
        .pointer_mut("/payload/verdicts/0/verdict")
        .expect("the assessment has one verdict") =
        serde_json::Value::String("reachable".to_owned());
    assert!(matches!(
        parse_assessment(&refresh_payload_digest(
            &mut inconsistent,
            ASSESSMENT_PAYLOAD_SCHEMA
        )),
        Err(AssessmentDefect::Contract(_))
    ));

    let mut repeated = document;
    let verdicts = repeated
        .pointer_mut("/payload/verdicts")
        .and_then(serde_json::Value::as_array_mut)
        .expect("the assessment verdicts are an array");
    let mut other = verdicts.first().cloned().expect("one verdict");
    *other
        .pointer_mut("/documents/0")
        .expect("the verdict has one document") =
        serde_json::Value::String("docs/other.md".to_owned());
    verdicts.push(other);
    assert!(matches!(
        parse_assessment(&refresh_payload_digest(
            &mut repeated,
            ASSESSMENT_PAYLOAD_SCHEMA
        )),
        Err(AssessmentDefect::Contract(_))
    ));
}

fn verdicts_of(assessment: &[u8]) -> Vec<(String, String, String)> {
    let assessment = amiss_wire::json::parse(assessment).expect("the assessment is strict JSON");
    array(field(field(&assessment, "payload"), "verdicts"))
        .iter()
        .map(|row| {
            let reason = if let Value::Object(members) = row {
                members
                    .iter()
                    .find(|(key, _)| key == "reason")
                    .map(|(_, value)| text(value).to_owned())
                    .unwrap_or_default()
            } else {
                String::new()
            };
            (
                text(field(row, "destination")).to_owned(),
                text(field(row, "verdict")).to_owned(),
                reason,
            )
        })
        .collect()
}

#[test]
fn the_judgment_policy_is_conservative() {
    let destinations = [
        (
            "https://a.example/gone",
            probe("https://a.example/gone", "get", 410),
        ),
        (
            "https://b.example/head404",
            probe("https://b.example/head404", "head", 404),
        ),
        (
            "https://c.example/ok",
            probe("https://c.example/ok", "head", 200),
        ),
        (
            "https://d.example/wall",
            probe("https://d.example/wall", "get", 403),
        ),
        (
            "https://e.example/limit",
            probe("https://e.example/limit", "get", 429),
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
    let plan = planned(observations);
    let evidence = evidence(
        &plan,
        destinations.iter().map(|(_, row)| row.clone()).collect(),
    );
    let assessment = assess(
        &serde_json_canonicalizer::to_vec(&plan).unwrap(),
        &evidence,
        "0.0.0",
        sample_digest(),
    )
    .expect("the pair yields an assessment");
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
    let plan = planned(vec![
        row(Value::Null, external_occurrence("docs/a.md", permanent)),
        row(Value::Null, external_occurrence("docs/a.md", temporary)),
    ]);
    let observed = evidence(
        &plan,
        vec![
            object(vec![
                ("checked_at", string("t0")),
                ("destination", string(permanent)),
                ("final_destination", string(permanent_target)),
                ("kind", string("http-probe")),
                ("method", string("head")),
                ("redirect_chain_permanent", Value::Bool(true)),
                ("status", Value::Integer(200)),
            ]),
            object(vec![
                ("checked_at", string("t0")),
                ("destination", string(temporary)),
                ("final_destination", string(temporary_target)),
                ("kind", string("http-probe")),
                ("method", string("head")),
                ("status", Value::Integer(200)),
            ]),
        ],
    );
    let assessment = assess(
        &serde_json_canonicalizer::to_vec(&plan).unwrap(),
        &observed,
        "0.0.0",
        sample_digest(),
    )
    .expect("the redirects are evidence");
    let assessment = amiss_wire::json::parse(&assessment).expect("the assessment is strict JSON");
    let verdicts = array(field(field(&assessment, "payload"), "verdicts"));
    let verdict = |destination: &str| {
        verdicts
            .iter()
            .find(|row| row.text("destination") == Some(destination))
            .expect("the plan destination has one verdict")
    };
    assert_eq!(verdict(permanent).text("retarget"), Some(permanent_target));
    assert_eq!(verdict(temporary).text("retarget"), None);

    for malformed in [
        object(vec![
            ("checked_at", string("t0")),
            ("destination", string(permanent)),
            ("kind", string("http-probe")),
            ("method", string("head")),
            ("redirect_chain_permanent", Value::Bool(true)),
            ("status", Value::Integer(200)),
        ]),
        object(vec![
            ("checked_at", string("t0")),
            ("destination", string(permanent)),
            ("final_destination", string(permanent_target)),
            ("kind", string("http-probe")),
            ("method", string("head")),
            ("redirect_chain_permanent", Value::Bool(false)),
            ("status", Value::Integer(200)),
        ]),
    ] {
        assert!(matches!(
            assess(
                &serde_json_canonicalizer::to_vec(&plan).unwrap(),
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
    let plan = planned(observations);
    let evidence = evidence(
        &plan,
        vec![
            forge_row(&shaped("one"), "readable", Some("path-missing")),
            forge_row(&shaped("two"), "missing", None),
            forge_row(&shaped("three"), "readable", None),
        ],
    );
    let assessment = assess(
        &serde_json_canonicalizer::to_vec(&plan).unwrap(),
        &evidence,
        "0.0.0",
        sample_digest(),
    )
    .expect("the pair yields an assessment");
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
    let plan = planned(introduced("https://a.example/x"));
    for rows in [
        vec![probe("https://other.example/y", "get", 200)],
        vec![
            probe("https://a.example/x", "get", 200),
            probe("https://a.example/x", "head", 200),
        ],
        vec![forge_row("https://a.example/x", "readable", None)],
    ] {
        assert!(matches!(
            assess(
                &serde_json_canonicalizer::to_vec(&plan).unwrap(),
                &evidence(&plan, rows),
                "0.0.0",
                sample_digest(),
            ),
            Err(AssessDefect::UnboundEvidence)
        ));
    }
    let Value::Object(members) =
        amiss_wire::json::parse(&evidence(&plan, Vec::new())).expect("the evidence is strict JSON")
    else {
        panic!("the evidence is an object");
    };
    let mut members = members.into_vec();
    members.retain(|(key, _)| key != "plan_payload_digest");
    members.push((
        "plan_payload_digest".to_owned(),
        string(&sample_digest().to_string()),
    ));
    members.sort_by(|left, right| left.0.cmp(&right.0));
    let foreign = serde_json_canonicalizer::to_vec(&Value::object(members)).unwrap();
    assert!(matches!(
        assess(
            &serde_json_canonicalizer::to_vec(&plan).unwrap(),
            &foreign,
            "0.0.0",
            sample_digest()
        ),
        Err(AssessDefect::UnboundEvidence)
    ));
}

#[test]
fn malformed_evidence_rows_are_refused() {
    let plan = planned(introduced("https://a.example/x"));
    let both = object(vec![
        ("kind", string("http-probe")),
        ("destination", string("https://a.example/x")),
        ("method", string("get")),
        ("status", Value::Integer(200)),
        ("failure", string("tls")),
        ("checked_at", string("t0")),
    ]);
    let neither = object(vec![
        ("kind", string("http-probe")),
        ("destination", string("https://a.example/x")),
        ("method", string("get")),
        ("checked_at", string("t0")),
    ]);
    let below = probe("https://a.example/x", "get", 42);
    let above = probe("https://a.example/x", "get", 1000);
    for bad in [both, neither, below, above] {
        assert!(matches!(
            assess(
                &serde_json_canonicalizer::to_vec(&plan).unwrap(),
                &evidence(&plan, vec![bad]),
                "0.0.0",
                sample_digest()
            ),
            Err(AssessDefect::Evidence(_))
        ));
    }
}

/// The published contract's own bounds hold at the judge too: an empty
/// producer version or a plan row the assessment schema would reject never
/// becomes an artifact.
#[test]
fn the_judge_is_no_laxer_than_its_contracts() {
    let plan = planned(introduced("https://a.example/x"));
    let Value::Object(unnamed) =
        amiss_wire::json::parse(&evidence(&plan, Vec::new())).expect("the evidence is strict JSON")
    else {
        panic!("the evidence is an object");
    };
    let mut unnamed = unnamed.into_vec();
    for (key, value) in &mut unnamed {
        if key == "producer" {
            *value = object(vec![("name", string("p")), ("version", string(""))]);
        }
    }
    assert!(matches!(
        assess(
            &serde_json_canonicalizer::to_vec(&plan).unwrap(),
            &serde_json_canonicalizer::to_vec(&Value::object(unnamed)).unwrap(),
            "0.0.0",
            sample_digest(),
        ),
        Err(AssessDefect::Evidence(_))
    ));

    let broken_row = object(vec![
        ("destination", string("https://a.example/x")),
        ("scheme", string("https")),
    ]);
    let payload = object(vec![
        ("schema", string(PLAN_PAYLOAD_SCHEMA)),
        (
            "report",
            object(vec![(
                "payload_digest",
                string(&sample_digest().to_string()),
            )]),
        ),
        ("introduced", Value::array(vec![broken_row])),
        ("removed", Value::array(Vec::new())),
        ("retained_count", Value::Integer(0)),
    ]);
    let digest = hb(
        PLAN_PAYLOAD_SCHEMA,
        &serde_json_canonicalizer::to_vec(&payload).expect("fixture JSON"),
    );
    let handcrafted = object(vec![
        ("schema", string(PLAN_ENVELOPE_SCHEMA)),
        ("payload", payload),
        ("payload_digest", string(&digest.to_string())),
    ]);
    let empty = evidence(&handcrafted, Vec::new());
    assert!(matches!(
        assess(
            &serde_json_canonicalizer::to_vec(&handcrafted).unwrap(),
            &empty,
            "0.0.0",
            sample_digest(),
        ),
        Err(AssessDefect::Plan(_))
    ));
}

/// A tail resolution for a bare repository shape is evidence about nothing
/// the plan asked for.
#[test]
fn a_tail_resolution_needs_a_tail_in_the_shape() {
    let bare = "https://github.com/acme/widgets";
    let plan = planned(introduced(bare));
    assert!(matches!(
        assess(
            &serde_json_canonicalizer::to_vec(&plan).unwrap(),
            &evidence(&plan, vec![forge_row(bare, "readable", Some("resolved"))]),
            "0.0.0",
            sample_digest()
        ),
        Err(AssessDefect::UnboundEvidence)
    ));
    let visibility_only = assess(
        &serde_json_canonicalizer::to_vec(&plan).unwrap(),
        &evidence(&plan, vec![forge_row(bare, "readable", None)]),
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
    let plan = planned(introduced("https://a.example/x"));
    let rows = vec![probe("https://a.example/x", "get", 200)];
    let evidence = evidence(&plan, rows);
    let assessment = assess(
        &serde_json_canonicalizer::to_vec(&plan).unwrap(),
        &evidence,
        "0.0.0",
        sample_digest(),
    )
    .expect("the pair yields an assessment");
    let assessment = amiss_wire::json::parse(&assessment).expect("the assessment is strict JSON");
    let subject = field(field(&assessment, "payload"), "subject");
    assert_eq!(
        field(subject, "plan_payload_digest"),
        field(&plan, "payload_digest")
    );
    assert_eq!(
        text(field(subject, "evidence_digest")),
        hb(EVIDENCE_SCHEMA, &evidence).to_string()
    );
    let payload = field(&assessment, "payload");
    assert_eq!(
        text(field(&assessment, "payload_digest")),
        hb(
            "amiss/external-assessment-payload",
            &serde_json_canonicalizer::to_vec(payload).expect("fixture JSON")
        )
        .to_string()
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
        plan(
            &serde_json_canonicalizer::to_vec(&source).unwrap(),
            "0.0.0",
            sample_digest()
        ),
        Err(PlanDefect::MalformedExternal)
    );
}
