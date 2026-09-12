use amiss_wire::{controls, semantic};

const DEBT: &[u8] = include_bytes!("../../../../spec/examples/debt-snapshot.json");
const FLOOR: &[u8] = include_bytes!("../../../../spec/examples/organization-floor.json");
const WAIVER: &[u8] = include_bytes!("../../../../spec/examples/waiver-bundle.json");

#[test]
fn typed_control_readers_keep_the_closed_input_contract() -> Result<(), Box<dyn std::error::Error>>
{
    let read_debt: fn(&[u8]) -> bool = |bytes| controls::parse_debt_snapshot(bytes).is_ok();
    let readers = [
        (
            DEBT,
            controls::DebtSnapshotSchema::Current.to_string(),
            None,
            read_debt,
        ),
        (
            FLOOR,
            controls::OrganizationFloorSchema::Current.to_string(),
            None,
            |bytes| controls::parse_organization_floor(bytes).is_ok(),
        ),
        (
            WAIVER,
            controls::WaiverBundleSchema::Current.to_string(),
            None,
            |bytes| controls::parse_waiver_bundle(bytes).is_ok(),
        ),
        (
            include_bytes!("../../../../spec/examples/scanner-policy.json"),
            controls::ScannerPolicySchema::Current.to_string(),
            None,
            |bytes| controls::parse_scanner_policy(bytes).is_ok(),
        ),
        (
            include_bytes!("../../../../spec/examples/scanner-semantic-evidence.json"),
            semantic::EnvelopeSchema::Current.to_string(),
            Some(semantic::SEMANTIC_EVIDENCE_BYTES),
            |bytes| semantic::parse(bytes).is_ok(),
        ),
        (
            include_bytes!("../../../../spec/examples/scanner-semantic-template.json"),
            semantic::TemplateSchema::Current.to_string(),
            Some(semantic::SEMANTIC_EVIDENCE_BYTES),
            |bytes| semantic::parse_template(bytes).is_ok(),
        ),
        (
            include_bytes!("../../../../spec/examples/scanner-record-set-input.json"),
            semantic::record::InputSchema::Current.to_string(),
            Some(semantic::SEMANTIC_EVIDENCE_BYTES),
            |bytes| semantic::record::parse_input(bytes).is_ok(),
        ),
    ];
    for (bytes, schema, limit, read) in readers {
        super::input::assert_closed_input(bytes, &schema, limit, read)?;
    }
    let debt = controls::parse_debt_snapshot(DEBT)?;
    let fact = &debt.items[0].accepted_fact;
    let (bytes, _digest) = controls::canonical_fact(fact)?;
    super::input::assert_closed_input(&bytes, &fact.schema.to_string(), None, |bytes| {
        controls::parse_fact(bytes).is_ok()
    })?;
    Ok(())
}

#[test]
fn typed_control_numbers_and_nested_duplicate_keys_are_rejected()
-> Result<(), Box<dyn std::error::Error>> {
    let debt = controls::parse_debt_snapshot(DEBT)?;
    let fact = serde_json::to_string(&debt.items[0].accepted_fact)?;
    let debt = std::str::from_utf8(DEBT)?;
    let floor = std::str::from_utf8(FLOOR)?;
    let waiver = std::str::from_utf8(WAIVER)?;
    let rejects_debt: fn(&[u8]) -> bool = |bytes| controls::parse_debt_snapshot(bytes).is_err();
    let readers = [
        (debt, rejects_debt),
        (waiver, |bytes| {
            controls::parse_waiver_bundle(bytes).is_err()
        }),
    ];
    for number in ["-0", "9007199254740992", "1.0", "1e0"] {
        let member = format!("\"occurrence_multiplicity\": {number}");
        for (text, rejects) in readers {
            let changed = text.replace("\"occurrence_multiplicity\": 1", &member);
            assert_ne!(changed, text);
            assert!(rejects(changed.as_bytes()), "{changed}");
        }
        let changed = fact.replace("\"occurrence_multiplicity\":1", &member);
        assert_ne!(changed, fact);
        assert!(controls::parse_fact(changed.as_bytes()).is_err());
        let changed = floor.replace("\"maximum\": 64", &format!("\"maximum\": {number}"));
        assert_ne!(changed, floor);
        assert!(controls::parse_organization_floor(changed.as_bytes()).is_err());
    }
    for key in ["host", r"\u0068ost"] {
        for (text, rejects) in [
            readers[0],
            readers[1],
            (floor, |bytes| {
                controls::parse_organization_floor(bytes).is_err()
            }),
        ] {
            let changed = text.replace(
                "\"host\":",
                &format!("\"{key}\":\"gitlab.example.internal\",\"host\":"),
            );
            assert_ne!(changed, text);
            assert!(rejects(changed.as_bytes()));
        }
    }
    Ok(())
}

#[test]
fn standalone_projection_input_rejects_ambiguous_members_and_numbers()
-> Result<(), Box<dyn std::error::Error>> {
    let source = controls::ProjectionSource::BlobLines(controls::BlobLineSelection {
        path: "src/lib.rs".parse()?,
        first_line: 1,
        last_line: 2,
    });
    let text = serde_json::to_string(&source)?;
    let projection = controls::ProjectionKind::CodeTextV1;
    assert_eq!(
        controls::parse_projection_source(text.as_bytes(), projection)?,
        source
    );
    for value in ["-0", "9007199254740992", "2.0", "2e0"] {
        let changed = text.replace("\"last_line\":2", &format!("\"last_line\":{value}"));
        assert_ne!(changed, text);
        assert!(controls::parse_projection_source(changed.as_bytes(), projection).is_err());
    }
    for key in ["last_line", r"\u006cast_line"] {
        let changed = text.replace("\"last_line\":2", &format!("\"last_line\":2,\"{key}\":2"));
        assert_ne!(changed, text);
        assert!(controls::parse_projection_source(changed.as_bytes(), projection).is_err());
    }
    for changed in [
        format!("{text}{{}}"),
        text.replacen('{', "{\"future\":null,", 1),
        text.replace("\"path\":\"src/lib.rs\"", "\"path\":null"),
    ] {
        assert_ne!(changed, text);
        assert!(controls::parse_projection_source(changed.as_bytes(), projection).is_err());
    }
    Ok(())
}
