use amiss_wire::digest::{hj, hj_with_length};
use amiss_wire::json::{ErrorKind, Value, canonical, canonical_length, parse};

#[test]
fn digest_counting_matches_the_independent_operations() {
    let value = Value::Object(vec![
        (
            "escaped".to_owned(),
            Value::String("q\" b\\ n\n scalar \u{1f600}".to_owned()),
        ),
        (
            "nested".to_owned(),
            Value::Array(vec![Value::Integer(42), Value::Bool(true), Value::Null]),
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
    let value = Value::String(text.to_owned());
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
            Value::String(expected.to_owned()),
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
