use std::collections::BTreeMap;

use amiss_wire::{JsonInputError, read_json};
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize)]
struct Document {
    count: u64,
    label: Option<String>,
}

#[test]
fn typed_input_preserves_the_complete_document_and_byte_ceiling() {
    let input = br#" { "label": "\u00fc", "count": 9007199254740991 } "#;
    let limit = u64::try_from(input.len()).unwrap();
    assert_eq!(
        read_json::<Document>(input, limit).unwrap(),
        Document {
            count: 9_007_199_254_740_991,
            label: Some("ü".to_owned()),
        }
    );
    assert!(matches!(
        read_json::<Document>(input, limit - 1),
        Err(JsonInputError::LimitExceeded)
    ));
    for invalid in [
        r#"{"count":1,"label":null,"unknown":true}"#,
        r#"{"count":1}"#,
        "[1,null]",
        r#"{"count":true,"label":null}"#,
    ] {
        assert!(
            matches!(
                read_json::<Document>(invalid.as_bytes(), 1024),
                Err(JsonInputError::Shape(_))
            ),
            "{invalid}"
        );
    }
}

#[test]
fn typed_input_rejects_lexical_defects_before_deserialization() {
    use amiss_wire::json::ErrorKind;

    for (invalid, expected) in [
        (
            r#"{"count":1,"count":1,"label":null}"#,
            ErrorKind::DuplicateKey,
        ),
        (
            r#"{"count":1,"\u0063ount":1,"label":null}"#,
            ErrorKind::DuplicateKey,
        ),
        (r#"{"count":-0,"label":null}"#, ErrorKind::NegativeZero),
        (
            r#"{"count":1.0,"label":null}"#,
            ErrorKind::FractionOrExponent,
        ),
        (
            r#"{"count":1e0,"label":null}"#,
            ErrorKind::FractionOrExponent,
        ),
        (
            r#"{"count":9007199254740992,"label":null}"#,
            ErrorKind::IntegerOutOfRange,
        ),
        (r#"{"count":1,"label":"\ud800"}"#, ErrorKind::LoneSurrogate),
        (
            r#"{"count":1,"label":null} false"#,
            ErrorKind::TrailingContent,
        ),
        (
            "\u{feff}{\"count\":1,\"label\":null}",
            ErrorKind::ByteOrderMark,
        ),
    ] {
        let Err(JsonInputError::Json(error)) = read_json::<Document>(invalid.as_bytes(), 1024)
        else {
            panic!("strict JSON defect was not reported: {invalid}");
        };
        assert_eq!(error.kind, expected, "{invalid}");
    }
    assert!(matches!(
        read_json::<Document>(&[0xff], 1024),
        Err(JsonInputError::Json(amiss_wire::json::Error {
            kind: ErrorKind::InvalidUtf8,
            ..
        }))
    ));
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize)]
struct Section {
    children: Vec<Section>,
}

#[test]
fn typed_input_retains_the_strict_depth_limit_not_serdes_default() {
    for (levels, valid) in [(128, true), (255, true), (256, false)] {
        let input = format!(
            "{}{{\"children\":[]}}{}",
            "{\"children\":[".repeat(levels),
            "]}".repeat(levels)
        );
        let result = read_json::<Section>(input.as_bytes(), 16_384);
        if valid {
            let section = result.unwrap();
            assert_eq!(serde_json::to_string(&section).unwrap(), input);
        } else {
            assert!(matches!(
                result,
                Err(JsonInputError::Json(amiss_wire::json::Error {
                    kind: amiss_wire::json::ErrorKind::DepthLimit,
                    ..
                }))
            ));
        }
    }
}

#[test]
fn typed_maps_preserve_unicode_keys_without_requiring_input_order() {
    let input = r#"{"\ue000":"first","😀":"second"}"#;
    let document: BTreeMap<String, String> = read_json(input.as_bytes(), 1024).unwrap();
    assert_eq!(document["\u{e000}"], "first");
    assert_eq!(document["😀"], "second");
}
