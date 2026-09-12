use amiss_controller::{ProviderError, decode_bounded_json};
use amiss_controller_github::repository::RepositoryRecord;

const CAPTURE: &str = include_str!("../fixtures/repository.json");
const FORK: &str = include_str!("../fixtures/repository-fork.json");

#[test]
fn repository_captures_retain_refresh_identity_and_default_branch() {
    for (input, id, owner, name) in [
        (CAPTURE, 1_298_463_903, "HardMax71", "amiss"),
        (FORK, 282_982_518, "rizalgowandy", "rest-api-description"),
    ] {
        let (record, consumed): (RepositoryRecord, _) = decode_bounded_json(
            input.as_bytes(),
            Some(u64::try_from(input.len()).unwrap()),
            input.len(),
            |bytes| serde_json::from_slice(bytes),
        )
        .unwrap();
        assert_eq!(consumed, input.len());
        assert_eq!(record.id, id);
        assert_eq!(record.name, name);
        assert_eq!(record.owner.login, owner);
        assert_eq!(record.full_name, format!("{owner}/{name}"));
        assert_eq!(record.default_branch, "main");
        let encoded = serde_json::to_string(&record).unwrap();
        let metadata = encoded.replacen(
            '{',
            r#"{"parent":false,"source":[],"organization":null,"template_repository":42,"code_of_conduct":{},"security_and_analysis":true,"network_count":-1,"extra":{},"#,
            1,
        );
        assert!(serde_json::from_str::<RepositoryRecord>(&metadata).unwrap() == record);
        let positional = serde_json::to_vec(&(
            record.id,
            &record.name,
            &record.full_name,
            &record.owner,
            &record.default_branch,
        ))
        .unwrap();
        assert!(serde_json::from_slice::<RepositoryRecord>(&positional).unwrap() == record);
        assert!(matches!(
            decode_bounded_json::<RepositoryRecord, _>(
                input.as_bytes(),
                None,
                input.len() - 1,
                |bytes| serde_json::from_slice(bytes),
            ),
            Err(ProviderError::InvalidResponse)
        ));
        assert!(serde_json::from_str::<RepositoryRecord>(&format!("{input} {{}}")).is_err());
    }
}

#[test]
fn repository_refresh_fields_are_required_typed_and_unique() {
    let record: RepositoryRecord = serde_json::from_str(CAPTURE).unwrap();
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
        (
            "default_branch",
            serde_json::to_string(&record.default_branch).unwrap(),
        ),
    ] {
        let original = format!(r#""{field}":{value}"#);
        for replacement in [
            format!(r#""missing_{field}":{value}"#),
            format!(r#""{field}":null"#),
            format!(r#""{field}":false"#),
            format!(r#""{field}":{value},"{field}":{value}"#),
        ] {
            amiss_fixtures::assert_json_rejections::<RepositoryRecord>(
                &encoded,
                &[(&original, &replacement)],
            );
        }
    }
    amiss_fixtures::assert_json_rejections::<RepositoryRecord>(
        &encoded,
        &[(r#""full_name":"#, r#""\u0066ull_name":null,"full_name":"#)],
    );
}

#[test]
fn repository_identity_keeps_exact_integer_bounds() {
    let mut record: RepositoryRecord = serde_json::from_str(CAPTURE).unwrap();
    let original = format!(r#""id":{}"#, record.id);
    for invalid in ["-1", "1.5", "9007199254740992", r#""1298463903""#] {
        amiss_fixtures::assert_json_rejections::<RepositoryRecord>(
            CAPTURE,
            &[(&original, &format!(r#""id":{invalid}"#))],
        );
    }
    for id in [0, js_int::MAX_SAFE_UINT] {
        record.id = id;
        let encoded = serde_json::to_vec(&record).unwrap();
        assert!(serde_json::from_slice::<RepositoryRecord>(&encoded).unwrap() == record);
    }
    record.id = js_int::MAX_SAFE_UINT + 1;
    assert!(serde_json::to_vec(&record).is_err());
}
