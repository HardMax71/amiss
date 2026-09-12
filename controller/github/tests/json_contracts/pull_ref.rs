use amiss_controller::{ProviderError, decode_bounded_json};
use amiss_controller_github::pull::PullRefRecord;

const CAPTURE: &str = include_str!("../fixtures/pull-refs.json");

#[test]
fn native_pull_refs_preserve_open_and_deleted_head_facts() {
    let (refs, consumed): (Vec<PullRefRecord>, _) = decode_bounded_json(
        CAPTURE.as_bytes(),
        Some(u64::try_from(CAPTURE.len()).unwrap()),
        CAPTURE.len(),
        |bytes| serde_json::from_slice(bytes),
    )
    .unwrap();
    assert_eq!(consumed, CAPTURE.len());
    assert_eq!(refs.len(), 4);
    let head = refs.first().unwrap();
    assert_eq!(head.branch, "github/typed-pull-repositories");
    assert_eq!(head.repo.as_ref().unwrap().full_name, "HardMax71/amiss");
    let deleted = refs
        .iter()
        .find(|reference| reference.repo.is_none())
        .unwrap();
    assert_eq!(
        deleted.sha.as_str(),
        "c1b91fa95dc826be3e1df515b854074b540d8030"
    );
    assert!(matches!(
        decode_bounded_json::<Vec<PullRefRecord>, _>(
            CAPTURE.as_bytes(),
            None,
            CAPTURE.len() - 1,
            |bytes| serde_json::from_slice(bytes),
        ),
        Err(ProviderError::InvalidResponse)
    ));
    let trailing = format!("{CAPTURE} {{}}");
    assert!(serde_json::from_str::<Vec<PullRefRecord>>(&trailing).is_err());
}

#[test]
fn nullable_ref_repositories_do_not_make_reference_fields_optional() {
    let refs: Vec<PullRefRecord> = serde_json::from_str(CAPTURE).unwrap();
    let deleted = refs
        .iter()
        .find(|reference| reference.repo.is_none())
        .unwrap();
    let input = serde_json::to_string(deleted).unwrap();
    assert!(serde_json::from_str::<PullRefRecord>(&input).unwrap() == *deleted);
    for (field, value) in [
        ("ref", serde_json::to_string(&deleted.branch).unwrap()),
        ("sha", serde_json::to_string(&deleted.sha).unwrap()),
    ] {
        let member = format!(r#""{field}":{value},"#);
        amiss_fixtures::assert_json_rejections::<PullRefRecord>(
            &input,
            &[
                (&member, ""),
                (&member, &format!(r#""{field}":null,"#)),
                (&member, &format!(r#""{field}":false,"#)),
                (&member, &format!("{member}{member}")),
            ],
        );
    }
    amiss_fixtures::assert_json_rejections::<PullRefRecord>(
        &input,
        &[
            (r#","repo":null"#, ""),
            (r#""repo":null"#, r#""repo":false"#),
            (r#""repo":null"#, r#""repo":{}"#),
            (r#""repo":null"#, r#""repo":null,"\u0072epo":null"#),
        ],
    );
}

#[test]
fn pull_ref_metadata_is_ignored_without_hiding_invalid_references() {
    let refs: Vec<PullRefRecord> = serde_json::from_str(CAPTURE).unwrap();
    for reference in refs {
        let input = serde_json::to_string(&reference).unwrap();
        let metadata = input.replacen(
            '{',
            r#"{"label":null,"user":false,"future":{"anything":[]},"#,
            1,
        );
        assert!(serde_json::from_str::<PullRefRecord>(&metadata).unwrap() == reference);
        let sha = serde_json::to_string(&reference.sha).unwrap();
        amiss_fixtures::assert_json_rejections::<PullRefRecord>(
            &input,
            &[
                (r#""ref":"#, r#""\u0072ef":"other","ref":"#),
                (&sha, r#""not-an-oid""#),
            ],
        );
    }
}
