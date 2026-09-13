use amiss_wire::envelope::document_digest;
use amiss_wire::requests::{ControlsRequest, RequestTrust, SuppliedControl};
use serde::de::DeserializeOwned;

#[expect(
    clippy::expect_used,
    reason = "published control fixtures must parse and validate"
)]
fn supplied<T: DeserializeOwned + serde::Serialize>(
    bytes: &[u8],
    domain: &str,
) -> SuppliedControl<T> {
    let value = serde_json::from_slice(bytes).expect("the published control parses");
    let expected_digest = document_digest(domain, &value).expect("the fixture serializes");
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
            "amiss/organization-floor",
        )),
        debt_snapshot: Some(supplied(
            include_bytes!("../../../../spec/examples/debt-snapshot.json"),
            "amiss/debt-snapshot",
        )),
        waiver_bundle: Some(supplied(
            include_bytes!("../../../../spec/examples/waiver-bundle.json"),
            "amiss/waiver-bundle",
        )),
        execution_constraint: Some(supplied(
            include_bytes!("../../../../spec/examples/scanner-execution-constraint.json"),
            "amiss/scanner-execution-constraint",
        )),
        ..ControlsRequest::default()
    };
    let bytes = serde_json_canonicalizer::to_vec(&request).unwrap();
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
}
