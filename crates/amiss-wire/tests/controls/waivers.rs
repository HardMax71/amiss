use amiss_wire::controls::{WaiverBundle, parse_waiver_bundle};
use amiss_wire::de::ErrorKind;
use amiss_wire::model::UtcInstant;

use super::support::WAIVER;

#[test]
fn waiver_instants_bind_at_their_exact_boundaries() {
    for (created, not_before, valid) in [
        ("2026-07-02T00:00:00Z", "2026-07-02T00:00:00Z", true),
        ("2026-07-03T00:00:00Z", "2026-07-03T00:00:00Z", true),
        ("2026-07-02T00:00:01Z", "2026-07-02T00:00:00Z", false),
        ("2026-07-04T00:00:00Z", "2026-07-05T00:00:00Z", false),
    ] {
        let mut bundle: WaiverBundle = serde_json::from_slice(WAIVER).unwrap();
        bundle.created_at = UtcInstant::new("2026-07-03T00:00:00Z".to_owned()).unwrap();
        bundle.items[0].created_at = UtcInstant::new(created.to_owned()).unwrap();
        bundle.items[0].not_before = UtcInstant::new(not_before.to_owned()).unwrap();
        bundle.items[0].expires_at = UtcInstant::new("2026-08-01T00:00:00Z".to_owned()).unwrap();
        let result = bundle.validate();
        if valid {
            assert!(result.is_ok(), "{created}: {result:?}");
        } else {
            assert_eq!(result.unwrap_err().kind, ErrorKind::Inconsistent);
        }
    }
}

#[test]
fn waiver_reasons_must_explain_the_exception() {
    for reason in ["", "   "] {
        let mut bundle: WaiverBundle = serde_json::from_slice(WAIVER).unwrap();
        bundle.items[0].reason = reason.to_owned();
        assert_eq!(bundle.validate().unwrap_err().kind, ErrorKind::InvalidValue);
    }
}

#[test]
fn parses_a_valid_waiver_bundle_and_rejects_duplicates() {
    let original: WaiverBundle = serde_json::from_slice(WAIVER).unwrap();
    assert_eq!(parse_waiver_bundle(WAIVER).unwrap(), original);
    let mut same_owner = original.clone();
    same_owner.items[0].issuer = same_owner.items[0].owner.clone();
    assert!(same_owner.validate().is_ok());

    let mut duplicate = original.clone();
    let mut second = duplicate.items[0].clone();
    second.waiver_id = "waiver/two".parse().unwrap();
    duplicate.items.push(second);
    assert_eq!(
        duplicate.validate().unwrap_err().kind,
        ErrorKind::DuplicateMember
    );

    let mut bad_window = original;
    bad_window.items[0].not_before = UtcInstant::new("2026-09-01T00:00:00Z".to_owned()).unwrap();
    assert_eq!(
        bad_window.validate().unwrap_err().kind,
        ErrorKind::Inconsistent
    );
    for residual in ["record", "quietly"] {
        let malformed = std::str::from_utf8(WAIVER).unwrap().replace(
            "\"residual_disposition\": \"warn\"",
            &format!("\"residual_disposition\": \"{residual}\""),
        );
        assert_eq!(
            parse_waiver_bundle(malformed.as_bytes()).unwrap_err().kind,
            ErrorKind::InvalidValue
        );
    }
}
