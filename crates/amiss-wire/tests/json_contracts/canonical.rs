use amiss_wire::{
    digest::hb,
    json::{Value, parse},
};

#[test]
fn strict_values_serialize_as_json_without_enum_or_pair_wrappers() {
    for (input, expected) in [
        ("null", "null"),
        (
            "[false,true,-9007199254740991,0,9007199254740991]",
            "[false,true,-9007199254740991,0,9007199254740991]",
        ),
        (
            r#"{"z":[1,{"b":null,"a":true}],"a":[]}"#,
            r#"{"a":[],"z":[1,{"a":true,"b":null}]}"#,
        ),
        (
            "{\"\u{e000}\":2,\"\u{10000}\":1}",
            "{\"\u{10000}\":1,\"\u{e000}\":2}",
        ),
        (
            r#""\u0000\u0008\u0009\u000a\u000c\u000d\u001f\"\\/é😀""#,
            r#""\u0000\b\t\n\f\r\u001f\"\\/é😀""#,
        ),
    ] {
        let value = parse(input.as_bytes()).unwrap();
        let bytes = serde_json_canonicalizer::to_vec(&value).unwrap();
        assert_eq!(bytes, expected.as_bytes(), "{input}");
        assert_eq!(parse(&bytes).unwrap(), value);
        let digest = amiss_wire::digest::hj_serde("amiss/test-json", |mut writer| {
            serde_json_canonicalizer::to_writer(&value, &mut writer)
        })
        .unwrap();
        assert_eq!(
            digest,
            hb("amiss/test-json", expected.as_bytes()),
            "{input}"
        );
    }
    let empty = Value::Object(Box::new([]));
    assert_eq!(serde_json::to_vec(&empty).unwrap(), b"{}");
}

#[test]
fn every_committed_example_keeps_its_strict_value_through_the_library_writer() {
    let examples = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/examples");
    let mut paths = std::fs::read_dir(examples)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect::<Vec<_>>();
    paths.sort();
    assert!(!paths.is_empty());
    for path in paths {
        let source = std::fs::read(&path).unwrap();
        let value = parse(&source).unwrap();
        let bytes = serde_json_canonicalizer::to_vec(&value).unwrap();
        assert_eq!(parse(&bytes).unwrap(), value, "{}", path.display());
        assert_eq!(
            bytes,
            amiss_wire::json::canonical(&value),
            "{}",
            path.display()
        );
    }
}
