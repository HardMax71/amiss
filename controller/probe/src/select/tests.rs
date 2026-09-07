#![cfg(test)]

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
    let (report, _verdict) = amiss_wire::report::validate_envelope(&report).unwrap();
    let plan = amiss_wire::external::plan(
        &report,
        &report.payload.engine.engine_version,
        report.payload.engine.engine_digest,
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
