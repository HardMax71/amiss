use amiss_wire::{
    controls::{
        canonical_debt_snapshot, canonical_execution_constraint, canonical_organization_floor,
        canonical_waiver_bundle,
    },
    digest::Digest,
    requests::{ControlsRequest, RequestTrust, SuppliedControl},
};
use serde::de::DeserializeOwned;

#[expect(
    clippy::expect_used,
    reason = "published control fixtures must parse and validate"
)]
fn supplied<T: DeserializeOwned, E: std::fmt::Debug>(
    bytes: &[u8],
    canonical: impl FnOnce(&T) -> Result<(Vec<u8>, Digest), E>,
) -> SuppliedControl<T> {
    let value = serde_json::from_slice(bytes).expect("the published control parses");
    let expected_digest = canonical(&value)
        .expect("the published control validates")
        .1;
    SuppliedControl {
        value,
        expected_digest,
        trust_source: RequestTrust::OrganizationPolicy,
    }
}

#[test]
fn control_inputs_keep_their_concrete_shapes_and_identities() {
    let request = ControlsRequest {
        organization_floor: Some(supplied(
            include_bytes!("../../../../spec/examples/organization-floor.json"),
            canonical_organization_floor,
        )),
        debt_snapshot: Some(supplied(
            include_bytes!("../../../../spec/examples/debt-snapshot.json"),
            canonical_debt_snapshot,
        )),
        waiver_bundle: Some(supplied(
            include_bytes!("../../../../spec/examples/waiver-bundle.json"),
            canonical_waiver_bundle,
        )),
        execution_constraint: Some(supplied(
            include_bytes!("../../../../spec/examples/scanner-execution-constraint.json"),
            canonical_execution_constraint,
        )),
        ..ControlsRequest::default()
    };
    let bytes = request.canonical_bytes().unwrap();
    assert_eq!(ControlsRequest::parse(&bytes).unwrap(), request);
    let encoded = serde_json::to_string(&request).unwrap();
    let floor = &request.organization_floor.as_ref().unwrap().value;
    for object in [
        serde_json::to_string(floor).unwrap(),
        serde_json::to_string(&request.debt_snapshot.as_ref().unwrap().value).unwrap(),
        serde_json::to_string(&request.waiver_bundle.as_ref().unwrap().value).unwrap(),
        serde_json::to_string(&request.execution_constraint.as_ref().unwrap().value).unwrap(),
    ] {
        for invalid in [
            object.replacen('{', "{\"future\": true,", 1),
            "null".to_owned(),
            "[]".to_owned(),
            "{}".to_owned(),
            "42".to_owned(),
        ] {
            let altered = encoded.replace(&object, &invalid);
            assert_ne!(altered, encoded);
            assert!(
                ControlsRequest::parse(altered.as_bytes()).is_err(),
                "{invalid}"
            );
            assert!(
                serde_json::from_str::<ControlsRequest>(&altered).is_err(),
                "{invalid}"
            );
        }
    }

    let positional = serde_json::to_string(&(
        floor.schema,
        &floor.floor_id,
        &floor.repository,
        &floor.ref_name,
        floor.minimum_profile,
        &floor.minimum_dispositions,
        &floor.protected_inventory,
        &floor.protected_control_paths,
        &floor.waivable_finding_kinds,
        &floor.authorized_debt_owners,
        &floor.authorized_waiver_issuers,
        &floor.resource_limits,
    ))
    .unwrap();
    let altered = encoded.replace(&serde_json::to_string(floor).unwrap(), &positional);
    assert_ne!(altered, encoded);
    assert!(ControlsRequest::parse(altered.as_bytes()).is_err());
    assert!(serde_json::from_str::<ControlsRequest>(&altered).is_err());
}
