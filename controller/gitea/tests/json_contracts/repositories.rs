use amiss_controller::{ProviderError, decode_bounded_json};
use amiss_controller_gitea::repository::RepositoryRecord;
use amiss_wire::model::ObjectFormat;

#[test]
fn repository_captures_retain_identity_and_ignore_unconsumed_metadata()
-> Result<(), Box<dyn std::error::Error>> {
    for (input, id, owner, manual_merge) in [
        (
            include_str!("../fixtures/gitea-repository.json"),
            101,
            "acme",
            Some(false),
        ),
        (
            include_str!("../fixtures/forgejo-repository.json"),
            101,
            "acme",
            None,
        ),
        (
            include_str!("../fixtures/gitea-fork.json"),
            202,
            "contributor",
            Some(false),
        ),
    ] {
        let (repository, length): (RepositoryRecord, _) =
            decode_bounded_json(input.as_bytes(), None, input.len(), |bytes| {
                serde_json::from_slice(bytes)
            })?;
        assert_eq!(length, input.len());
        assert_eq!(repository.id, id);
        assert_eq!(repository.name, "widget");
        assert_eq!(repository.full_name, format!("{owner}/widget"));
        assert_eq!(repository.owner.login, owner);
        assert_eq!(repository.default_branch, "main");
        assert_eq!(repository.object_format_name, ObjectFormat::Sha1);
        assert_eq!(repository.allow_manual_merge, manual_merge);
        assert_eq!(
            decode_bounded_json::<RepositoryRecord, _>(
                input.as_bytes(),
                None,
                input.len() - 1,
                |bytes| serde_json::from_slice(bytes),
            ),
            Err(ProviderError::InvalidResponse)
        );
        let encoded = serde_json::to_string(&repository)?;
        let metadata = encoded.replacen(
            '{',
            r#"{"parent":false,"repo_transfer":[],"permissions":null,"topics":{},"size":-1,"default_merge_style":"future","extra":{},"#,
            1,
        );
        assert_eq!(
            serde_json::from_str::<RepositoryRecord>(&metadata)?,
            repository
        );
    }
    Ok(())
}

#[test]
fn repository_identity_fields_remain_required_and_unique() -> Result<(), Box<dyn std::error::Error>>
{
    let repository: RepositoryRecord =
        serde_json::from_str(include_str!("../fixtures/gitea-repository.json"))?;
    let encoded = serde_json::to_string(&repository)?;
    for (field, value) in [
        ("id", repository.id.to_string()),
        ("name", serde_json::to_string(&repository.name)?),
        ("full_name", serde_json::to_string(&repository.full_name)?),
        ("owner", serde_json::to_string(&repository.owner)?),
        (
            "default_branch",
            serde_json::to_string(&repository.default_branch)?,
        ),
        (
            "object_format_name",
            serde_json::to_string(&repository.object_format_name)?,
        ),
    ] {
        let original = format!(r#""{field}":{value}"#);
        for replacement in [
            format!(r#""missing_{field}":{value}"#),
            format!(r#""{field}":{value},"{field}":{value}"#),
            format!(r#""{field}":null"#),
        ] {
            amiss_fixtures::assert_json_rejections::<RepositoryRecord>(
                &encoded,
                &[(&original, &replacement)],
            );
        }
    }
    amiss_fixtures::assert_json_rejections::<RepositoryRecord>(
        &encoded,
        &[
            (r#""id":101"#, r#""id":-1"#),
            (r#""id":101"#, r#""id":9007199254740992"#),
            (r#""id":101"#, r#""id":101,"\u0069d":101"#),
            (r#""name":"widget""#, r#""name":false"#),
            (r#""owner":{"#, r#""owner":{"id":false,"#),
            (
                r#""object_format_name":"sha1""#,
                r#""object_format_name":"unknown""#,
            ),
        ],
    );
    for invalid in ["null", "true", "42", "{}", "[]", "[{}]"] {
        assert_eq!(
            decode_bounded_json::<RepositoryRecord, _>(
                invalid.as_bytes(),
                None,
                invalid.len(),
                |bytes| serde_json::from_slice(bytes),
            ),
            Err(ProviderError::InvalidResponse)
        );
    }
    Ok(())
}

#[test]
fn manual_merge_capability_preserves_omission_without_accepting_null()
-> Result<(), Box<dyn std::error::Error>> {
    let mut repository: RepositoryRecord =
        serde_json::from_str(include_str!("../fixtures/gitea-repository.json"))?;
    for capability in [None, Some(false), Some(true)] {
        repository.allow_manual_merge = capability;
        let encoded = serde_json::to_string(&repository)?;
        assert_eq!(encoded.contains("allow_manual_merge"), capability.is_some());
        assert_eq!(
            serde_json::from_str::<RepositoryRecord>(&encoded)?,
            repository
        );
    }
    let encoded = serde_json::to_string(&repository)?;
    amiss_fixtures::assert_json_rejections::<RepositoryRecord>(
        &encoded,
        &[
            (
                r#""allow_manual_merge":true"#,
                r#""allow_manual_merge":null"#,
            ),
            (
                r#""allow_manual_merge":true"#,
                r#""allow_manual_merge":"true""#,
            ),
            (r#""allow_manual_merge":true"#, r#""allow_manual_merge":1"#),
            (
                r#""allow_manual_merge":true"#,
                r#""allow_manual_merge":false,"allow_manual_merge":true"#,
            ),
        ],
    );
    Ok(())
}
