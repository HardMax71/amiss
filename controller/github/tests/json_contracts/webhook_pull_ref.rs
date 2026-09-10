use amiss_controller_github::pull::PullRefRecord;
use amiss_controller_github::webhook::repository::WorkflowOwner;
use amiss_controller_github::webhook::repository::pull::PullRepository;

const REFERENCES: &str = include_str!("../fixtures/webhook-pull-refs.json");

#[test]
fn published_webhook_refs_keep_all_five_fields() {
    let references: Vec<PullRefRecord<Option<WorkflowOwner>, Option<PullRepository>>> =
        serde_json::from_str(REFERENCES).unwrap();
    assert_eq!(references.len(), 2);
    assert!(
        amiss_fixtures::canonical_json(&serde_json::to_vec(&references).unwrap()).unwrap()
            == amiss_fixtures::canonical_json(REFERENCES.as_bytes()).unwrap(),
        "reference metadata was discarded",
    );
    let [head, base]: &[_; 2] = references.as_slice().try_into().unwrap();
    assert_eq!(head.label, "Codertocat:changes");
    assert_eq!(head.branch, "changes");
    assert_eq!(base.label, "Codertocat:master");
    assert_eq!(base.branch, "master");
    assert_ne!(head.sha, base.sha);
    assert_eq!(head.user.as_ref().unwrap().id, js_int::uint!(21_031_067));
    assert_eq!(base.repo.as_ref().unwrap().id, 186_853_002);
    let bases: Vec<PullRefRecord<Option<WorkflowOwner>, PullRepository>> =
        amiss_wire::read_json(REFERENCES.as_bytes(), u64::MAX).unwrap();
    assert_eq!(
        amiss_fixtures::canonical_json(&serde_json::to_vec(&bases).unwrap()).unwrap(),
        amiss_fixtures::canonical_json(REFERENCES.as_bytes()).unwrap(),
    );
    assert!(
        amiss_wire::read_json::<Vec<PullRefRecord<Option<WorkflowOwner>, Option<PullRepository>>>>(
            REFERENCES.as_bytes(),
            0,
        )
        .is_err(),
    );
}

#[test]
fn webhook_ref_parameters_preserve_nullable_head_and_required_base_repositories() {
    let mut references: Vec<PullRefRecord<Option<WorkflowOwner>, Option<PullRepository>>> =
        serde_json::from_str(REFERENCES).unwrap();
    let reference = references.first_mut().unwrap();
    let mut repository = reference.repo.clone().unwrap();
    repository.created_at = serde_json::from_str("-1").unwrap();
    let user: WorkflowOwner = serde_json::from_str(r#"{"login":"owner","id":1}"#).unwrap();
    for (user, repository) in [
        (None, None),
        (Some(user.clone()), None),
        (None, Some(repository.clone())),
        (Some(user), Some(repository)),
    ] {
        reference.user = user;
        reference.repo = repository;
        let encoded = serde_json::to_vec(reference).unwrap();
        assert!(
            amiss_wire::read_json::<PullRefRecord<Option<WorkflowOwner>, Option<PullRepository>>>(
                &encoded,
                u64::MAX,
            )
            .unwrap()
                == *reference,
        );
        assert_eq!(
            serde_json::from_slice::<PullRefRecord<Option<WorkflowOwner>, PullRepository>>(
                &encoded
            )
            .is_ok(),
            reference.repo.is_some(),
        );
        assert!(serde_json::from_slice::<PullRefRecord>(&encoded).is_err());
    }
}

#[test]
fn generic_ref_members_remain_required_even_when_their_types_accept_null() {
    let mut references: Vec<PullRefRecord<Option<WorkflowOwner>, Option<PullRepository>>> =
        serde_json::from_str(REFERENCES).unwrap();
    let nullable = references.first_mut().unwrap();
    nullable.user = None;
    nullable.repo = None;
    for reference in references {
        let wire = serde_json::to_string(&reference).unwrap();
        for (name, value) in [
            ("label", serde_json::to_string(&reference.label).unwrap()),
            ("ref", serde_json::to_string(&reference.branch).unwrap()),
            ("sha", serde_json::to_string(&reference.sha).unwrap()),
            ("user", serde_json::to_string(&reference.user).unwrap()),
            ("repo", serde_json::to_string(&reference.repo).unwrap()),
        ] {
            let member = format!(r#""{name}":{value}"#);
            assert_eq!(wire.matches(&member).count(), 1, "{name}");
            let missing =
                wire.replacen(&format!("{member},"), "", 1)
                    .replacen(&format!(",{member}"), "", 1);
            assert_ne!(wire, missing);
            assert!(serde_json::from_str::<PullRefRecord<Option<WorkflowOwner>, Option<PullRepository>>>(&missing).is_err(), "{name}");
            assert!(amiss_wire::read_json::<PullRefRecord<Option<WorkflowOwner>, Option<PullRepository>>>(missing.as_bytes(), u64::MAX).is_err(), "{name}");
        }
    }
}

#[test]
fn webhook_ref_records_reject_unknown_and_invalid_nested_members() {
    let references: Vec<PullRefRecord<Option<WorkflowOwner>, Option<PullRepository>>> =
        serde_json::from_str(REFERENCES).unwrap();
    let reference = references.first().unwrap();
    let wire = serde_json::to_string(reference).unwrap();
    for (old, new) in [
        ("{", r#"{"unknown":true,"#),
        (r#""user":{"#, r#""user":{"unknown":true,"#),
        (r#""repo":{"#, r#""repo":{"unknown":true,"#),
        (r#""user":{"#, r#""\u0075ser":null,"user":{"#),
        (r#""repo":{"#, r#""\u0072epo":null,"repo":{"#),
        (r#""label":"Codertocat:changes""#, r#""label":null"#),
        (r#""ref":"changes""#, r#""ref":false"#),
        (
            r#""ref":"changes""#,
            r#""\u0072ef":"other","ref":"changes""#,
        ),
        (
            r#""sha":"ec26c3e57ca3a959ca5aad62de7213c562f8c821""#,
            r#""sha":"not-an-oid""#,
        ),
        (r#""id":21031067"#, r#""id":9007199254740992"#),
        (r#""id":186853002"#, r#""id":9007199254740992"#),
    ] {
        assert!(wire.contains(old), "{old}");
        let invalid = wire.replacen(old, new, 1);
        assert!(
            serde_json::from_str::<PullRefRecord<Option<WorkflowOwner>, Option<PullRepository>>>(
                &invalid
            )
            .is_err(),
            "{new}"
        );
        assert!(
            amiss_wire::read_json::<PullRefRecord<Option<WorkflowOwner>, Option<PullRepository>>>(
                invalid.as_bytes(),
                u64::MAX
            )
            .is_err(),
            "{new}"
        );
    }
    let user = serde_json::to_string(&reference.user).unwrap();
    let repository = serde_json::to_string(&reference.repo).unwrap();
    for (member, replacement) in [
        (&user, "false"),
        (&user, "{}"),
        (&repository, "false"),
        (&repository, "{}"),
    ] {
        assert!(wire.contains(member));
        let invalid = wire.replacen(member, replacement, 1);
        assert!(
            serde_json::from_str::<PullRefRecord<Option<WorkflowOwner>, Option<PullRepository>>>(
                &invalid
            )
            .is_err()
        );
    }
}

#[test]
fn complete_ref_inputs_reject_positional_and_trailing_documents() {
    let references: Vec<PullRefRecord<Option<WorkflowOwner>, Option<PullRepository>>> =
        serde_json::from_str(REFERENCES).unwrap();
    let reference = references.first().unwrap();
    let wire = serde_json::to_string(reference).unwrap();
    let positional = serde_json::to_string(&(
        &reference.label,
        &reference.branch,
        &reference.sha,
        &reference.user,
        &reference.repo,
    ))
    .unwrap();
    for invalid in [positional, format!("{wire} null"), REFERENCES.to_owned()] {
        assert!(
            amiss_wire::read_json::<PullRefRecord<Option<WorkflowOwner>, Option<PullRepository>>>(
                invalid.as_bytes(),
                u64::MAX
            )
            .is_err()
        );
    }
}
