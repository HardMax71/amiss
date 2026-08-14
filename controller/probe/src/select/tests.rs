#![cfg(test)]

use amiss_wire::json::parse;

use super::targets;

#[test]
fn only_unshaped_https_destinations_are_selected_up_to_the_cap() {
    let report = amiss_fixtures::external_report(&[
        "https://a.example/one",
        "https://b.example/two",
        "https://c.example/three",
        "https://github.com/acme/widgets",
    ]);
    let parsed = parse(&report).unwrap();
    let engine = parsed
        .member("payload")
        .and_then(|payload| payload.member("engine"))
        .unwrap();
    let plan = amiss_wire::external::plan(
        &parsed,
        engine.text("engine_version").unwrap(),
        engine.text("engine_digest").unwrap(),
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
        "the github row is shaped and belongs to the API verifier"
    );
    assert_eq!(skipped, 0);

    let (capped, skipped) = targets(&plan, 2);
    assert_eq!(capped.len(), 2);
    assert_eq!(skipped, 1);
}
