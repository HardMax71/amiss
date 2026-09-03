#![cfg(test)]

use amiss_wire::external::parse_plan;
use amiss_wire::json::{canonical, parse};

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
        .member("payload")
        .and_then(|payload| payload.member("engine"))
        .unwrap();
    let plan = amiss_wire::external::plan(
        &parsed,
        engine.text("engine_version").unwrap(),
        amiss_wire::digest::Digest::from_wire(engine.text("engine_digest").unwrap()).unwrap(),
    )
    .unwrap();
    let plan = parse_plan(&canonical(&plan)).unwrap();

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
