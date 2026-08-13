#![expect(
    clippy::expect_used,
    reason = "test assertions over constructed values"
)]

use amiss_wire::digest::hj;
use amiss_wire::external::{PLAN_ENVELOPE_SCHEMA, PLAN_PAYLOAD_SCHEMA, PlanDefect, plan};
use amiss_wire::json::Value;
use amiss_wire::report::{ENVELOPE_SCHEMA, PAYLOAD_SCHEMA};

fn object(members: Vec<(&str, Value)>) -> Value {
    let mut members: Vec<(String, Value)> = members
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value))
        .collect();
    members.sort_by(|left, right| left.0.cmp(&right.0));
    Value::Object(members)
}

fn string(value: &str) -> Value {
    Value::String(value.to_owned())
}

fn members(value: Value) -> Vec<(String, Value)> {
    if let Value::Object(members) = value {
        Some(members)
    } else {
        None
    }
    .expect("an object value")
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
        Some(items.as_slice())
    } else {
        None
    }
    .expect("an array value")
}

fn text(value: &Value) -> &str {
    if let Value::String(text) = value {
        Some(text.as_str())
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

/// A minimal report holding only what the derivation reads, with a true
/// digest, so every test exercises the same trust path the command does.
fn report(observations: Vec<Value>) -> Value {
    let payload = object(vec![
        ("schema", string(PAYLOAD_SCHEMA)),
        ("result", object(vec![("complete", Value::Bool(true))])),
        (
            "evaluation",
            object(vec![
                ("base", object(vec![("commit_oid", string("a"))])),
                ("candidate", object(vec![("commit_oid", string("b"))])),
                ("mode", string("commit-pair")),
            ]),
        ),
        ("observations", Value::Array(observations)),
    ]);
    let digest = hj(PAYLOAD_SCHEMA, &payload);
    object(vec![
        ("schema", string(ENVELOPE_SCHEMA)),
        ("payload", payload),
        ("payload_digest", string(&digest.to_string())),
    ])
}

fn planned(observations: Vec<Value>) -> Value {
    plan(&report(observations), "0.0.0", &sample_digest()).expect("the report yields a plan")
}

fn sample_digest() -> String {
    hj(PAYLOAD_SCHEMA, &Value::Null).to_string()
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
fn the_envelope_binds_the_source_digest_and_its_own() {
    let source = report(Vec::new());
    let derived = plan(&source, "0.0.0", &sample_digest()).expect("an empty report yields a plan");
    assert_eq!(field(&derived, "schema"), &string(PLAN_ENVELOPE_SCHEMA));
    let payload = field(&derived, "payload");
    let recomputed = hj(PLAN_PAYLOAD_SCHEMA, payload).to_string();
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
fn a_tampered_payload_is_refused() {
    let mut envelope = members(report(Vec::new()));
    for (key, value) in &mut envelope {
        if key == "payload"
            && let Value::Object(payload) = value
        {
            payload.retain(|(name, _)| name != "result");
        }
    }
    assert_eq!(
        plan(&Value::Object(envelope), "0.0.0", &sample_digest()),
        Err(PlanDefect::DigestMismatch)
    );
}

#[test]
fn an_incomplete_report_is_refused() {
    let payload = object(vec![
        ("schema", string(PAYLOAD_SCHEMA)),
        ("result", object(vec![("complete", Value::Bool(false))])),
        ("observations", Value::Array(Vec::new())),
    ]);
    let digest = hj(PAYLOAD_SCHEMA, &payload);
    let envelope = object(vec![
        ("schema", string(ENVELOPE_SCHEMA)),
        ("payload", payload),
        ("payload_digest", string(&digest.to_string())),
    ]);
    assert_eq!(
        plan(&envelope, "0.0.0", &sample_digest()),
        Err(PlanDefect::Incomplete)
    );
}

#[test]
fn a_foreign_value_is_not_a_report() {
    assert_eq!(
        plan(&Value::Null, "0.0.0", &sample_digest()),
        Err(PlanDefect::NotAReport)
    );
    assert_eq!(
        plan(
            &object(vec![("schema", string("amiss/something-else"))]),
            "0.0.0",
            &sample_digest()
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
            "https://gitlab.com/group/sub/widgets",
            r#"{"dialect":"gitlab","host":"gitlab.com","name":"widgets","owner":"group/sub"}"#,
        ),
        (
            "https://codeberg.org/acme/widgets/src/branch/main/a.md",
            r#"{"dialect":"gitea","form":"src","host":"codeberg.org","name":"widgets","owner":"acme","tail":"branch/main/a.md"}"#,
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
            String::from_utf8(amiss_wire::json::canonical(&repository)).expect("canonical utf-8"),
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
    let destination = "https://ghes.corp.example/other/repo/blob/main/x.md";
    let payload = object(vec![
        ("schema", string(PAYLOAD_SCHEMA)),
        ("result", object(vec![("complete", Value::Bool(true))])),
        (
            "evaluation",
            object(vec![
                ("base", object(vec![("commit_oid", string("a"))])),
                ("candidate", object(vec![("commit_oid", string("b"))])),
                ("mode", string("commit-pair")),
                ("forge", string("github")),
                (
                    "repository",
                    object(vec![("host", string("ghes.corp.example"))]),
                ),
            ]),
        ),
        ("observations", Value::Array(introduced(destination))),
    ]);
    let digest = hj(PAYLOAD_SCHEMA, &payload);
    let envelope = object(vec![
        ("schema", string(ENVELOPE_SCHEMA)),
        ("payload", payload),
        ("payload_digest", string(&digest.to_string())),
    ]);
    let derived =
        plan(&envelope, "0.0.0", &sample_digest()).expect("the declared-host report yields a plan");
    let repository = repository_of(&derived, destination).expect("the declared host is shaped");
    assert_eq!(
        String::from_utf8(amiss_wire::json::canonical(&repository)).expect("canonical utf-8"),
        r#"{"dialect":"github","form":"blob","host":"ghes.corp.example","name":"repo","owner":"other","tail":"main/x.md"}"#,
    );
}

#[test]
fn an_external_occurrence_missing_its_promise_is_refused() {
    let mut occurrence = members(external_occurrence("docs/a.md", "https://x.example/a"));
    occurrence.retain(|(name, _)| name != "external_destination");
    let source = report(vec![row(Value::Null, Value::Object(occurrence))]);
    assert_eq!(
        plan(&source, "0.0.0", &sample_digest()),
        Err(PlanDefect::MalformedExternal)
    );
}
