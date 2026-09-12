use std::borrow::Cow;

use amiss_wire::de::ErrorKind;
use amiss_wire::semantic::SemanticEvidenceTemplate;
use amiss_wire::semantic::observation::Observation;
use amiss_wire::semantic::record::{parse_input, template, validate_records};

const B: &str = "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const C: &str = "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

fn input(records: &str) -> Vec<u8> {
    format!(
        r#"{{
          "schema":"amiss/record-set-input",
          "producer_identity":"test-public-api",
          "context_digest":"{B}",
          "input_digest":"{C}",
          "complete":true,
          "name":"rust/public-api",
          "records":{records}
        }}"#
    )
    .into_bytes()
}

#[test]
fn normalized_records_become_one_checked_candidate_free_observation() {
    let source = parse_input(&input(
        r#"[{"key":"amiss::Request","value":"pub struct Request"},{"key":"amiss::run","value":"pub fn run()"}]"#,
    ))
    .unwrap();
    let bytes = template(source).unwrap();
    let parsed: SemanticEvidenceTemplate<'static> = serde_json::from_slice(&bytes).unwrap();
    assert!(amiss_wire::semantic::parse_template(&bytes).is_ok());
    assert_eq!(
        parsed.producer.kind,
        amiss_wire::semantic::SemanticProducerKind::RecordSet
    );
    assert_eq!(parsed.producer.identity.as_str(), "test-public-api");
    assert_eq!(parsed.producer.version, "1");
    assert!(parsed.complete);
    let [Cow::Owned(Observation::Record(decoded))] = parsed.observations.as_ref() else {
        panic!("one normalized set becomes one observation")
    };
    validate_records("$.observations[0].records", &decoded.records).unwrap();
    assert_eq!(decoded.name.as_str(), "rust/public-api");
    assert_eq!(decoded.records.len(), 2);
    assert_eq!(decoded.records[1].key, "amiss::run");
    assert_eq!(decoded.records[1].value, "pub fn run()");
}

#[test]
fn row_order_duplicates_and_closed_metadata_are_refused() {
    let escaped = parse_input(&input(r#"[{"\u006bey":"a","value":"A"}]"#)).unwrap();
    assert_eq!(escaped.records[0].key, "a");
    for records in [
        r#"[{"key":"a","key":"a","value":"A"}]"#,
        r#"[{"key":"a","\u006bey":"a","value":"A"}]"#,
        r#"[{"key":"a","value":"A","value":"A"}]"#,
    ] {
        assert!(matches!(parse_input(&input(records)).unwrap_err().kind,
            ErrorKind::Deserialize(source) if source.is_data()));
    }
    for (records, kind) in [
        (
            r#"[{"key":"z","value":"Z"},{"key":"a","value":"A"}]"#,
            ErrorKind::UnsortedSet,
        ),
        (
            r#"[{"key":"a","value":"A"},{"key":"a","value":"B"}]"#,
            ErrorKind::DuplicateMember,
        ),
    ] {
        assert_eq!(
            std::mem::discriminant(&parse_input(&input(records)).unwrap_err().kind),
            std::mem::discriminant(&kind)
        );
    }

    let source = String::from_utf8(input("[]")).unwrap();
    assert!(parse_input(source.as_bytes()).is_ok());
    let unknown = source.replacen('{', r#"{"producer_version":"2","#, 1);
    assert_ne!(unknown, source);
    assert!(
        matches!(parse_input(unknown.as_bytes()).unwrap_err().kind, ErrorKind::Deserialize(source) if source.is_data())
    );
}

#[test]
fn directly_constructed_inputs_reuse_the_reader_laws() {
    let mut source = parse_input(&input(
        r#"[{"key":"a","value":"A"},{"key":"z","value":"Z"}]"#,
    ))
    .unwrap();
    source.records.reverse();

    let error = template(source).unwrap_err();
    assert_eq!(error.path, "$.records");
    assert!(matches!(error.kind, ErrorKind::UnsortedSet));
}

#[test]
fn record_strings_use_the_scanner_consumer_bounds() {
    for records in [
        r#"[{"key":"","value":"value"}]"#,
        r#"[{"key":"key","value":"line\nfeed"}]"#,
    ] {
        assert!(matches!(
            parse_input(&input(records)).unwrap_err().kind,
            ErrorKind::InvalidValue
        ));
    }

    let oversized = "k".repeat(amiss_wire::semantic::RECORD_KEY_BYTES.saturating_add(1));
    let records = format!(r#"[{{"key":"{oversized}","value":"value"}}]"#);
    assert!(matches!(
        parse_input(&input(&records)).unwrap_err().kind,
        ErrorKind::InvalidValue
    ));
}
