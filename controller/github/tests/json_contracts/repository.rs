use amiss_controller_github::repository::WorkflowRepositoryRecord;

const CAPTURE: &str = include_str!("../fixtures/workflow-repository.json");

#[test]
fn workflow_repository_keeps_artifact_identity_and_ignores_metadata() {
    let record: WorkflowRepositoryRecord = serde_json::from_str(CAPTURE).unwrap();
    assert_eq!(record.id, 1_298_463_903);
    assert_eq!(record.name, "amiss");
    assert_eq!(record.full_name, "HardMax71/amiss");
    assert_eq!(record.owner.login, "HardMax71");
    let encoded = serde_json::to_string(&record).unwrap();
    let metadata = encoded.replacen(
        '{',
        r#"{"permissions":false,"license":[],"code_of_conduct":null,"security_and_analysis":{"future":[]},"size":-1,"custom_properties":true,"extra":{},"#,
        1,
    );
    assert_eq!(
        serde_json::from_str::<WorkflowRepositoryRecord>(&metadata).unwrap(),
        record
    );
}

#[test]
fn workflow_repository_identity_is_required_typed_and_unique() {
    let record: WorkflowRepositoryRecord = serde_json::from_str(CAPTURE).unwrap();
    let encoded = serde_json::to_string(&record).unwrap();
    for (field, value) in [
        ("id", record.id.to_string()),
        ("name", serde_json::to_string(&record.name).unwrap()),
        (
            "full_name",
            serde_json::to_string(&record.full_name).unwrap(),
        ),
        ("owner", serde_json::to_string(&record.owner).unwrap()),
        ("login", serde_json::to_string(&record.owner.login).unwrap()),
    ] {
        let original = format!(r#""{field}":{value}"#);
        for replacement in [
            format!(r#""missing_{field}":{value}"#),
            format!(r#""{field}":null"#),
            format!(r#""{field}":false"#),
            format!(r#""{field}":{value},"{field}":{value}"#),
        ] {
            amiss_fixtures::assert_json_rejections::<WorkflowRepositoryRecord>(
                &encoded,
                &[(&original, &replacement)],
            );
        }
    }
    let original = format!(r#""id":{}"#, record.id);
    for invalid in ["-1", "1.5", "9007199254740992"] {
        amiss_fixtures::assert_json_rejections::<WorkflowRepositoryRecord>(
            &encoded,
            &[(&original, &format!(r#""id":{invalid}"#))],
        );
    }
}
