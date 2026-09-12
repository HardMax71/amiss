#![cfg(test)]

use amiss_wire::external::parse_plan;

use super::targets;

#[test]
fn only_unshaped_https_destinations_are_selected_up_to_the_cap() {
    let report = amiss_fixtures::external_report(&[
        "https://a.example/one",
        "https://b.example/two",
        "https://c.example/three",
        "https://github.com/acme/widgets",
        "http://plain.example/insecure",
    ])
    .unwrap();
    let parsed = serde_json::from_slice::<serde_json::Value>(&report).unwrap();
    let engine = parsed
        .get("payload")
        .and_then(|payload| payload.get("engine"))
        .unwrap();
    let plan = amiss_wire::external::plan(
        &report,
        engine
            .get("engine_version")
            .and_then(serde_json::Value::as_str)
            .unwrap(),
        amiss_wire::model::Digest::from_wire(
            engine
                .get("engine_digest")
                .and_then(serde_json::Value::as_str)
                .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let plan = parse_plan(&plan).unwrap();

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
