#![cfg(test)]

use crate::json::ValueExt as _;
use garde::Validate;
use serde::{Deserialize, Serialize};

use super::{
    Document, Envelope, MAX_SAFE_INTEGER, Schema, canonical, decode, digest, nullable, sorted_roles,
};
use crate::de::{Error, ErrorKind, fail};
use crate::digest::hj;
use crate::json;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
#[garde(allow_unvalidated)]
struct Probe {
    schema: Schema<Self>,
    #[garde(range(max = MAX_SAFE_INTEGER))]
    count: u64,
    #[serde(deserialize_with = "nullable")]
    slot: Option<u64>,
    #[garde(dive)]
    rows: Vec<Row>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
#[garde(allow_unvalidated)]
struct Row {
    #[garde(length(bytes, min = 1, max = 4))]
    name: String,
}

impl Document for Probe {
    const PAYLOAD_SCHEMA: &'static str = "amiss/probe-payload";
    const ENVELOPE_SCHEMA: &'static str = "amiss/probe-envelope";
    const LIMIT: u64 = 256;

    fn check(&self, root: &str) -> Result<(), Error> {
        if self.count == 7 {
            fail(&format!("{root}.count"), ErrorKind::Inconsistent)
        } else {
            Ok(())
        }
    }
}

fn probe() -> Probe {
    Probe {
        schema: Schema::default(),
        count: 1,
        slot: None,
        rows: vec![Row {
            name: "ab".to_owned(),
        }],
    }
}

fn sealed_text(from: &str, to: &str) -> Vec<u8> {
    let bytes = canonical(&Envelope::seal(probe()).unwrap()).unwrap();
    String::from_utf8(bytes)
        .unwrap()
        .replacen(from, to, 1)
        .into_bytes()
}

fn refusal(bytes: &[u8]) -> Error {
    Envelope::<Probe>::parse(bytes).unwrap_err()
}

#[test]
fn sealed_documents_round_trip_and_match_the_canonical_writer() {
    let sealed = Envelope::seal(probe()).unwrap();
    let bytes = canonical(&sealed).unwrap();
    let value = json::parse(&bytes).unwrap();

    assert_eq!(Envelope::<Probe>::parse(&bytes).unwrap(), sealed);
    assert_eq!(json::canonical(&value), bytes);
    assert_eq!(
        sealed.payload_digest,
        digest(Probe::PAYLOAD_SCHEMA, &probe()).unwrap()
    );
    assert_eq!(
        sealed.payload_digest,
        hj(Probe::PAYLOAD_SCHEMA, value.member("payload").unwrap())
    );
    assert_eq!(
        bytes,
        br#"{"payload":{"count":1,"rows":[{"name":"ab"}],"schema":"amiss/probe-payload","slot":null},"payload_digest":"#
            .iter()
            .chain(format!("\"{}\",\"schema\":\"amiss/probe-envelope\"}}", sealed.payload_digest).as_bytes())
            .copied()
            .collect::<Vec<u8>>()
    );
}

#[test]
fn shape_and_constraint_defects_keep_their_kinds_and_paths() {
    let cases = [
        (
            (r#""count":1"#, r#""count":1,"extra":true"#),
            "$.payload.extra",
            ErrorKind::UnknownField,
        ),
        (
            (r#""count":1,"#, ""),
            "$.payload.count",
            ErrorKind::MissingField,
        ),
        (
            (r#""count":1"#, r#""count":"one""#),
            "$.payload.count",
            ErrorKind::WrongType,
        ),
        (
            (r#""slot":null"#, r#""slot":-1"#),
            "$.payload.slot",
            ErrorKind::InvalidValue,
        ),
        (
            (r#","slot":null"#, ""),
            "$.payload.slot",
            ErrorKind::MissingField,
        ),
        (
            ("amiss/probe-payload", "amiss/other"),
            "$.payload.schema",
            ErrorKind::InvalidValue,
        ),
        (
            ("amiss/probe-envelope", "amiss/other"),
            "$.schema",
            ErrorKind::InvalidValue,
        ),
        (
            (r#""name":"ab""#, r#""name":"abcde""#),
            "$.payload.rows[0].name",
            ErrorKind::InvalidValue,
        ),
        (
            (r#""count":1"#, r#""count":7"#),
            "$.payload.count",
            ErrorKind::Inconsistent,
        ),
    ];
    for ((from, to), path, kind) in cases {
        let error = refusal(&sealed_text(from, to));
        assert_eq!(
            (error.path.as_str(), error.kind),
            (path, kind),
            "{from} -> {to}"
        );
        assert!(!error.message.is_empty());
    }
}

#[test]
fn digest_size_and_syntax_defects_are_refused_before_any_law() {
    let sealed = Envelope::seal(probe()).unwrap();
    let recorded = sealed.payload_digest.to_string();
    let tampered = sealed_text(&recorded, &format!("sha256:{}", "f".repeat(64)));
    let error = refusal(&tampered);
    assert_eq!(
        (error.path.as_str(), error.kind),
        ("$.payload_digest", ErrorKind::DigestMismatch)
    );

    let oversized = vec![b' '; 257];
    let error = refusal(&oversized);
    assert_eq!(
        (error.path.as_str(), error.kind),
        ("$", ErrorKind::LimitExceeded)
    );

    let duplicate = sealed_text(r#""count":1"#, r#""count":1,"count":2"#);
    let error = refusal(&duplicate);
    assert_eq!(error.path, "$.payload.count");
    assert!(matches!(
        error.kind,
        ErrorKind::Json(json::Error {
            kind: json::ErrorKind::DuplicateKey,
            ..
        })
    ));

    let mut trailing = canonical(&sealed).unwrap();
    trailing.extend_from_slice(b" x");
    let error = refusal(&trailing);
    assert!(matches!(
        error.kind,
        ErrorKind::Json(json::Error {
            kind: json::ErrorKind::TrailingContent,
            ..
        })
    ));

    let error = decode::<Envelope<Probe>>(b"{\n  \"payload\": \xff}").unwrap_err();
    assert_eq!(
        error.kind,
        ErrorKind::Json(json::Error {
            kind: json::ErrorKind::InvalidUtf8,
            offset: 15,
        })
    );

    let error = decode::<Envelope<Probe>>(b"{\n  \"payload\": x}").unwrap_err();
    assert_eq!(
        error.kind,
        ErrorKind::Json(json::Error {
            kind: json::ErrorKind::UnexpectedByte,
            offset: 15,
        })
    );
    assert_eq!(error.path, "$");
    assert_eq!(error.message, "unexpected byte at byte 15");
}

#[test]
fn sealing_refuses_what_parsing_refuses() {
    let mut inconsistent = probe();
    inconsistent.count = 7;
    let error = Envelope::seal(inconsistent).unwrap_err();
    assert_eq!(
        (error.path.as_str(), error.kind),
        ("$.payload.count", ErrorKind::Inconsistent)
    );

    let mut unsafe_count = probe();
    unsafe_count.count = MAX_SAFE_INTEGER + 1;
    let error = Envelope::seal(unsafe_count).unwrap_err();
    assert_eq!(
        (error.path.as_str(), error.kind),
        ("$.payload.count", ErrorKind::InvalidValue)
    );
    assert_eq!(
        error.to_string(),
        "greater than 9007199254740991 at $.payload.count"
    );

    let mut oversized = probe();
    oversized.rows = (0..40)
        .map(|_row| Row {
            name: "abcd".to_owned(),
        })
        .collect();
    let error = Envelope::seal(oversized).unwrap_err();
    assert_eq!(
        (error.path.as_str(), error.kind),
        ("$", ErrorKind::LimitExceeded)
    );
}

#[test]
fn two_subject_documents_order_their_roles() {
    assert!(sorted_roles("$.payload", &1, &2).is_ok());
    let equal = sorted_roles("$.payload", &2, &2).unwrap_err();
    assert_eq!(
        (equal.path.as_str(), equal.kind),
        ("$.payload.subjects", ErrorKind::DuplicateMember)
    );
    let reversed = sorted_roles("$.payload", &3, &2).unwrap_err();
    assert_eq!(
        (reversed.path.as_str(), reversed.kind),
        ("$.payload.subjects", ErrorKind::UnsortedSet)
    );
}

#[test]
fn strict_profile_applies_inside_dynamic_values_before_typed_decoding() {
    for (number, expected) in [
        ("9007199254740992", json::ErrorKind::IntegerOutOfRange),
        ("-0", json::ErrorKind::NegativeZero),
        ("1.0", json::ErrorKind::FractionOrExponent),
        ("1e0", json::ErrorKind::FractionOrExponent),
    ] {
        let bytes = format!(r#"{{"outer":[{{"number":{number}}}]}}"#);
        let error = decode::<serde_json::Value>(bytes.as_bytes()).unwrap_err();
        assert_eq!(error.path, "$.outer[0].number");
        assert!(matches!(error.kind, ErrorKind::Json(defect) if defect.kind == expected));
    }
    let error = decode::<serde_json::Value>(br#"{"outer":{"key":1,"\u006bey":2}}"#).unwrap_err();
    assert_eq!(error.path, "$.outer.key");
    assert!(
        matches!(error.kind, ErrorKind::Json(defect) if defect.kind == json::ErrorKind::DuplicateKey)
    );
}

#[test]
fn canonical_output_orders_utf16_keys_and_refuses_lossy_numeric_projection() {
    let value = serde_json::json!({"\u{e000}": 1, "\u{10000}": 2});
    assert_eq!(
        canonical(&value).unwrap(),
        "{\"\u{10000}\":2,\"\u{e000}\":1}".as_bytes()
    );
    for float in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.0, 1.0] {
        assert!(canonical(&[float]).is_err());
        assert!(super::to_value(&[float]).is_err());
    }
    assert!(canonical(&[MAX_SAFE_INTEGER + 1]).is_err());
    assert!(canonical(&std::collections::BTreeMap::from([(1_u64, "value")])).is_err());
}

#[test]
fn envelope_digest_covers_received_payload_before_normalization() {
    #[derive(Debug, Serialize, Deserialize, Validate)]
    #[garde(allow_unvalidated)]
    struct Normalizing {
        schema: Schema<Self>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        optional: Option<String>,
    }
    impl Document for Normalizing {
        const PAYLOAD_SCHEMA: &'static str = "amiss/normalizing-payload";
        const ENVELOPE_SCHEMA: &'static str = "amiss/normalizing-envelope";
        const LIMIT: u64 = 1024;
    }
    let envelope = Envelope::seal(Normalizing {
        schema: Schema::default(),
        optional: None,
    })
    .unwrap();
    let wire = String::from_utf8(canonical(&envelope).unwrap()).unwrap();
    let modified = wire.replace(r#""payload":{"#, r#""payload":{"optional":null,"#);
    let error = Envelope::<Normalizing>::parse(modified.as_bytes()).unwrap_err();
    assert_eq!(error.kind, ErrorKind::DigestMismatch);
    assert_eq!(error.path, "$.payload_digest");
}

#[test]
fn depth_boundary_survives_parse_clone_serialize_and_drop_on_a_worker_stack() {
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(|| {
            let at = format!("{}{}", "[".repeat(512), "]".repeat(512));
            let value = json::parse(at.as_bytes()).unwrap();
            let clone = value.clone();
            assert_eq!(json::canonical(&clone), at.as_bytes());
            drop(clone);
            drop(value);
            let over = format!("{}{}", "[".repeat(513), "]".repeat(513));
            assert_eq!(
                json::parse(over.as_bytes()).unwrap_err().kind,
                json::ErrorKind::DepthLimit
            );
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn serialization_depth_counts_external_variant_and_byte_containers() {
    use serde::ser::SerializeSeq as _;

    struct Nested<'a, T> {
        remaining: usize,
        leaf: &'a T,
    }
    impl<T: Serialize> Serialize for Nested<'_, T> {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            let Some(remaining) = self.remaining.checked_sub(1) else {
                return self.leaf.serialize(serializer);
            };
            let mut sequence = serializer.serialize_seq(Some(1))?;
            sequence.serialize_element(&Self {
                remaining,
                leaf: self.leaf,
            })?;
            sequence.end()
        }
    }
    #[derive(Serialize)]
    enum Variant {
        Newtype(u8),
        Tuple(u8, u8),
        Record { field: u8 },
    }
    struct Bytes;
    impl Serialize for Bytes {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            serializer.serialize_bytes(&[1])
        }
    }
    fn check(remaining: usize, leaf: &Variant) -> Result<(), serde_json::Error> {
        json::check_profile(&Nested { remaining, leaf })
    }
    assert!(check(511, &Variant::Newtype(1)).is_ok());
    assert!(check(512, &Variant::Newtype(1)).is_err());
    for variant in [Variant::Tuple(1, 2), Variant::Record { field: 1 }] {
        assert!(check(510, &variant).is_ok());
        assert!(check(511, &variant).is_err());
    }
    assert!(
        json::check_profile(&Nested {
            remaining: 511,
            leaf: &Bytes
        })
        .is_ok()
    );
    assert!(
        json::check_profile(&Nested {
            remaining: 512,
            leaf: &Bytes
        })
        .is_err()
    );
}
