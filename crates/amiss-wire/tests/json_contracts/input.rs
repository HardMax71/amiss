use std::collections::BTreeMap;

use amiss_wire::{
    JsonInputError,
    de::{Error, ErrorKind},
    read_json,
};
use serde::{Deserialize, Serialize};

pub(super) fn assert_closed_input(
    bytes: &[u8],
    schema: &str,
    limit: u64,
    read: impl Fn(&[u8]) -> bool,
) -> Result<(), Box<dyn std::error::Error>> {
    assert!(read(bytes));
    let text = std::str::from_utf8(bytes)?;
    for key in ["schema", r"\u0073chema"] {
        let duplicate = text.replacen('{', &format!(r#"{{"{key}":"{schema}","#), 1);
        assert!(!read(duplicate.as_bytes()), "{duplicate}");
    }
    for invalid in [b"null".as_slice(), b"true", b"0", b"[]", b"\xff"] {
        assert!(!read(invalid));
    }
    assert!(!read(format!("\u{feff}{text}").as_bytes()));
    assert!(read(format!(" \n{text}\r\t").as_bytes()));
    for member in [
        r#""future":-0,"#,
        r#""future":0.5,"#,
        r#""future":1e0,"#,
        r#""future":9007199254740992,"#,
        r#""future":0,"future":1,"#,
        r#""future":0,"\u0066uture":1,"#,
    ] {
        let invalid = text.replacen('{', &format!("{{{member}"), 1);
        assert!(!read(invalid.as_bytes()), "{member}");
    }
    for depth in [128, 511, 512] {
        let nested = format!("{}null{}", "[".repeat(depth), "]".repeat(depth));
        let invalid = text.replacen('{', &format!(r#"{{"future":{nested},"#), 1);
        assert!(!read(invalid.as_bytes()), "depth {depth}");
    }
    for suffix in ["null", "{}", "garbage"] {
        assert!(!read(format!("{text}{suffix}").as_bytes()));
    }
    assert!(!read(format!("[{text}]").as_bytes()));
    let oversized = vec![
        b' ';
        usize::try_from(limit)?
            .checked_add(1)
            .ok_or("no representable oversized sample")?
    ];
    assert!(!read(&oversized));
    Ok(())
}

pub(super) fn assert_object_required<T, U>(
    (document, read, root_error): (&T, impl Fn(&[u8]) -> Result<T, Error>, ErrorKind),
    object: &U,
    positional: impl Serialize,
) -> Result<(), Box<dyn std::error::Error>>
where
    T: Serialize + PartialEq + std::fmt::Debug,
    U: Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let text = serde_json::to_string(document)?;
    for valid in [
        serde_json::to_string_pretty(document)?,
        String::from_utf8(serde_json_canonicalizer::to_vec(document)?)?,
        format!(" \n\t{}\r ", text.replace("schema", r"\u0073chema")),
        text.replace('/', r"\/"),
    ] {
        assert_eq!(&read(valid.as_bytes())?, document);
    }
    let positional = serde_json::to_string(&positional)?;
    assert_eq!(&serde_json::from_str::<U>(&positional)?, object);
    let object = serde_json::to_string(object)?;
    assert_eq!(text.matches(&object).count(), 1);
    let changed = text.replacen(&object, &positional, 1);
    assert_ne!(changed, text);
    assert_eq!(
        read(changed.as_bytes()).err(),
        Some(Error {
            path: "$".to_owned(),
            kind: if object == text {
                root_error
            } else {
                ErrorKind::InvalidValue
            },
        })
    );
    Ok(())
}

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
