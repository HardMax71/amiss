use amiss_wire::{
    digest::hb,
    json::{Value, parse},
};

#[test]
fn the_example_oracle_rejects_incomplete_or_trailing_json() {
    for invalid in [
        b"".as_slice(),
        b"{",
        b"[1,]",
        b"{} null",
        b"[] trailing",
        b"\"\xff\"",
    ] {
        assert!(
            amiss_fixtures::canonical_json(invalid).is_err(),
            "{invalid:?}"
        );
    }
}

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
        let transcoded = amiss_fixtures::canonical_json(input.as_bytes()).unwrap();
        assert_eq!(transcoded, bytes, "{input}");
        assert_eq!(parse(&bytes).unwrap(), value);
        let mut counter = countio::Counter::new(std::io::sink());
        serde_json_canonicalizer::to_writer(&value, &mut counter).unwrap();
        assert_eq!(counter.writer_bytes(), expected.len());
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
    let unordered = Value::Object(Box::new([
        ("\u{e000}".to_owned(), Value::Integer(2)),
        ("\u{10000}".to_owned(), Value::Integer(1)),
    ]));
    assert_eq!(
        serde_json_canonicalizer::to_vec(&unordered).unwrap(),
        "{\"\u{10000}\":1,\"\u{e000}\":2}".as_bytes()
    );
}

#[test]
fn library_serialization_preserves_the_strict_readers_full_depth() {
    for (open, close) in [("[", "]"), (r#"{"a":"#, "}")] {
        let source = format!("{}null{}", open.repeat(512), close.repeat(512));
        let value = parse(source.as_bytes()).unwrap();
        let bytes = serde_json_canonicalizer::to_vec(&value).unwrap();
        assert_eq!(bytes, source.as_bytes());
        assert_eq!(parse(&bytes).unwrap(), value);
        let mut deserializer = serde_json::Deserializer::from_str(&source);
        deserializer.disable_recursion_limit();
        assert_eq!(
            serde_json_canonicalizer::to_vec(&serde_transcode::Transcoder::new(&mut deserializer))
                .unwrap(),
            bytes
        );
    }
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
    }
}
