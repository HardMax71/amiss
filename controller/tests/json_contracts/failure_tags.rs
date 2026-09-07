use amiss_controller::RunFailure;
use strum::IntoEnumIterator;

#[test]
fn failure_tags_keep_their_published_spelling() {
    let failures: Vec<_> = RunFailure::iter().collect();
    let expected = [
        "missing-output",
        "timeout",
        "tampered-runtime",
        "unavailable",
        "oversized-output",
        "wrong-identity",
        "wrong-tree",
        "authorization-revoked",
        "closed",
    ];
    assert_eq!(
        serde_json::to_string(&failures).unwrap(),
        serde_json::to_string(&expected).unwrap()
    );
    assert_eq!(
        failures.iter().map(ToString::to_string).collect::<Vec<_>>(),
        expected
    );
}
