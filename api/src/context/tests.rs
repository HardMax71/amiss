#![cfg(test)]

use super::{Error, parse};

fn context(features: &str, target_triple: &str) -> Vec<u8> {
    format!(
        r#"{{"cfg":[],"compiler":"rustc 1.100.0-nightly","dependencies_digest":"sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","features":{features},"name":"rust/example/local-function-declarations","package":"example","rustdoc_format":61,"schema":"amiss/rust-public-api-context","target":"example","target_triple":"{target_triple}"}}"#,
    )
    .into_bytes()
}

#[test]
fn context_binds_the_complete_normalization_configuration() {
    let (parsed, digest) = parse(&context(
        r#"["default","serde"]"#,
        "x86_64-unknown-linux-gnu",
    ))
    .unwrap();
    let (_, differently_featured) =
        parse(&context(r#"["default"]"#, "x86_64-unknown-linux-gnu")).unwrap();
    let (_, differently_targeted) =
        parse(&context(r#"["default","serde"]"#, "wasm32-unknown-unknown")).unwrap();

    assert_ne!(digest, differently_featured);
    assert_ne!(digest, differently_targeted);
    assert_eq!(
        parsed.name.as_str(),
        "rust/example/local-function-declarations"
    );
    assert_eq!(parsed.rustdoc_format, 61);
    assert_eq!(parsed.target, "example");
    assert_eq!(parsed.target_triple, "x86_64-unknown-linux-gnu");
}

#[test]
fn context_refuses_ambiguous_sets_and_unscoped_names() {
    assert!(matches!(
        parse(&context(
            r#"["serde","default"]"#,
            "x86_64-unknown-linux-gnu"
        )),
        Err(Error::Contract(_))
    ));
    let unscoped = String::from_utf8(context("[]", "x86_64-unknown-linux-gnu"))
        .unwrap()
        .replace("rust/example/local-function-declarations", "rust/example");
    assert!(matches!(
        parse(unscoped.as_bytes()),
        Err(Error::Contract(_))
    ));
}

#[test]
fn typed_context_preserves_canonical_bytes_and_every_digest_input() {
    let bytes = context(r#"["default","serde"]"#, "x86_64-unknown-linux-gnu");
    let (parsed, digest) = parse(&bytes).unwrap();
    let strict = amiss_wire::json::parse(&bytes).unwrap();
    assert_eq!(
        serde_json::to_vec(&parsed).unwrap(),
        serde_json_canonicalizer::to_vec(&strict).unwrap()
    );
    assert_eq!(
        digest,
        amiss_wire::digest::hb(
            super::DIGEST_DOMAIN,
            &serde_json_canonicalizer::to_vec(&strict).unwrap()
        )
    );
    assert_eq!(
        parse(&serde_json::to_vec_pretty(&parsed).unwrap()).unwrap(),
        (parsed, digest)
    );

    let original: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    for (field, replacement) in [
        ("cfg", serde_json::json!(["custom"])),
        ("compiler", serde_json::json!("another compiler")),
        (
            "dependencies_digest",
            serde_json::json!(
                "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            ),
        ),
        ("features", serde_json::json!(["default"])),
        (
            "name",
            serde_json::json!("rust/another/local-function-declarations"),
        ),
        ("package", serde_json::json!("another-package")),
        ("rustdoc_format", serde_json::json!(62)),
        ("target", serde_json::json!("another_target")),
        ("target_triple", serde_json::json!("wasm32-unknown-unknown")),
    ] {
        let mut changed = original.clone();
        changed[field] = replacement;
        let bytes = serde_json::to_vec(&changed).unwrap();
        let (_, changed_digest) = parse(&bytes).unwrap();
        assert_ne!(changed_digest, digest, "{field}");
        let strict = amiss_wire::json::parse(&bytes).unwrap();
        assert_eq!(
            changed_digest,
            amiss_wire::digest::hb(
                super::DIGEST_DOMAIN,
                &serde_json_canonicalizer::to_vec(&strict).unwrap()
            ),
            "{field}"
        );
    }
}

#[test]
fn context_requires_the_closed_typed_shape() {
    let original: serde_json::Value =
        serde_json::from_slice(&context("[]", "x86_64-unknown-linux-gnu")).unwrap();
    for field in original.as_object().unwrap().keys() {
        let mut missing = original.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(
            matches!(
                parse(&serde_json::to_vec(&missing).unwrap()),
                Err(Error::Shape(_))
            ),
            "{field}"
        );
        let mut null = original.clone();
        null[field] = serde_json::Value::Null;
        assert!(
            matches!(
                parse(&serde_json::to_vec(&null).unwrap()),
                Err(Error::Shape(_))
            ),
            "{field}"
        );
    }
    for (field, replacement) in [
        ("unknown", serde_json::json!(true)),
        ("schema", serde_json::json!("future")),
        ("dependencies_digest", serde_json::json!("not-a-digest")),
        (
            "name",
            serde_json::json!("invalid name/local-function-declarations"),
        ),
        ("rustdoc_format", serde_json::json!(-1)),
        ("rustdoc_format", serde_json::json!(u64::from(u32::MAX) + 1)),
    ] {
        let mut invalid = original.clone();
        invalid[field] = replacement;
        assert!(
            matches!(
                parse(&serde_json::to_vec(&invalid).unwrap()),
                Err(Error::Shape(_))
            ),
            "{field}"
        );
    }
}

#[test]
fn context_text_bounds_are_bytes_and_sets_remain_sorted_and_unique() {
    let original: serde_json::Value =
        serde_json::from_slice(&context("[]", "x86_64-unknown-linux-gnu")).unwrap();
    let longest = "é".repeat(super::TEXT_BYTES / 2);
    for field in [
        "compiler",
        "package",
        "target",
        "target_triple",
        "cfg",
        "features",
    ] {
        for text in [
            longest.clone(),
            format!("{longest}x"),
            String::new(),
            "a\nb".to_owned(),
            "a\u{85}b".to_owned(),
        ] {
            let mut value = original.clone();
            value[field] = if value[field].is_array() {
                serde_json::json!([text])
            } else {
                serde_json::json!(text)
            };
            assert_eq!(
                parse(&serde_json::to_vec(&value).unwrap()).is_ok(),
                text == longest,
                "{field}"
            );
        }
    }
    for field in ["cfg", "features"] {
        for entries in [serde_json::json!(["z", "a"]), serde_json::json!(["a", "a"])] {
            let mut value = original.clone();
            value[field] = entries;
            assert!(
                matches!(
                    parse(&serde_json::to_vec(&value).unwrap()),
                    Err(Error::Contract(_))
                ),
                "{field}"
            );
        }
        let mut value = original.clone();
        value[field] = (0..super::SET_MEMBERS)
            .map(|index| format!("{index:04}"))
            .collect();
        assert!(
            parse(&serde_json::to_vec(&value).unwrap()).is_ok(),
            "{field}"
        );
        value[field]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!("last"));
        assert!(
            matches!(
                parse(&serde_json::to_vec(&value).unwrap()),
                Err(Error::Contract(_))
            ),
            "{field}"
        );
    }
}

#[test]
fn context_keeps_strict_json_and_complete_stream_limits() {
    let valid = context("[]", "x86_64-unknown-linux-gnu");
    let duplicate = String::from_utf8(valid.clone())
        .unwrap()
        .replace(r#""cfg":[]"#, r#""cfg":[],"cfg":[]"#);
    for bytes in [
        duplicate.into_bytes(),
        [b"\xef\xbb\xbf".as_slice(), &valid].concat(),
        [valid.as_slice(), b" false"].concat(),
    ] {
        assert!(matches!(parse(&bytes), Err(Error::Json(_))));
    }
    let mut padded = valid;
    padded.resize(usize::try_from(super::BYTES).unwrap(), b' ');
    assert!(parse(&padded).is_ok());
    padded.push(b' ');
    assert!(matches!(parse(&padded), Err(Error::Bytes)));
}
