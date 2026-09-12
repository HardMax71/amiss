#![expect(
    clippy::expect_used,
    reason = "test assertions over constructed values"
)]

use amiss_wire::de::ErrorKind;
use amiss_wire::external::{
    ASSESSMENT_PAYLOAD_SCHEMA, AssessDefect, AssessmentDefect, EVIDENCE_SCHEMA, ExternalEvidence,
    ExternalEvidenceProducer, ExternalEvidenceRow, ExternalEvidenceSchema, ExternalVerdict,
    PLAN_ENVELOPE_SCHEMA, PLAN_PAYLOAD_SCHEMA, PlanDefect, ProbeMethod, assess, parse_assessment,
    parse_evidence, parse_plan, plan,
};
use amiss_wire::model::Digest;
use amiss_wire::report::PAYLOAD_SCHEMA;
use serde_json::Value;
use sha2::Digest as _;

const REPORT: &[u8] = include_bytes!("../../../../spec/examples/scanner-report.canonical.json");

fn external_occurrence(document: &str, destination: &str) -> Value {
    Value::from_iter(vec![
        ("document", Value::from(document)),
        ("external_destination", Value::from(destination)),
        (
            "intent",
            Value::from_iter(vec![("external_scheme", Value::from("https"))]),
        ),
        (
            "resolution",
            Value::from_iter(vec![
                ("kind", Value::from("external")),
                ("reason", Value::from("url")),
            ]),
        ),
    ])
}

fn row(base: Value, candidate: Value) -> Value {
    Value::from_iter(vec![("base", base), ("candidate", candidate)])
}

fn report(observations: Vec<Value>) -> Value {
    let mut document: Value =
        serde_json::from_slice(REPORT).expect("the report example is valid JSON");
    let examples = document
        .pointer("/payload/observations")
        .and_then(Value::as_array)
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
            let supplied = row;
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
        .expect("the report example has observations") = Value::Array(rows);
    let digest = Digest::from(
        sha2::Sha256::new_with_prefix(PAYLOAD_SCHEMA)
            .chain_update([0_u8])
            .chain_update(
                serde_json_canonicalizer::to_vec(document.get("payload").expect("fixture payload"))
                    .expect("canonical payload"),
            )
            .finalize()
            .0,
    );
    *document.get_mut("payload_digest").expect("fixture digest") = Value::from(digest.to_string());
    document
}

fn expand_occurrence(supplied: &Value, resolved: &Value, external: &Value) -> Value {
    if supplied.is_null() {
        return Value::Null;
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
    if let Some(intent) = supplied.get("intent").and_then(Value::as_object) {
        let target = occurrence_object
            .get_mut("intent")
            .and_then(Value::as_object_mut)
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
    serde_json::from_slice::<Value>(&bytes).expect("the plan is strict JSON")
}

fn sample_digest() -> Digest {
    Digest::from(
        sha2::Sha256::new_with_prefix(PAYLOAD_SCHEMA)
            .chain_update([0_u8])
            .chain_update(b"null")
            .finalize()
            .0,
    )
}

fn refresh_payload_digest(document: &mut Value, domain: &str) -> Vec<u8> {
    let payload = document
        .get("payload")
        .expect("the plan document holds its payload");
    let canonical = serde_json_canonicalizer::to_vec(payload)
        .expect("the plan payload is canonically serializable");
    let digest = Digest::from(
        sha2::Sha256::new_with_prefix(domain)
            .chain_update([0_u8])
            .chain_update(&canonical)
            .finalize()
            .0,
    )
    .to_string();
    let recorded = document
        .get_mut("payload_digest")
        .expect("the plan document holds its digest");
    *recorded = Value::String(digest);
    serde_json_canonicalizer::to_vec(document).expect("the plan document is serializable")
}

fn destinations(planned: &Value, side: &str) -> Vec<(String, Vec<String>)> {
    (((planned).get("payload").expect("fixture field"))
        .get(side)
        .expect("fixture field"))
    .as_array()
    .expect("fixture array")
    .iter()
    .map(|row| {
        (
            ((row).get("destination").expect("fixture field"))
                .as_str()
                .expect("fixture text")
                .to_owned(),
            ((row).get("documents").expect("fixture field"))
                .as_array()
                .expect("fixture array")
                .iter()
                .map(|document| (document).as_str().expect("fixture text").to_owned())
                .collect(),
        )
    })
    .collect()
}

fn retained(planned: &Value) -> i64 {
    planned
        .get("payload")
        .and_then(|payload| payload.get("retained_count"))
        .and_then(Value::as_i64)
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
        row(
            serde_json::json!({"document": "docs/a.md", "resolution": {"kind": "resolved"}}),
            Value::Null,
        ),
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
            Value::from_iter(vec![
                ("document", Value::from(document)),
                (
                    "resolution",
                    Value::from_iter(vec![
                        ("kind", Value::from("external")),
                        ("reason", Value::from(reason)),
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
    let historical = Value::from_iter(vec![
        ("document", Value::from("docs/a.md")),
        ("external_destination", Value::from(destination)),
        ("intent", serde_json::json!({})),
        (
            "resolution",
            Value::from_iter(vec![
                ("kind", Value::from("unsupported-version")),
                (
                    "scope",
                    Value::from_iter(vec![
                        ("kind", Value::from("known-commit")),
                        (
                            "commit_oid",
                            Value::from("0123456789012345678901234567890123456789"),
                        ),
                        ("path", Value::from("docs/a.md")),
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
    let introduced = &(((introduced_plan).get("payload").expect("fixture field"))
        .get("introduced")
        .expect("fixture field"))
    .as_array()
    .expect("fixture array")[0];
    assert_eq!(
        ((introduced).get("scheme").expect("fixture field"))
            .as_str()
            .expect("fixture text"),
        "https"
    );
    assert_eq!(
        String::from_utf8(
            serde_json_canonicalizer::to_vec(
                (introduced).get("repository").expect("fixture field")
            )
            .unwrap()
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
    let mut historical = historical;
    historical.remove("external_destination");
    let source = report(vec![row(Value::Null, Value::from_iter(historical))]);
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
    let derived = serde_json::from_slice::<Value>(&derived).expect("the plan is strict JSON");
    assert_eq!(
        (derived).get("schema").expect("fixture field"),
        &Value::from(PLAN_ENVELOPE_SCHEMA)
    );
    let payload = (derived).get("payload").expect("fixture field");
    let recomputed = Digest::from(
        sha2::Sha256::new_with_prefix(PLAN_PAYLOAD_SCHEMA)
            .chain_update([0_u8])
            .chain_update(serde_json_canonicalizer::to_vec(payload).expect("fixture JSON"))
            .finalize()
            .0,
    )
    .to_string();
    assert_eq!(
        (derived).get("payload_digest").expect("fixture field"),
        &Value::from(recomputed),
        "the plan digest is recomputable from its payload"
    );
    assert_eq!(
        ((payload).get("report").expect("fixture field"))
            .get("payload_digest")
            .expect("fixture field"),
        (source).get("payload_digest").expect("fixture field"),
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
    let document: Value = serde_json::from_slice(&written).expect("valid plan");
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
            Value::Null,
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
    let mut document = written;
    document
        .get_mut("payload")
        .and_then(Value::as_object_mut)
        .expect("the plan payload is an object")
        .insert("future_fact".to_owned(), Value::Bool(true));
    let parsed = parse_plan(&refresh_payload_digest(&mut document, PLAN_PAYLOAD_SCHEMA))
        .expect("an additive field remains compatible");
    assert!(parsed.payload.introduced.is_empty());
}

#[test]
fn known_optional_plan_fields_do_not_accept_null() {
    let written = planned(introduced("https://github.com/acme/widgets/blob/main/a.md"));
    let mut document = written;
    let repository = document
        .pointer_mut("/payload/introduced/0/repository")
        .and_then(Value::as_object_mut)
        .expect("the introduced destination has a repository shape");
    repository.insert("form".to_owned(), Value::Null);
    let error =
        parse_plan(&refresh_payload_digest(&mut document, PLAN_PAYLOAD_SCHEMA)).unwrap_err();
    assert_eq!(error.kind, ErrorKind::WrongType);
    assert_eq!(error.path, "$.payload.introduced[0].repository.form");
}

#[test]
fn malformed_known_plan_fields_are_refused_after_binding() {
    let written = planned(introduced("https://example.com/manual"));
    let mut document = written;
    let destination = document
        .pointer_mut("/payload/introduced/0/destination")
        .expect("the introduced row holds a destination");
    *destination = Value::String(String::new());
    let error =
        parse_plan(&refresh_payload_digest(&mut document, PLAN_PAYLOAD_SCHEMA)).unwrap_err();
    assert_eq!(error.kind, ErrorKind::InvalidValue);
    assert_eq!(error.path, "$.payload.introduced[0].destination");
}

#[test]
fn a_tampered_payload_is_refused() {
    let mut envelope: amiss_wire::report::model::ReportEnvelope =
        serde_json::from_slice(REPORT).unwrap();
    envelope.payload.result.finding_count += 1;
    let wire = String::from_utf8(serde_json_canonicalizer::to_vec(&envelope).unwrap()).unwrap();
    assert_eq!(
        plan(wire.as_bytes(), "0.0.0", sample_digest()),
        Err(PlanDefect::DigestMismatch)
    );
    let result = serde_json::to_string(&envelope.payload.result).unwrap();
    let malformed = wire.replace(&format!("\"result\":{result}"), "\"result\":null");
    assert_ne!(malformed, wire);
    assert_eq!(
        plan(malformed.as_bytes(), "0.0.0", sample_digest()),
        Err(PlanDefect::NotAReport)
    );
}

#[test]
fn an_incomplete_report_is_refused() {
    let mut document = report(Vec::new());
    document["payload"]["result"]["complete"] = Value::Bool(false);
    document["payload"]["result"]["status"] = Value::String("incomplete".to_owned());
    document["payload"]["result"]["exit_code"] = Value::Number(2.into());
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
    (((planned).get("payload").expect("fixture field"))
        .get("introduced")
        .expect("fixture field"))
    .as_array()
    .expect("fixture array")
    .iter()
    .find(|row| {
        ((row).get("destination").expect("fixture field"))
            .as_str()
            .expect("fixture text")
            == destination
    })
    .and_then(|row| row.get("repository").cloned())
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
        let mut document: Value = serde_json::from_slice(
            &serde_json_canonicalizer::to_vec(&report(introduced(destination))).unwrap(),
        )
        .expect("the complete report is JSON");
        document["payload"]["evaluation"]["forge"] = Value::String(dialect.to_owned());
        document["payload"]["evaluation"]["repository"]["host"] = Value::String(host.to_owned());
        let envelope =
            serde_json::from_slice::<Value>(&refresh_payload_digest(&mut document, PAYLOAD_SCHEMA))
                .expect("the declared-host report is strict JSON");
        let derived = plan(
            &serde_json_canonicalizer::to_vec(&envelope).unwrap(),
            "0.0.0",
            sample_digest(),
        )
        .expect("the declared-host report yields a plan");
        let derived = serde_json::from_slice::<Value>(&derived).expect("the plan is strict JSON");
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
    serde_json_canonicalizer::to_vec(&Value::from_iter(vec![
        ("schema", Value::from(EVIDENCE_SCHEMA)),
        (
            "plan_payload_digest",
            (plan).get("payload_digest").expect("fixture field").clone(),
        ),
        (
            "producer",
            Value::from_iter(vec![
                ("name", Value::from("amiss-probe")),
                ("version", Value::from("0.0.0")),
            ]),
        ),
        ("rows", Value::Array(rows)),
    ]))
    .expect("fixture JSON")
}

fn probe(destination: &str, method: &str, status: i64) -> Value {
    Value::from_iter(vec![
        ("kind", Value::from("http-probe")),
        ("destination", Value::from(destination)),
        ("method", Value::from(method)),
        ("status", Value::from(status)),
        ("checked_at", Value::from("t0")),
    ])
}

fn forge_row(destination: &str, repository: &str, tail: Option<&str>) -> Value {
    let mut members = vec![
        ("kind", Value::from("forge-api")),
        ("destination", Value::from(destination)),
        ("repository", Value::from(repository)),
        ("checked_at", Value::from("t0")),
    ];
    if let Some(tail) = tail {
        members.push(("tail", Value::from(tail)));
    }
    Value::from_iter(members)
}

#[test]
fn additive_evidence_fields_are_inert_but_known_nulls_are_refused() {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../spec/examples/scanner-external-evidence.json"
    ))
    .expect("the evidence example is readable");
    let mut document: Value = serde_json::from_slice(&bytes).expect("valid JSON");
    document
        .as_object_mut()
        .expect("the evidence is an object")
        .insert("future_fact".to_owned(), Value::Bool(true));
    assert!(
        parse_evidence(&serde_json_canonicalizer::to_vec(&document).expect("canonical JSON"))
            .is_ok()
    );
    document
        .pointer_mut("/rows/0")
        .and_then(Value::as_object_mut)
        .expect("the evidence has one row")
        .insert("failure".to_owned(), Value::Null);
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
        serde_json_canonicalizer::to_vec(
            &serde_json::from_slice::<Value>(&bytes).expect("strict JSON")
        )
        .unwrap(),
    );
    assert!(!bytes.ends_with(b"\n"));
}

#[test]
fn assessment_evidence_bytes_bind_additive_fields_and_ignore_whitespace() {
    let plan = planned(introduced("https://a.example/x"));
    let mut document: Value = serde_json::from_slice(&evidence(
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
    let assessment =
        serde_json::from_slice::<Value>(&assessment).expect("the assessment is strict JSON");
    let subject = ((assessment).get("payload").expect("fixture field"))
        .get("subject")
        .expect("fixture field");
    assert_eq!(
        ((subject).get("evidence_digest").expect("fixture field"))
            .as_str()
            .expect("fixture text"),
        Digest::from(
            sha2::Sha256::new_with_prefix(EVIDENCE_SCHEMA)
                .chain_update([0_u8])
                .chain_update(&canonical)
                .finalize()
                .0
        )
        .to_string(),
    );
    let (typed, _digest) = parse_evidence(&canonical).expect("valid evidence");
    assert_ne!(
        Digest::from(
            sha2::Sha256::new_with_prefix(EVIDENCE_SCHEMA)
                .chain_update([0_u8])
                .chain_update(&canonical)
                .finalize()
                .0
        ),
        Digest::from(
            sha2::Sha256::new_with_prefix(EVIDENCE_SCHEMA)
                .chain_update([0_u8])
                .chain_update(amiss_wire::external::evidence(&typed).expect("typed evidence"))
                .finalize()
                .0
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
    let document: Value = serde_json::from_slice(&bytes).expect("valid JSON");

    let mut extended = document.clone();
    extended
        .get_mut("payload")
        .and_then(Value::as_object_mut)
        .expect("the assessment payload is an object")
        .insert("future_fact".to_owned(), Value::Bool(true));
    let extended_bytes = refresh_payload_digest(&mut extended, ASSESSMENT_PAYLOAD_SCHEMA);
    let parsed = parse_assessment(&extended_bytes).expect("an additive field remains compatible");
    assert_eq!(
        parsed.payload.verdicts.first().map(|row| row.verdict),
        Some(ExternalVerdict::Refuted)
    );

    extended
        .pointer_mut("/payload/future_fact")
        .map(|value| *value = Value::Bool(false))
        .expect("the additive field is present");
    let tampered = serde_json_canonicalizer::to_vec(&extended).expect("canonical JSON");
    let Err(AssessmentDefect::Wire(error)) = parse_assessment(&tampered) else {
        panic!("changing an additive field must break its digest");
    };
    assert_eq!(error.kind, ErrorKind::DigestMismatch);

    for field in ["reason", "retarget"] {
        let mut null = document.clone();
        null.pointer_mut("/payload/verdicts/0")
            .and_then(Value::as_object_mut)
            .expect("the assessment has one verdict")
            .insert(field.to_owned(), Value::Null);
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
        .expect("the assessment has one verdict") = Value::String("reachable".to_owned());
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
        .and_then(Value::as_array_mut)
        .expect("the assessment verdicts are an array");
    let mut other = verdicts.first().cloned().expect("one verdict");
    *other
        .pointer_mut("/documents/0")
        .expect("the verdict has one document") = Value::String("docs/other.md".to_owned());
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
    let assessment =
        serde_json::from_slice::<Value>(assessment).expect("the assessment is strict JSON");
    (((assessment).get("payload").expect("fixture field"))
        .get("verdicts")
        .expect("fixture field"))
    .as_array()
    .expect("fixture array")
    .iter()
    .map(|row| {
        let reason = row
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        (
            ((row).get("destination").expect("fixture field"))
                .as_str()
                .expect("fixture text")
                .to_owned(),
            ((row).get("verdict").expect("fixture field"))
                .as_str()
                .expect("fixture text")
                .to_owned(),
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
            Value::from_iter(vec![
                ("checked_at", Value::from("t0")),
                ("destination", Value::from(permanent)),
                ("final_destination", Value::from(permanent_target)),
                ("kind", Value::from("http-probe")),
                ("method", Value::from("head")),
                ("redirect_chain_permanent", Value::Bool(true)),
                ("status", Value::from(200)),
            ]),
            Value::from_iter(vec![
                ("checked_at", Value::from("t0")),
                ("destination", Value::from(temporary)),
                ("final_destination", Value::from(temporary_target)),
                ("kind", Value::from("http-probe")),
                ("method", Value::from("head")),
                ("status", Value::from(200)),
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
    let assessment =
        serde_json::from_slice::<Value>(&assessment).expect("the assessment is strict JSON");
    let verdicts = (((assessment).get("payload").expect("fixture field"))
        .get("verdicts")
        .expect("fixture field"))
    .as_array()
    .expect("fixture array");
    let verdict = |destination: &str| {
        verdicts
            .iter()
            .find(|row| row.get("destination").and_then(Value::as_str) == Some(destination))
            .expect("the plan destination has one verdict")
    };
    assert_eq!(
        verdict(permanent).get("retarget").and_then(Value::as_str),
        Some(permanent_target)
    );
    assert_eq!(
        verdict(temporary).get("retarget").and_then(Value::as_str),
        None
    );

    for malformed in [
        Value::from_iter(vec![
            ("checked_at", Value::from("t0")),
            ("destination", Value::from(permanent)),
            ("kind", Value::from("http-probe")),
            ("method", Value::from("head")),
            ("redirect_chain_permanent", Value::Bool(true)),
            ("status", Value::from(200)),
        ]),
        Value::from_iter(vec![
            ("checked_at", Value::from("t0")),
            ("destination", Value::from(permanent)),
            ("final_destination", Value::from(permanent_target)),
            ("kind", Value::from("http-probe")),
            ("method", Value::from("head")),
            ("redirect_chain_permanent", Value::Bool(false)),
            ("status", Value::from(200)),
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
    let Value::Object(members) = serde_json::from_slice::<Value>(&evidence(&plan, Vec::new()))
        .expect("the evidence is strict JSON")
    else {
        panic!("the evidence is an object");
    };
    let mut members = members;
    members.insert(
        "plan_payload_digest".to_owned(),
        Value::from(sample_digest().to_string()),
    );
    let foreign = serde_json_canonicalizer::to_vec(&Value::from_iter(members)).unwrap();
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
    let both = Value::from_iter(vec![
        ("kind", Value::from("http-probe")),
        ("destination", Value::from("https://a.example/x")),
        ("method", Value::from("get")),
        ("status", Value::from(200)),
        ("failure", Value::from("tls")),
        ("checked_at", Value::from("t0")),
    ]);
    let neither = Value::from_iter(vec![
        ("kind", Value::from("http-probe")),
        ("destination", Value::from("https://a.example/x")),
        ("method", Value::from("get")),
        ("checked_at", Value::from("t0")),
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
    let Value::Object(unnamed) = serde_json::from_slice::<Value>(&evidence(&plan, Vec::new()))
        .expect("the evidence is strict JSON")
    else {
        panic!("the evidence is an object");
    };
    let mut unnamed = unnamed;
    for (key, value) in &mut unnamed {
        if key == "producer" {
            *value = Value::from_iter(vec![
                ("name", Value::from("p")),
                ("version", Value::from("")),
            ]);
        }
    }
    assert!(matches!(
        assess(
            &serde_json_canonicalizer::to_vec(&plan).unwrap(),
            &serde_json_canonicalizer::to_vec(&Value::from_iter(unnamed)).unwrap(),
            "0.0.0",
            sample_digest(),
        ),
        Err(AssessDefect::Evidence(_))
    ));

    let broken_row = Value::from_iter(vec![
        ("destination", Value::from("https://a.example/x")),
        ("scheme", Value::from("https")),
    ]);
    let payload = Value::from_iter(vec![
        ("schema", Value::from(PLAN_PAYLOAD_SCHEMA)),
        (
            "report",
            Value::from_iter(vec![(
                "payload_digest",
                Value::from(sample_digest().to_string()),
            )]),
        ),
        ("introduced", Value::Array(vec![broken_row])),
        ("removed", Value::Array(Vec::new())),
        ("retained_count", Value::from(0)),
    ]);
    let digest = Digest::from(
        sha2::Sha256::new_with_prefix(PLAN_PAYLOAD_SCHEMA)
            .chain_update([0_u8])
            .chain_update(serde_json_canonicalizer::to_vec(&payload).expect("fixture JSON"))
            .finalize()
            .0,
    );
    let handcrafted = Value::from_iter(vec![
        ("schema", Value::from(PLAN_ENVELOPE_SCHEMA)),
        ("payload", payload),
        ("payload_digest", Value::from(digest.to_string())),
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
    let assessment =
        serde_json::from_slice::<Value>(&assessment).expect("the assessment is strict JSON");
    let subject = ((assessment).get("payload").expect("fixture field"))
        .get("subject")
        .expect("fixture field");
    assert_eq!(
        (subject).get("plan_payload_digest").expect("fixture field"),
        (plan).get("payload_digest").expect("fixture field")
    );
    assert_eq!(
        ((subject).get("evidence_digest").expect("fixture field"))
            .as_str()
            .expect("fixture text"),
        Digest::from(
            sha2::Sha256::new_with_prefix(EVIDENCE_SCHEMA)
                .chain_update([0_u8])
                .chain_update(&evidence)
                .finalize()
                .0
        )
        .to_string()
    );
    let payload = (assessment).get("payload").expect("fixture field");
    assert_eq!(
        ((assessment).get("payload_digest").expect("fixture field"))
            .as_str()
            .expect("fixture text"),
        Digest::from(
            sha2::Sha256::new_with_prefix("amiss/external-assessment-payload")
                .chain_update([0_u8])
                .chain_update(serde_json_canonicalizer::to_vec(payload).expect("fixture JSON"))
                .finalize()
                .0
        )
        .to_string()
    );
}

#[test]
fn an_external_occurrence_missing_its_promise_is_refused() {
    let Value::Object(occurrence) = external_occurrence("docs/a.md", "https://x.example/a") else {
        panic!("the occurrence is an object");
    };
    let mut occurrence = occurrence;
    occurrence.remove("external_destination");
    let source = report(vec![row(Value::Null, Value::from_iter(occurrence))]);
    assert_eq!(
        plan(
            &serde_json_canonicalizer::to_vec(&source).unwrap(),
            "0.0.0",
            sample_digest()
        ),
        Err(PlanDefect::MalformedExternal)
    );
}
