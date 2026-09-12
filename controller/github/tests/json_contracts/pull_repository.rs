use amiss_controller::{ProviderError, decode_bounded_json};
use amiss_controller_github::repository::WorkflowRepositoryRecord;

#[test]
fn pull_repository_capture_keeps_shared_identity_without_metadata_requirements() {
    let input: &[u8] = include_bytes!("../fixtures/pull-repository.json");
    let (captured, consumed): (WorkflowRepositoryRecord, _) = decode_bounded_json(
        input,
        Some(u64::try_from(input.len()).unwrap()),
        input.len(),
        |bytes| serde_json::from_slice(bytes),
    )
    .unwrap();
    assert_eq!(consumed, input.len());
    assert_eq!(captured.id, 1_298_463_903);
    assert_eq!(captured.name, "amiss");
    assert_eq!(captured.full_name, "HardMax71/amiss");
    assert_eq!(captured.owner.login, "HardMax71");
    let encoded = serde_json::to_string(&captured).unwrap();
    let metadata = encoded.replacen('{', r#"{"default_branch":null,"disabled":[],"permissions":false,"license":0,"custom_properties":true,"template_repository":{},"extra":null,"#, 1);
    assert_eq!(
        serde_json::from_str::<WorkflowRepositoryRecord>(&metadata).unwrap(),
        captured
    );
    assert_eq!(
        decode_bounded_json::<WorkflowRepositoryRecord, _>(input, None, input.len() - 1, |bytes| {
            serde_json::from_slice(bytes)
        },),
        Err(ProviderError::InvalidResponse)
    );
    assert!(serde_json::from_str::<WorkflowRepositoryRecord>(&format!("{encoded} {{}}")).is_err());
}
