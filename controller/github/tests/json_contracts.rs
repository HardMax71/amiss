use amiss_controller::{ProviderError, decode_bounded_json};
use amiss_controller_github::commit::{GitCommitRecord, VerificationReason};
use amiss_controller_github::reference::RefRecord;
use amiss_wire::model::ObjectFormat;

#[path = "json_contracts/artifact.rs"]
mod artifact;

#[path = "json_contracts/check_app.rs"]
mod check_app;

#[path = "json_contracts/check_run.rs"]
mod check_run;

#[path = "json_contracts/owner.rs"]
mod owner;

#[path = "json_contracts/repository.rs"]
mod repository;

#[path = "json_contracts/workflow.rs"]
mod workflow;

#[test]
fn reference_captures_share_the_complete_object_without_losing_fields() {
    let input = include_str!("fixtures/git-reference.json");
    let listing = include_str!("fixtures/git-references.json");
    let record: RefRecord = amiss_wire::read_json(input.as_bytes(), u64::MAX).unwrap();
    let records: Vec<RefRecord> = amiss_wire::read_json(listing.as_bytes(), u64::MAX).unwrap();
    assert_eq!(records.as_slice(), std::slice::from_ref(&record));
    assert_eq!(record.reference, "refs/heads/github/typed-commit-flow");
    assert_eq!(record.object.kind, amiss_wire::model::ObjectKind::Commit);
    assert_eq!(record.object.sha.object_format(), ObjectFormat::Sha1);
    assert_eq!(
        record.object.sha.as_str(),
        "9f0c1d21a356128a9b0337a446850e6f281ac62f"
    );
    assert_eq!(
        amiss_fixtures::canonical_json(&serde_json::to_vec(&record).unwrap()).unwrap(),
        amiss_fixtures::canonical_json(input.as_bytes()).unwrap()
    );
    assert_eq!(
        amiss_fixtures::canonical_json(&serde_json::to_vec(&records).unwrap()).unwrap(),
        amiss_fixtures::canonical_json(listing.as_bytes()).unwrap()
    );
    for (old, new) in [
        (r#""ref":"#, r#""unexpected":true,"ref":"#),
        (r#""object":{"#, r#""object":{"unexpected":true,"#),
        (r#""type":"commit""#, r#""type":"future_kind""#),
        (r#""type":"commit""#, r#""type":{"commit":null}"#),
        (r#""type":"commit""#, r#""type":0"#),
        (r#""node_id":"#, r#""\u006eode_id":null,"node_id":"#),
        (
            r#""sha":"9f0c1d21a356128a9b0337a446850e6f281ac62f""#,
            r#""sha":"not-an-oid""#,
        ),
    ] {
        assert_eq!(input.matches(old).count(), 1, "{old}");
        let changed = input.replacen(old, new, 1);
        assert!(
            serde_json::from_str::<RefRecord>(&changed).is_err(),
            "{new}"
        );
        assert!(
            amiss_wire::read_json::<RefRecord>(changed.as_bytes(), u64::MAX).is_err(),
            "{new}"
        );
    }
    for field in [
        format!(
            "\"node_id\":{},",
            serde_json::to_string(&record.node_id).unwrap()
        ),
        format!("\"url\":{},", serde_json::to_string(&record.url).unwrap()),
        format!(
            ",\"url\":{}",
            serde_json::to_string(&record.object.url).unwrap()
        ),
    ] {
        assert_eq!(input.matches(&field).count(), 1, "{field}");
        let changed = input.replacen(&field, "", 1);
        assert!(
            serde_json::from_str::<RefRecord>(&changed).is_err(),
            "{field}"
        );
    }
    let positional = (
        &record.reference,
        &record.node_id,
        &record.url,
        &record.object,
    );
    let object = serde_json::to_string(&record.object).unwrap();
    let positional_object =
        serde_json::to_string(&(&record.object.kind, &record.object.sha, &record.object.url))
            .unwrap();
    let named = serde_json::to_string(&record).unwrap();
    assert_eq!(named.matches(&object).count(), 1);
    for encoded in [
        named.replacen(&object, &positional_object, 1).into_bytes(),
        serde_json::to_vec(&positional).unwrap(),
    ] {
        assert!(
            amiss_wire::read_json::<RefRecord>(&encoded, u64::MAX).is_err(),
            "{encoded:?}"
        );
    }
    let encoded = serde_json::to_vec(&[positional]).unwrap();
    assert!(amiss_wire::read_json::<Vec<RefRecord>>(&encoded, u64::MAX).is_err());
    assert!(amiss_wire::read_json::<RefRecord>(listing.as_bytes(), u64::MAX).is_err());
    assert!(amiss_wire::read_json::<Vec<RefRecord>>(input.as_bytes(), u64::MAX).is_err());
}

#[test]
fn captured_git_commits_keep_unsigned_and_verified_metadata() {
    for (input, reason, parents) in [
        (
            include_bytes!("fixtures/git-commit-unsigned.json").as_slice(),
            VerificationReason::Unsigned,
            1,
        ),
        (
            include_bytes!("fixtures/git-commit-signed.json").as_slice(),
            VerificationReason::Valid,
            2,
        ),
    ] {
        let (commit, length): (GitCommitRecord, _) =
            decode_bounded_json(input, None, input.len(), |bytes| {
                amiss_wire::read_json(bytes, u64::MAX)
            })
            .unwrap();
        assert_eq!(length, input.len());
        assert_eq!(commit.parents.len(), parents);
        assert_eq!(commit.sha.object_format(), ObjectFormat::Sha1);
        assert_eq!(commit.tree.sha.object_format(), ObjectFormat::Sha1);
        assert_ne!(commit.sha, commit.tree.sha);
        assert!(commit.parents.iter().all(|parent| parent.sha != commit.sha));
        assert_eq!(commit.verification.reason, reason);
        let verified = reason == VerificationReason::Valid;
        assert_eq!(commit.verification.verified, verified);
        assert_eq!(commit.verification.signature.is_some(), verified);
        assert_eq!(commit.verification.payload.is_some(), verified);
        assert_eq!(commit.verification.verified_at.is_some(), verified);
        let encoded = serde_json::to_vec(&commit).unwrap();
        assert_eq!(
            amiss_fixtures::canonical_json(&encoded).unwrap(),
            amiss_fixtures::canonical_json(input).unwrap()
        );
        assert_eq!(
            amiss_wire::read_json::<GitCommitRecord>(&encoded, u64::MAX).unwrap(),
            commit
        );
        assert!(matches!(
            decode_bounded_json::<GitCommitRecord, _>(input, None, input.len() - 1, |bytes| {
                amiss_wire::read_json(bytes, u64::MAX)
            }),
            Err(ProviderError::InvalidResponse)
        ));
    }
}

#[test]
fn commit_records_refuse_missing_unknown_and_malformed_fields() {
    let input = include_str!("fixtures/git-commit-unsigned.json");
    for (original, replacement) in [
        (r#""node_id":"#, r#""unexpected":true,"node_id":"#),
        (r#""author":{"#, r#""author":{"unexpected":true,"#),
        (r#""committer":{"#, r#""committer":{"unexpected":true,"#),
        (r#""tree":{"#, r#""tree":{"unexpected":true,"#),
        (r#""parents":[{"#, r#""parents":[{"unexpected":true,"#),
        (
            r#""verification":{"#,
            r#""verification":{"unexpected":true,"#,
        ),
        (r#""verified":false"#, r#""verified":0"#),
        (r#""reason":"unsigned""#, r#""reason":"future_reason""#),
        (r#""signature":null,"#, ""),
        (r#""payload":null,"#, ""),
        (r#","verified_at":null"#, ""),
        (r#""signature":null"#, r#""signature":false"#),
        (r#""payload":null"#, r#""payload":{}"#),
        (r#""verified_at":null"#, r#""verified_at":[]"#),
        (
            r#""sha":"9cefdc3c43f7f5c2b2da9f4bb84e1170842b668e""#,
            r#""sha":"not-an-oid""#,
        ),
        (
            r#""sha":"5bc80d6bddcf54450cf68482c87eb0567702c871""#,
            r#""sha":"5BC80D6BDDCF54450CF68482C87EB0567702C871""#,
        ),
        (
            r#""sha":"76be64f69a88f8b0fab32807ef221494d36a0b04""#,
            r#""sha":null"#,
        ),
        (
            r#""verified":false"#,
            r#""verified":false,"\u0076erified":false"#,
        ),
    ] {
        assert_eq!(input.matches(original).count(), 1, "{original}");
        let changed = input.replacen(original, replacement, 1);
        assert!(
            serde_json::from_str::<GitCommitRecord>(&changed).is_err(),
            "native model accepted {original} -> {replacement}"
        );
        assert!(
            amiss_wire::read_json::<GitCommitRecord>(changed.as_bytes(), u64::MAX).is_err(),
            "input boundary accepted {original} -> {replacement}"
        );
    }

    let commit: GitCommitRecord = serde_json::from_str(input).unwrap();
    for (original, replacement) in [
        (
            r#""reason":"unsigned""#.to_owned(),
            r#""reason":{"unsigned":null}"#.to_owned(),
        ),
        (
            format!(
                "\"author\":{}",
                serde_json::to_string(&commit.author).unwrap()
            ),
            format!(
                "\"author\":{}",
                serde_json::to_string(&(
                    &commit.author.name,
                    &commit.author.email,
                    &commit.author.date,
                ))
                .unwrap()
            ),
        ),
        (
            format!("\"tree\":{}", serde_json::to_string(&commit.tree).unwrap()),
            format!(
                "\"tree\":{}",
                serde_json::to_string(&(&commit.tree.sha, &commit.tree.url)).unwrap()
            ),
        ),
    ] {
        assert_eq!(input.matches(&original).count(), 1, "{original}");
        let changed = input.replacen(&original, &replacement, 1);
        assert!(
            amiss_wire::read_json::<GitCommitRecord>(changed.as_bytes(), u64::MAX).is_err(),
            "input boundary accepted {replacement}"
        );
    }
}
