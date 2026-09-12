use amiss_wire::de::JsonProfile;

#[test]
fn library_serialization_preserves_the_strict_readers_full_depth() {
    for (open, close) in [("[", "]"), (r#"{"a":"#, "}")] {
        let source = format!("{}null{}", open.repeat(512), close.repeat(512));
        JsonProfile::validate(source.as_bytes()).unwrap();
        let mut parser = serde_json::Deserializer::from_str(&source);
        parser.disable_recursion_limit();
        let value: serde_json::Value = serde::Deserialize::deserialize(&mut parser).unwrap();
        let bytes = serde_json_canonicalizer::to_vec(&value).unwrap();
        assert_eq!(bytes, source.as_bytes());
        JsonProfile::validate(&bytes).unwrap();
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
fn wire_profile_rejects_ambiguous_values_and_bounds_recursion() {
    for invalid in [
        br#"{"future":{"a":0,"\u0061":1}}"#.as_slice(),
        b"-0",
        b"1.0",
        b"1e0",
        b"9007199254740992",
        b"-9007199254740992",
        b"{} {}",
        b"\xff",
        b"\xef\xbb\xbf{}",
    ] {
        assert!(JsonProfile::validate(invalid).is_err(), "{invalid:?}");
    }
    for scalar in ["null", "0", "9007199254740991", "-9007199254740991"] {
        JsonProfile::validate(scalar.as_bytes()).unwrap();
        let limit = format!("{}{}{}", "[".repeat(512), scalar, "]".repeat(512));
        JsonProfile::validate(limit.as_bytes()).unwrap();
        assert!(JsonProfile::validate(format!("[{limit}]").as_bytes()).is_err());
    }
}
