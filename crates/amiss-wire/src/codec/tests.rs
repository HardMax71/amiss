#![cfg(test)]

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
            (r#""count":1"#, r#""count":9007199254740992"#),
            "$.payload.count",
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
    assert_eq!(error.message, "expected value");
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
