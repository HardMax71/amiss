use amiss_wire::de::ErrorKind;
use amiss_wire::json::{Value, canonical};
use amiss_wire::semantic::record::{decode_observation, parse_input, template};

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
    let value = template(source).unwrap();
    let parsed = amiss_wire::semantic::parse_template(&canonical(&value)).unwrap();
    assert_eq!(parsed.producer_kind.as_str(), "record-set");
    assert_eq!(parsed.producer_identity.as_str(), "test-public-api");
    assert_eq!(parsed.producer_version, "1");
    assert!(parsed.complete);
    let [observation] = parsed.observations.as_ref() else {
        panic!("one normalized set becomes one observation")
    };
    let decoded = decode_observation("$.observations[0]", observation.clone()).unwrap();
    assert_eq!(decoded.name.as_str(), "rust/public-api");
    assert_eq!(decoded.records.len(), 2);
    assert_eq!(decoded.records["amiss::run"], "pub fn run()");
}

#[test]
fn row_order_duplicates_and_closed_metadata_are_refused() {
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
        assert_eq!(parse_input(&input(records)).unwrap_err().kind, kind);
    }

    let value = amiss_wire::json::parse(&input("[]")).unwrap();
    let Value::Object(members) = value else {
        panic!("the source is an object")
    };
    let mut members = members.into_vec();
    members.push(("producer_version".to_owned(), Value::string("2")));
    let value = Value::object(members);
    assert_eq!(
        parse_input(&canonical(&value)).unwrap_err().kind,
        ErrorKind::UnknownField
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
    assert_eq!(error.kind, ErrorKind::UnsortedSet);
}

#[test]
fn record_strings_use_the_scanner_consumer_bounds() {
    for records in [
        r#"[{"key":"","value":"value"}]"#,
        r#"[{"key":"key","value":"line\nfeed"}]"#,
    ] {
        assert_eq!(
            parse_input(&input(records)).unwrap_err().kind,
            ErrorKind::InvalidValue
        );
    }

    let oversized = "k".repeat(amiss_wire::semantic::RECORD_KEY_BYTES.saturating_add(1));
    let records = format!(r#"[{{"key":"{oversized}","value":"value"}}]"#);
    assert_eq!(
        parse_input(&input(&records)).unwrap_err().kind,
        ErrorKind::InvalidValue
    );
}
