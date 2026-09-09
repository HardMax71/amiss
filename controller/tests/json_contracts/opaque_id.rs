use amiss_controller::{DeliveryIdentity, OpaqueId, ProviderIdentity};

#[test]
fn opaque_delivery_ids_reject_ambiguous_bytes() {
    let alphabet = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789._:/@+-";
    for byte in u8::MIN..=u8::MAX {
        let character = char::from(byte);
        let accepted = alphabet.contains(character);
        for raw in [
            character.to_string(),
            format!("{character}a"),
            format!("a{character}"),
            format!("a{character}b"),
        ] {
            assert_eq!(OpaqueId::try_from(raw.clone()).is_ok(), accepted, "{raw:?}");
            let encoded = serde_json::to_string(&raw).unwrap();
            assert_eq!(
                serde_json::from_str::<OpaqueId>(&encoded).is_ok(),
                accepted,
                "{encoded}"
            );
        }
    }
    for raw in ["0123-abcd:1", "/", "..", "A/B@C+D_E:F-G.H", ".a/../b"] {
        assert!(OpaqueId::try_from(raw.to_owned()).is_ok(), "{raw}");
    }
    for raw in [
        "",
        " delivery",
        "line\nbreak",
        "a\r\n",
        "a ",
        "a💡",
        "a\\b",
        "a%2Fb",
    ] {
        assert!(OpaqueId::try_from(raw.to_owned()).is_err(), "{raw:?}");
    }
    for (length, accepted) in [
        (0, false),
        (1, true),
        (255, true),
        (256, true),
        (257, false),
    ] {
        assert_eq!(OpaqueId::try_from("a".repeat(length)).is_ok(), accepted);
        let encoded = serde_json::to_string(&"a".repeat(length)).unwrap();
        assert_eq!(serde_json::from_str::<OpaqueId>(&encoded).is_ok(), accepted);
    }
}

#[test]
fn opaque_ids_preserve_spelling_and_require_json_strings() {
    for raw in [
        "/".to_owned(),
        "..".to_owned(),
        "A/B@C+D_E:F-G.H".to_owned(),
        "a".repeat(256),
    ] {
        let id = OpaqueId::try_from(raw.clone()).unwrap();
        assert_eq!(id.as_str(), raw);
        assert_eq!(id.to_string(), raw);
        let encoded = serde_json::to_string(&id).unwrap();
        assert_eq!(encoded, serde_json::to_string(&raw).unwrap());
        assert_eq!(serde_json::from_str::<OpaqueId>(&encoded).unwrap(), id);
    }
    for encoded in [
        "null",
        "true",
        "0",
        "1.0",
        "[]",
        "[\"id\"]",
        "{}",
        "{\"id\":\"id\"}",
    ] {
        assert!(
            serde_json::from_str::<OpaqueId>(encoded).is_err(),
            "{encoded}"
        );
    }
}

#[test]
fn delivery_contracts_keep_the_stored_shape_and_close_each_identity() {
    let provider = ProviderIdentity::new("github".to_owned(), "Host.Example".to_owned()).unwrap();
    let delivery = DeliveryIdentity {
        provider,
        integration: OpaqueId::try_from("Integration/7".to_owned()).unwrap(),
        delivery: OpaqueId::try_from("Delivery:A+B@C".to_owned()).unwrap(),
    };
    let encoded = serde_json::to_string(&delivery).unwrap();
    assert_eq!(
        encoded,
        r#"{"provider":{"namespace":"github","instance":"Host.Example"},"integration":"Integration/7","delivery":"Delivery:A+B@C"}"#
    );
    assert_eq!(
        serde_json::from_str::<DeliveryIdentity>(&encoded).unwrap(),
        delivery
    );
    for mutation in [
        encoded.replacen('{', r#"{"unknown":false,"#, 1),
        encoded.replace(r#""namespace":"#, r#""unknown":false,"namespace":"#),
        encoded.replace(r#""delivery":"#, r#""delivery":"duplicate","delivery":"#),
        encoded.replace("Host.Example", "host example"),
        encoded.replace("Integration/7", ""),
        encoded.replace("Delivery:A+B@C", &"a".repeat(257)),
        encoded.replace(r#""Integration/7""#, "null"),
        encoded.replace(
            r#""delivery":"Delivery:A+B@C""#,
            r#""other":"Delivery:A+B@C""#,
        ),
    ] {
        assert_ne!(mutation, encoded);
        assert!(
            serde_json::from_str::<DeliveryIdentity>(&mutation).is_err(),
            "{mutation}"
        );
    }
}
