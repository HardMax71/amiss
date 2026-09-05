use amiss_wire::digest::{hj, hj_with_length};
use amiss_wire::json::{ErrorKind, Value, canonical, canonical_length, parse};

#[test]
fn serde_hashing_binds_the_selected_writer_and_propagates_errors() {
    use std::collections::BTreeMap;

    use amiss_wire::digest::{hb, hj_serde};
    use serde::Serialize;

    #[derive(Serialize)]
    struct Input<'a> {
        escaped: &'a str,
        integers: [i64; 3],
        nullable: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        omitted: Option<bool>,
    }

    #[derive(Serialize)]
    struct Unordered<'a> {
        z: &'a str,
        a: BTreeMap<&'a str, u64>,
    }

    let input = Input {
        escaped: "q\" \\ \n \u{1} β",
        integers: [-42, 0, 42],
        nullable: None,
        omitted: None,
    };
    let bytes = b"{\"escaped\":\"q\\\" \\\\ \\n \\u0001 \xce\xb2\",\"integers\":[-42,0,42],\"nullable\":null}";
    for domain in ["", "amiss/typed-test", "amiss/typed-test\0"] {
        assert_eq!(
            hj_serde(domain, |writer| serde_json::to_writer(writer, &input)).unwrap(),
            hb(domain, bytes)
        );
    }
    assert_ne!(
        hj_serde("amiss/typed-test", |writer| serde_json::to_writer(
            writer, &input
        ))
        .unwrap(),
        hj_serde("amiss/typed-test\0", |writer| serde_json::to_writer(
            writer, &input
        ))
        .unwrap()
    );
    let invalid = BTreeMap::from([((1, 2), true)]);
    assert!(
        hj_serde("amiss/typed-test", |writer| serde_json::to_writer(
            writer, &invalid
        ))
        .is_err()
    );

    let unordered = Unordered {
        z: "line\n",
        a: BTreeMap::from([("\u{e000}", 2), ("\u{10000}", 1)]),
    };
    let expected = "{\"a\":{\"\u{10000}\":1,\"\u{e000}\":2},\"z\":\"line\\n\"}";
    let canonical = hj_serde("amiss/typed-test", |mut writer| {
        serde_json_canonicalizer::to_writer(&unordered, &mut writer)
    })
    .unwrap();
    assert_eq!(canonical, hb("amiss/typed-test", expected.as_bytes()));
    assert_ne!(
        canonical,
        hj_serde("amiss/typed-test", |writer| serde_json::to_writer(
            writer, &unordered
        ))
        .unwrap()
    );
    assert!(
        hj_serde("amiss/typed-test", |mut writer| {
            serde_json_canonicalizer::to_writer(&invalid, &mut writer)
        })
        .is_err()
    );
}

#[cfg(target_pointer_width = "64")]
#[test]
fn an_owned_value_uses_three_machine_words() {
    assert_eq!(size_of::<Value>(), 24);
}

#[test]
fn digest_counting_matches_the_independent_operations() {
    let value = Value::object(vec![
        (
            "escaped".to_owned(),
            Value::string("q\" b\\ n\n scalar \u{1f600}".to_owned()),
        ),
        (
            "nested".to_owned(),
            Value::array(vec![Value::Integer(42), Value::Bool(true), Value::Null]),
        ),
    ]);
    let (digest, length) = hj_with_length("amiss/test", &value);
    assert_eq!(digest, hj("amiss/test", &value));
    assert_eq!(length, canonical_length(&value));
}

/// Every short escape on the write side, byte for byte, and back through the
/// parser; the counting sink must agree with the materialized bytes.
#[test]
fn every_escape_survives_a_round_trip() {
    let text = "q\" b\\ s\u{8} t\t n\n f\u{c} r\r e\u{1} done";
    let value = Value::string(text.to_owned());
    let wire = canonical(&value);
    assert_eq!(
        String::from_utf8(wire.clone()).unwrap(),
        "\"q\\\" b\\\\ s\\b t\\t n\\n f\\f r\\r e\\u0001 done\"",
    );
    assert_eq!(parse(&wire).unwrap(), value);
    assert_eq!(canonical_length(&value), u64::try_from(wire.len()).unwrap());
}

/// Surrogate pairs combine to the exact scalar at both ends of the plane,
/// uppercase hex digits count, and every half-pair shape is a lone surrogate.
#[test]
fn surrogate_pairs_combine_exactly() {
    let pairs: [(&[u8], &str); 4] = [
        (br#""\uD83D\uDE00""#, "\u{1F600}"),
        (br#""\uD800\uDC00""#, "\u{10000}"),
        (br#""\uDBFF\uDFFF""#, "\u{10FFFF}"),
        (br#""\u00AF""#, "\u{AF}"),
    ];
    for (wire, expected) in pairs {
        assert_eq!(
            parse(wire).unwrap(),
            Value::string(expected.to_owned()),
            "{}",
            String::from_utf8_lossy(wire)
        );
    }

    let lone: [&[u8]; 4] = [
        br#""\uD800""#,
        br#""\uD800x""#,
        br#""\uD800\n""#,
        br#""\uDC00""#,
    ];
    for wire in lone {
        assert_eq!(
            parse(wire).unwrap_err().kind,
            ErrorKind::LoneSurrogate,
            "{}",
            String::from_utf8_lossy(wire)
        );
    }
}

/// Depth 512 is the last legal nesting; 513 names the limit.
#[test]
fn the_depth_limit_is_inclusive() {
    let at = format!("{}{}", "[".repeat(512), "]".repeat(512));
    assert!(parse(at.as_bytes()).is_ok(), "the limit itself parses");

    let over = format!("{}{}", "[".repeat(513), "]".repeat(513));
    assert_eq!(
        parse(over.as_bytes()).unwrap_err().kind,
        ErrorKind::DepthLimit
    );
}
