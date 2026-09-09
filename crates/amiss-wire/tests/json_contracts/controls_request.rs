use amiss_wire::{
    controls::{DebtSnapshot, FactEvidence, ResourceLimit, ResourceName},
    requests::{ControlsRequest, SuppliedSemanticEvidence, SuppliedTime},
    semantic,
};

const CONTROLS: &[u8] = include_bytes!("../../../../spec/examples/scanner-controls-request.json");

#[test]
fn fact_multiplicity_is_checked_by_serde() -> Result<(), Box<dyn std::error::Error>> {
    let debt: DebtSnapshot = serde_json::from_slice(include_bytes!(
        "../../../../spec/examples/debt-snapshot.json"
    ))?;
    let evidence = &debt.items[0].accepted_fact.evidence;
    super::report_numbers::assert_safe_numbers(|occurrence_multiplicity| FactEvidence {
        occurrence_multiplicity,
        ..evidence.clone()
    })
}

#[test]
fn supplied_run_attempt_is_checked_by_serde() -> Result<(), Box<dyn std::error::Error>> {
    let supplied = ControlsRequest::parse(CONTROLS)?.trusted_time.unwrap();
    super::report_numbers::assert_safe_numbers(|provider_run_attempt| SuppliedTime {
        provider_run_attempt,
        ..supplied.clone()
    })
}

#[test]
fn resource_limit_numbers_are_checked_by_serde() -> Result<(), Box<dyn std::error::Error>> {
    let maximum = amiss_wire::json::MAX_SAFE_INTEGER;
    let mut limit = ResourceLimit {
        resource: ResourceName::MachineJsonBytes,
        maximum,
    };
    let text = serde_json::to_string(&limit)?;
    for valid in [-maximum, -1, 0, maximum] {
        limit.maximum = valid;
        let encoded = serde_json::to_string(&limit)?;
        assert_eq!(serde_json::from_str::<ResourceLimit>(&encoded)?, limit);
    }
    for invalid in [i64::MIN, -maximum - 1, maximum + 1, i64::MAX] {
        limit.maximum = invalid;
        let changed = text.replace(&maximum.to_string(), &invalid.to_string());
        assert_ne!(changed, text);
        assert_eq!(
            [
                serde_json::from_str::<ResourceLimit>(&changed).is_err(),
                serde_json::to_vec(&limit).is_err(),
                serde_json_canonicalizer::to_vec(&limit).is_err(),
            ],
            [true; 3],
            "{invalid}"
        );
    }
    for invalid in ["-0", "1.0", "1e0", "null", "false", "[]", "{}", "\"1\""] {
        let changed = text.replace(&maximum.to_string(), invalid);
        assert_ne!(changed, text);
        assert!(serde_json::from_str::<ResourceLimit>(&changed).is_err());
    }
    Ok(())
}

#[test]
fn controls_request_root_and_supplied_records_require_objects()
-> Result<(), Box<dyn std::error::Error>> {
    let document = semantic::parse(include_bytes!(
        "../../../../spec/examples/scanner-semantic-evidence.json"
    ))?;
    let mut request = ControlsRequest::parse(CONTROLS)?;
    request.semantic_evidence.push(SuppliedSemanticEvidence {
        expected_context_digest: document.payload.producer.context_digest,
        value: document.into(),
    });
    super::input::assert_object_required(
        (&request, ControlsRequest::parse),
        &request,
        (
            request.schema,
            &request.organization_floor,
            &request.debt_snapshot,
            &request.waiver_bundle,
            &request.trusted_time,
            &request.execution_constraint,
            &request.semantic_evidence,
        ),
    )?;
    let floor = request.organization_floor.as_ref().unwrap();
    let time = request.trusted_time.as_ref().unwrap();
    let evidence = &request.semantic_evidence[0];
    let text = serde_json::to_string(&request)?;
    let cases = [
        (
            serde_json::to_string(floor)?,
            serde_json::to_string(&(&floor.value, floor.expected_digest, floor.trust_source))?,
        ),
        (
            serde_json::to_string(time)?,
            serde_json::to_string(&(
                &time.value,
                time.expected_digest,
                &time.provider,
                &time.provider_run_id,
                time.provider_run_attempt,
            ))?,
        ),
        (
            serde_json::to_string(evidence)?,
            serde_json::to_string(&(&evidence.value, evidence.expected_context_digest))?,
        ),
    ];
    let mut accepted = Vec::new();
    for (object, positional) in cases {
        assert_eq!(text.matches(&object).count(), 1);
        let changed = text.replacen(&object, &positional, 1);
        assert_ne!(changed, text);
        assert_eq!(serde_json::from_str::<ControlsRequest>(&changed)?, request);
        accepted.push(ControlsRequest::parse(changed.as_bytes()).is_ok());
    }
    assert_eq!(accepted, [false; 3]);
    let canonical = request.canonical_bytes()?;
    assert_eq!(canonical, serde_json_canonicalizer::to_vec(&request)?);
    assert_eq!(ControlsRequest::parse(&canonical)?, request);
    Ok(())
}
