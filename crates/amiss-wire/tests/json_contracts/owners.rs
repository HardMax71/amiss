use amiss_wire::model::OwnerId;
use amiss_wire::requests::ControlsRequest;

#[test]
fn owner_json_enforces_the_existing_ascii_grammar() {
    for prefix in ["team:", "service:", "user:"] {
        for byte in u8::MIN..=u8::MAX {
            let character = char::from(byte);
            let first = "abcdefghijklmnopqrstuvwxyz0123456789".contains(character);
            let tail = first || "._/-".contains(character);
            for (raw, accepted) in [
                (format!("{prefix}{character}"), first),
                (format!("{prefix}{character}a"), first),
                (format!("{prefix}a{character}"), tail),
                (format!("{prefix}a{character}b"), tail),
            ] {
                assert_eq!(OwnerId::try_from(raw.clone()).is_ok(), accepted, "{raw:?}");
                let encoded = serde_json::to_string(&raw).unwrap();
                assert_eq!(
                    serde_json::from_str::<OwnerId>(&encoded).is_ok(),
                    accepted,
                    "{raw:?}"
                );
            }
        }
    }
}

#[test]
fn owner_json_preserves_text_and_checks_full_length_and_prefix() {
    for prefix in ["team:", "service:", "user:"] {
        let maximum = 160_usize.saturating_sub(prefix.len());
        for (length, accepted) in [
            (0, false),
            (1, true),
            (maximum.saturating_sub(1), true),
            (maximum, true),
            (maximum.saturating_add(1), false),
        ] {
            let raw = format!("{prefix}{}", "a".repeat(length));
            let encoded = serde_json::to_string(&raw).unwrap();
            let parsed = serde_json::from_str::<OwnerId>(&encoded);
            assert_eq!(parsed.is_ok(), accepted, "{raw:?}");
            if let Ok(owner) = parsed {
                assert_eq!(owner.as_str(), raw);
                assert_eq!(serde_json::to_string(&owner).unwrap(), encoded);
            }
        }
    }
    for raw in [
        "null",
        "true",
        "0",
        "[]",
        "{}",
        "[\"team:a\"]",
        "\"\"",
        "\"group:a\"",
        "\"Team:a\"",
        "\"team:a\\n\"",
        "\"team:a💡\"",
        "\" team:a\"",
    ] {
        assert!(serde_json::from_str::<OwnerId>(raw).is_err(), "{raw}");
    }
}

#[test]
fn controls_request_rejects_invalid_owners_before_authentication() {
    let bytes = include_str!("../../../../spec/examples/scanner-controls-request.json");
    ControlsRequest::parse(bytes.as_bytes()).unwrap();
    for valid in ["team:docs-platform", "team:release-engineering"] {
        for invalid in ["team:", "team:Uppercase", "group:docs"] {
            let changed = bytes.replace(valid, invalid);
            assert_ne!(changed, bytes);
            assert!(
                ControlsRequest::parse(changed.as_bytes()).is_err(),
                "{valid} -> {invalid}"
            );
        }
    }
}

#[test]
fn reports_reject_invalid_owners_even_with_matching_payload_digests() {
    use amiss_wire::digest::hb;
    use amiss_wire::report::{PAYLOAD_SCHEMA, validate_envelope};

    for mut report in super::report_findings::reports() {
        let payload =
            String::from_utf8(serde_json_canonicalizer::to_vec(&report.payload).unwrap()).unwrap();
        report.payload_digest = hb(PAYLOAD_SCHEMA, payload.as_bytes());
        let wire = String::from_utf8(serde_json_canonicalizer::to_vec(&report).unwrap()).unwrap();
        assert_eq!(validate_envelope(wire.as_bytes()).unwrap().0, report);
        for valid in ["team:docs-platform", "team:release-engineering"] {
            if !payload.contains(valid) {
                continue;
            }
            let changed = payload.replace(valid, "team:Invalid");
            assert_ne!(changed, payload);
            let changed = wire.replace(&payload, &changed).replace(
                &report.payload_digest.to_string(),
                &hb(PAYLOAD_SCHEMA, changed.as_bytes()).to_string(),
            );
            assert!(validate_envelope(changed.as_bytes()).is_err(), "{valid}");
        }
    }
}
