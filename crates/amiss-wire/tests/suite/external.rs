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

use amiss_wire::external::{AssessDefect, EVIDENCE_SCHEMA, assess};

fn evidence(plan: &Value, rows: Vec<Value>) -> Value {
    object(vec![
        ("schema", string(EVIDENCE_SCHEMA)),
        ("plan_payload_digest", field(plan, "payload_digest").clone()),
        (
            "producer",
            object(vec![
                ("name", string("amiss-probe")),
                ("version", string("0.0.0")),
            ]),
        ),
        ("rows", Value::Array(rows)),
    ])
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

fn verdicts_of(assessment: &Value) -> Vec<(String, String, String)> {
    array(field(field(assessment, "payload"), "verdicts"))
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
    let assessment =
        assess(&plan, &evidence, "0.0.0", &sample_digest()).expect("the pair yields an assessment");
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
    let assessment =
        assess(&plan, &evidence, "0.0.0", &sample_digest()).expect("the pair yields an assessment");
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
        assert_eq!(
            assess(&plan, &evidence(&plan, rows), "0.0.0", &sample_digest()),
            Err(AssessDefect::UnboundEvidence)
        );
    }
    let mut foreign = evidence(&plan, Vec::new());
    if let Value::Object(members) = &mut foreign {
        members.retain(|(key, _)| key != "plan_payload_digest");
        members.push(("plan_payload_digest".to_owned(), string(&sample_digest())));
        members.sort_by(|left, right| left.0.cmp(&right.0));
    }
    assert_eq!(
        assess(&plan, &foreign, "0.0.0", &sample_digest()),
        Err(AssessDefect::UnboundEvidence)
    );
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
        assert_eq!(
            assess(
                &plan,
                &evidence(&plan, vec![bad]),
                "0.0.0",
                &sample_digest()
            ),
            Err(AssessDefect::MalformedEvidence)
        );
    }
}

/// The published contract's own bounds hold at the judge too: an empty
/// producer version or a plan row the assessment schema would reject never
/// becomes an artifact.
#[test]
fn the_judge_is_no_laxer_than_its_contracts() {
    let plan = planned(introduced("https://a.example/x"));
    let mut unnamed = members(evidence(&plan, Vec::new()));
    for (key, value) in &mut unnamed {
        if key == "producer" {
            *value = object(vec![("name", string("p")), ("version", string(""))]);
        }
    }
    assert_eq!(
        assess(&plan, &Value::Object(unnamed), "0.0.0", &sample_digest()),
        Err(AssessDefect::NotEvidence)
    );

    let broken_row = object(vec![
        ("destination", string("https://a.example/x")),
        ("scheme", string("https")),
    ]);
    let payload = object(vec![
        ("schema", string(PLAN_PAYLOAD_SCHEMA)),
        (
            "report",
            object(vec![("payload_digest", string(&sample_digest()))]),
        ),
        ("introduced", Value::Array(vec![broken_row])),
        ("removed", Value::Array(Vec::new())),
        ("retained_count", Value::Integer(0)),
    ]);
    let digest = hj(PLAN_PAYLOAD_SCHEMA, &payload);
    let handcrafted = object(vec![
        ("schema", string(PLAN_ENVELOPE_SCHEMA)),
        ("payload", payload),
        ("payload_digest", string(&digest.to_string())),
    ]);
    let empty = evidence(&handcrafted, Vec::new());
    assert_eq!(
        assess(&handcrafted, &empty, "0.0.0", &sample_digest()),
        Err(AssessDefect::NotAPlan),
        "a digest-valid plan with rows the contract rejects is not a plan"
    );
}

/// A tail resolution for a bare repository shape is evidence about nothing
/// the plan asked for.
#[test]
fn a_tail_resolution_needs_a_tail_in_the_shape() {
    let bare = "https://github.com/acme/widgets";
    let plan = planned(introduced(bare));
    assert_eq!(
        assess(
            &plan,
            &evidence(&plan, vec![forge_row(bare, "readable", Some("resolved"))]),
            "0.0.0",
            &sample_digest()
        ),
        Err(AssessDefect::UnboundEvidence)
    );
    let visibility_only = assess(
        &plan,
        &evidence(&plan, vec![forge_row(bare, "readable", None)]),
        "0.0.0",
        &sample_digest(),
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
    let assessment =
        assess(&plan, &evidence, "0.0.0", &sample_digest()).expect("the pair yields an assessment");
    let subject = field(field(&assessment, "payload"), "subject");
    assert_eq!(
        field(subject, "plan_payload_digest"),
        field(&plan, "payload_digest")
    );
    assert_eq!(
        text(field(subject, "evidence_digest")),
        hj(EVIDENCE_SCHEMA, &evidence).to_string()
    );
    let payload = field(&assessment, "payload");
    assert_eq!(
        text(field(&assessment, "payload_digest")),
        hj("amiss/external-assessment-payload", payload).to_string()
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
