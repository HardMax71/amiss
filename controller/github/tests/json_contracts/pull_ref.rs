use amiss_controller::decode_bounded_json;
use amiss_controller_github::pull::PullRefRecord;

const CAPTURE: &str = include_str!("../fixtures/pull-refs.json");

#[test]
fn pull_ref_captures_preserve_complete_open_and_deleted_heads() {
    let (refs, consumed): (Vec<PullRefRecord>, _) = decode_bounded_json(
        CAPTURE.as_bytes(),
        Some(u64::try_from(CAPTURE.len()).unwrap()),
        CAPTURE.len(),
        |bytes| serde_json::from_slice(bytes),
    )
    .unwrap();
    assert_eq!(consumed, CAPTURE.len());
    assert_eq!(refs.len(), 4);
    assert_eq!(
        amiss_fixtures::canonical_json(&serde_json::to_vec(&refs).unwrap()).unwrap(),
        amiss_fixtures::canonical_json(CAPTURE.as_bytes()).unwrap()
    );
    assert!(
        amiss_wire::read_json::<Vec<PullRefRecord>>(CAPTURE.as_bytes(), u64::MAX).unwrap() == refs
    );
    let head = refs.first().unwrap();
    assert_eq!(head.branch, "github/typed-pull-repositories");
    assert_eq!(head.label, "HardMax71:github/typed-pull-repositories");
    assert_eq!(head.user.login, "HardMax71");
    assert_eq!(head.repo.as_ref().unwrap().full_name, "HardMax71/amiss");
    let deleted = refs
        .iter()
        .find(|reference| reference.repo.is_none())
        .unwrap();
    assert_eq!(
        deleted.sha.as_str(),
        "c1b91fa95dc826be3e1df515b854074b540d8030"
    );
    assert!(amiss_wire::read_json::<Vec<PullRefRecord>>(CAPTURE.as_bytes(), 0).is_err());
    let trailing = format!("{CAPTURE} {{}}");
    assert!(amiss_wire::read_json::<Vec<PullRefRecord>>(trailing.as_bytes(), u64::MAX).is_err());
}

#[test]
fn pull_ref_nullable_repository_is_still_a_required_member() {
    let refs: Vec<PullRefRecord> = amiss_wire::read_json(CAPTURE.as_bytes(), u64::MAX).unwrap();
    let deleted = refs
        .iter()
        .find(|reference| reference.repo.is_none())
        .unwrap();
    let encoded = serde_json::to_string(deleted).unwrap();
    assert!(serde_json::from_str::<PullRefRecord>(&encoded).unwrap() == *deleted);
    assert!(
        amiss_wire::read_json::<PullRefRecord>(encoded.as_bytes(), u64::MAX).unwrap() == *deleted
    );
    for (field, value) in [
        ("label", serde_json::to_string(&deleted.label).unwrap()),
        ("ref", serde_json::to_string(&deleted.branch).unwrap()),
        ("sha", serde_json::to_string(&deleted.sha).unwrap()),
        ("user", serde_json::to_string(&deleted.user).unwrap()),
    ] {
        let member = format!("\"{field}\":{value},");
        assert_eq!(encoded.matches(&member).count(), 1, "{field}");
        for replacement in [String::new(), format!("\"{field}\":null,")] {
            let changed = encoded.replacen(&member, &replacement, 1);
            assert!(
                serde_json::from_str::<PullRefRecord>(&changed).is_err(),
                "{field}"
            );
            assert!(
                amiss_wire::read_json::<PullRefRecord>(changed.as_bytes(), u64::MAX).is_err(),
                "{field}"
            );
        }
    }
    let member = ",\"repo\":null";
    assert_eq!(encoded.matches(member).count(), 1);
    let missing = encoded.replacen(member, "", 1);
    assert!(serde_json::from_str::<PullRefRecord>(&missing).is_err());
    assert!(amiss_wire::read_json::<PullRefRecord>(missing.as_bytes(), u64::MAX).is_err());
}

#[test]
fn pull_refs_reject_unknown_members_and_non_object_forms() {
    let refs: Vec<PullRefRecord> = amiss_wire::read_json(CAPTURE.as_bytes(), u64::MAX).unwrap();
    let reference = refs.first().unwrap();
    let encoded = serde_json::to_string(reference).unwrap();
    for (old, new) in [
        ("{", "{\"unexpected\":true,"),
        ("\"user\":{", "\"user\":{\"unexpected\":true,"),
        ("\"repo\":{", "\"repo\":{\"unexpected\":true,"),
        ("\"ref\":", "\"\\u0072ef\":\"other\",\"ref\":"),
    ] {
        assert!(encoded.contains(old));
        let changed = encoded.replacen(old, new, 1);
        assert!(
            serde_json::from_str::<PullRefRecord>(&changed).is_err(),
            "{new}"
        );
        assert!(
            amiss_wire::read_json::<PullRefRecord>(changed.as_bytes(), u64::MAX).is_err(),
            "{new}"
        );
    }
    let positional = serde_json::to_vec(&(
        &reference.label,
        &reference.branch,
        &reference.sha,
        &reference.user,
        &reference.repo,
    ))
    .unwrap();
    assert!(serde_json::from_slice::<PullRefRecord>(&positional).unwrap() == *reference);
    assert!(amiss_wire::read_json::<PullRefRecord>(&positional, u64::MAX).is_err());
}
