#![cfg(test)]

use amiss_wire::json::{Value, parse};

use super::targets;

#[test]
fn only_unshaped_https_destinations_are_selected_up_to_the_cap() {
    let report = amiss_fixtures::external_report(&[
        "https://a.example/one",
        "https://b.example/two",
        "https://c.example/three",
        "https://github.com/acme/widgets",
        "http://plain.example/insecure",
    ]);
    let parsed = parse(&report).unwrap();
    let engine = parsed
        .get("payload")
        .and_then(|payload| payload.get("engine"))
        .unwrap();
    let plan = amiss_wire::external::plan(
        &parsed,
        engine
            .get("engine_version")
            .and_then(Value::as_str)
            .unwrap(),
        engine.get("engine_digest").and_then(Value::as_str).unwrap(),
    )
    .unwrap();

    let (selected, skipped) = targets(&plan, 64);
    assert_eq!(
        selected,
        vec![
            "https://a.example/one",
            "https://b.example/two",
            "https://c.example/three",
        ],
        "the github row is shaped and the http row is not probeable"
    );
    assert_eq!(skipped, 0);

    let (capped, skipped) = targets(&plan, 2);
    assert_eq!(
        capped,
        vec!["https://a.example/one", "https://b.example/two"],
    );
    assert_eq!(skipped, 1);
}

#[test]
fn an_explicit_null_repository_still_excludes_a_destination_from_probing() {
    let plan = parse(br#"{"payload":{"introduced":[{"destination":"https://a.example/plain","scheme":"https"},{"destination":"https://b.example/shaped","scheme":"https","repository":null}]}}"#).unwrap();
    assert_eq!(targets(&plan, 64), (vec!["https://a.example/plain"], 0));
}
