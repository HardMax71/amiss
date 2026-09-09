use amiss_controller::ProviderNamespace;

#[test]
fn provider_namespace_is_open_but_canonical() {
    for byte in u8::MIN..=u8::MAX {
        let character = char::from(byte);
        let first = "abcdefghijklmnopqrstuvwxyz0123456789".contains(character);
        let tail = "abcdefghijklmnopqrstuvwxyz0123456789.-".contains(character);
        for (raw, accepted) in [
            (character.to_string(), first),
            (format!("{character}a"), first),
            (format!("a{character}"), tail),
            (format!("a{character}b"), tail),
        ] {
            assert_eq!(
                ProviderNamespace::try_from(raw.clone()).is_ok(),
                accepted,
                "{raw:?}"
            );
            let encoded = serde_json::to_string(&raw).unwrap();
            assert_eq!(
                serde_json::from_str::<ProviderNamespace>(&encoded).is_ok(),
                accepted,
                "{encoded}"
            );
        }
    }
    for raw in ["github", "forgejo-v2", "0", "a.", "a-", "a..b", "a--b"] {
        assert!(ProviderNamespace::try_from(raw.to_owned()).is_ok(), "{raw}");
    }
    for raw in [
        "",
        "GitHub",
        "gitea/family",
        ".a",
        "-a",
        "a\n",
        "a\r\n",
        " a",
        "a ",
        "a💡",
    ] {
        assert!(
            ProviderNamespace::try_from(raw.to_owned()).is_err(),
            "{raw:?}"
        );
    }
    for (length, accepted) in [(0, false), (1, true), (63, true), (64, true), (65, false)] {
        assert_eq!(
            ProviderNamespace::try_from("a".repeat(length)).is_ok(),
            accepted
        );
    }
}

#[test]
fn namespace_json_preserves_spelling_and_rejects_other_shapes() {
    for raw in ["github".to_owned(), "0.a--b.".to_owned(), "a".repeat(64)] {
        let namespace = ProviderNamespace::try_from(raw.clone()).unwrap();
        assert_eq!(namespace.as_str(), raw);
        assert_eq!(namespace.to_string(), raw);
        let encoded = serde_json::to_string(&namespace).unwrap();
        assert_eq!(encoded, format!("\"{raw}\""));
        assert_eq!(
            serde_json::from_str::<ProviderNamespace>(&encoded).unwrap(),
            namespace
        );
    }
    for raw in [
        "null",
        "true",
        "0",
        "1.0",
        "[]",
        "[\"github\"]",
        "{}",
        "{\"namespace\":\"github\"}",
        "\"\"",
        "\"GitHub\"",
        "\".github\"",
        "\"github\\n\"",
        "\"github/forge\"",
    ] {
        assert!(
            serde_json::from_str::<ProviderNamespace>(raw).is_err(),
            "{raw}"
        );
    }
    let too_long = serde_json::to_string(&"a".repeat(65)).unwrap();
    assert!(serde_json::from_str::<ProviderNamespace>(&too_long).is_err());
}
