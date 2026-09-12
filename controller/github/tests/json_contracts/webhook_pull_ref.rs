use amiss_controller_github::pull::PullRefRecord;
use amiss_controller_github::webhook::repository::pull::PullRepository;

const REFERENCES: &str = include_str!("../fixtures/webhook-pull-refs.json");

#[test]
fn published_webhook_refs_keep_branch_commit_and_repository_facts() {
    let references: Vec<PullRefRecord<Option<PullRepository>>> =
        serde_json::from_str(REFERENCES).unwrap();
    let [head, base]: &[_; 2] = references.as_slice().try_into().unwrap();
    assert_eq!(head.branch, "changes");
    assert_eq!(base.branch, "master");
    assert_ne!(head.sha, base.sha);
    assert_eq!(base.repo.as_ref().unwrap().id, 186_853_002);
    let bases: Vec<PullRefRecord<PullRepository>> = serde_json::from_str(REFERENCES).unwrap();
    for (optional, required) in references.iter().zip(&bases) {
        assert_eq!(optional.branch, required.branch);
        assert_eq!(optional.sha, required.sha);
        assert_eq!(optional.repo.as_ref().unwrap(), &required.repo);
    }
}

#[test]
fn webhook_ref_repository_type_preserves_nullable_heads_and_required_bases() {
    let mut references: Vec<PullRefRecord<Option<PullRepository>>> =
        serde_json::from_str(REFERENCES).unwrap();
    let reference = references.first_mut().unwrap();
    for repository in [None, reference.repo.clone()] {
        reference.repo = repository;
        let input = serde_json::to_string(reference).unwrap();
        assert!(
            serde_json::from_str::<PullRefRecord<Option<PullRepository>>>(&input).unwrap()
                == *reference
        );
        assert_eq!(
            serde_json::from_str::<PullRefRecord<PullRepository>>(&input).is_ok(),
            reference.repo.is_some()
        );
        let member = format!(
            r#","repo":{}"#,
            serde_json::to_string(&reference.repo).unwrap()
        );
        amiss_fixtures::assert_json_rejections::<PullRefRecord<Option<PullRepository>>>(
            &input,
            &[(&member, "")],
        );
        let metadata = input.replacen('{', r#"{"label":{},"user":null,"unknown":false,"#, 1);
        assert!(
            serde_json::from_str::<PullRefRecord<Option<PullRepository>>>(&metadata).unwrap()
                == *reference
        );
    }
}

#[test]
fn webhook_ref_consumed_fields_stay_typed_and_unique() {
    let references: Vec<PullRefRecord<Option<PullRepository>>> =
        serde_json::from_str(REFERENCES).unwrap();
    let reference = references.first().unwrap();
    let input = serde_json::to_string(reference).unwrap();
    let repository = serde_json::to_string(&reference.repo).unwrap();
    amiss_fixtures::assert_json_rejections::<PullRefRecord<Option<PullRepository>>>(
        &input,
        &[
            (r#""ref":"changes","#, ""),
            (r#""ref":"changes""#, r#""ref":false"#),
            (
                r#""ref":"changes""#,
                r#""ref":"changes","\u0072ef":"other""#,
            ),
            (r#""repo":{"#, r#""\u0072epo":null,"repo":{"#),
            (&repository, "false"),
            (&repository, "{}"),
            (r#""id":186853002"#, r#""id":9007199254740992"#),
            (
                r#""sha":"ec26c3e57ca3a959ca5aad62de7213c562f8c821""#,
                r#""sha":"bad""#,
            ),
        ],
    );
}
