use amiss_controller::{ProviderError, decode_bounded_json};
use amiss_controller_github::owner::OwnerRecord;
use amiss_controller_github::repository::WorkflowRepositoryRecord;
use amiss_controller_github::webhook::repository::WorkflowOwner;

#[test]
fn workflow_webhook_repository_keeps_only_the_shared_identity_facts() {
    let input: &[u8] = include_bytes!("../fixtures/webhook-workflow-repository.json");
    let (record, consumed): (WorkflowRepositoryRecord<Option<OwnerRecord>>, _) =
        decode_bounded_json(input, None, input.len(), |bytes| {
            serde_json::from_slice(bytes)
        })
        .unwrap();
    assert_eq!(consumed, input.len());
    assert_eq!(record.id, 300_029_405);
    assert_eq!(record.name, "octo-repo");
    assert_eq!(record.full_name, "octo-org/octo-repo");
    assert_eq!(record.owner.as_ref().unwrap().login, "octo-org");
    let encoded = serde_json::to_string(&record).unwrap();
    let metadata = encoded.replacen(
        '{',
        r#"{"node_id":null,"private":{},"description":[],"fork":false,"html_url":0,"extra":true,"#,
        1,
    );
    assert_eq!(
        serde_json::from_str::<WorkflowRepositoryRecord<Option<OwnerRecord>>>(&metadata).unwrap(),
        record
    );
    assert_eq!(
        decode_bounded_json::<WorkflowRepositoryRecord<Option<OwnerRecord>>, _>(
            input,
            None,
            input.len() - 1,
            |bytes| serde_json::from_slice(bytes),
        ),
        Err(ProviderError::InvalidResponse)
    );
}

#[test]
fn shared_workflow_repository_requires_a_nullable_owner_without_widening_rest() {
    let record = WorkflowRepositoryRecord {
        id: 1,
        name: "repo".to_owned(),
        full_name: "owner/repo".to_owned(),
        owner: None::<OwnerRecord>,
    };
    let input = serde_json::to_string(&record).unwrap();
    assert_eq!(
        serde_json::from_str::<WorkflowRepositoryRecord<Option<OwnerRecord>>>(&input).unwrap(),
        record
    );
    assert!(serde_json::from_str::<WorkflowRepositoryRecord>(&input).is_err());
    amiss_fixtures::assert_json_rejections::<WorkflowRepositoryRecord<Option<OwnerRecord>>>(
        &input,
        &[
            (r#","owner":null"#, ""),
            (r#""owner":null"#, r#""owner":false"#),
            (r#""owner":null"#, r#""owner":{}"#),
            (r#""owner":null"#, r#""owner":{"login":null}"#),
            (r#""owner":null"#, r#""owner":null,"\u006fwner":null"#),
            (r#""id":1"#, r#""missing_id":1"#),
            (r#""id":1"#, r#""id":9007199254740992"#),
            (r#""id":1"#, r#""id":-0"#),
            (r#""id":1"#, r#""id":1e0"#),
            (r#""id":1"#, r#""id":null"#),
            (r#""name":"repo""#, r#""name":null"#),
            (r#""name":"repo""#, r#""missing_name":"repo""#),
            (
                r#""full_name":"owner/repo""#,
                r#""missing_full_name":"owner/repo""#,
            ),
        ],
    );
    let present = input.replace(r#""owner":null"#, r#""owner":{"login":"owner"}"#);
    let rest: WorkflowRepositoryRecord = serde_json::from_str(&present).unwrap();
    let webhook: WorkflowRepositoryRecord<Option<OwnerRecord>> =
        serde_json::from_str(&present).unwrap();
    assert_eq!(webhook.owner, Some(rest.owner));
    assert_eq!(webhook.id, rest.id);
    assert_eq!(webhook.name, rest.name);
    assert_eq!(webhook.full_name, rest.full_name);
}

#[test]
fn workflow_owner_presence_and_account_kinds_follow_the_declared_contract() {
    for input in [
        r#"{"login":"owner","id":1}"#,
        r#"{"login":"owner","id":1,"email":null,"deleted":false,"user_view_type":"public"}"#,
        r#"{"login":"owner","id":1,"email":"owner@example.com","name":"Owner","type":"User"}"#,
        r#"{"login":"owner","id":9007199254740991,"type":"Bot"}"#,
    ] {
        let owner: WorkflowOwner = serde_json::from_str(input).unwrap();
        let encoded = serde_json::to_vec(&owner).unwrap();
        assert_eq!(
            amiss_fixtures::canonical_json(input.as_bytes()).unwrap(),
            amiss_fixtures::canonical_json(&encoded).unwrap(),
        );
        assert_eq!(
            amiss_wire::read_json::<WorkflowOwner>(input.as_bytes(), u64::MAX).unwrap(),
            owner,
        );
    }
    for input in [
        r#"{"login":"owner"}"#,
        r#"{"id":1}"#,
        r#"{"login":null,"id":1}"#,
        r#"{"login":"owner","id":null}"#,
        r#"{"login":"owner","id":9007199254740992}"#,
        r#"{"login":"owner","id":1.0}"#,
        r#"{"login":"owner","id":1e0}"#,
        r#"{"login":"owner","id":-0}"#,
        r#"{"login":"owner","\u006cogin":"duplicate","id":1}"#,
        r#"{"login":"owner","id":1,"name":null}"#,
        r#"{"login":"owner","id":1,"gravatar_id":null}"#,
        r#"{"login":"owner","id":1,"deleted":null}"#,
        r#"{"login":"owner","id":1,"type":null}"#,
        r#"{"login":"owner","id":1,"type":"unknown"}"#,
        r#"{"login":"owner","id":1,"type":{"User":null}}"#,
        r#"{"login":"owner","id":1,"email":7}"#,
    ] {
        assert!(
            serde_json::from_str::<WorkflowOwner>(input).is_err(),
            "{input}"
        );
        assert!(
            amiss_wire::read_json::<WorkflowOwner>(input.as_bytes(), u64::MAX).is_err(),
            "{input}"
        );
    }
    let input = r#"{"login":"owner","id":1,"unknown":false}"#;
    let owner: WorkflowOwner = serde_json::from_str(input).unwrap();
    assert_eq!(owner.login, "owner");
    assert_eq!(owner.id, js_int::uint!(1));
    assert!(amiss_wire::read_json::<WorkflowOwner>(input.as_bytes(), u64::MAX).is_err());
}
