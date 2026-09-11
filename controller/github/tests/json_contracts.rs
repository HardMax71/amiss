use amiss_controller::{ProviderError, decode_bounded_json};
use amiss_controller_github::commit::GitCommitRecord;
use amiss_controller_github::reference::RefRecord;
use amiss_wire::model::ObjectFormat;

#[path = "json_contracts/artifact.rs"]
mod artifact;

#[path = "json_contracts/rules.rs"]
mod rules;

#[path = "json_contracts/check_app.rs"]
mod check_app;

#[path = "json_contracts/check_run.rs"]
mod check_run;

#[path = "json_contracts/full_repository.rs"]
mod full_repository;

#[path = "json_contracts/installation.rs"]
mod installation;

#[path = "json_contracts/owner.rs"]
mod owner;

#[path = "json_contracts/pull_repository.rs"]
mod pull_repository;

#[path = "json_contracts/pull_ref.rs"]
mod pull_ref;

#[path = "json_contracts/pull_request.rs"]
mod pull_request;

#[path = "json_contracts/repository.rs"]
mod repository;

#[path = "json_contracts/workflow.rs"]
mod workflow;

#[path = "json_contracts/webhook_refs.rs"]
mod webhook_refs;

#[path = "json_contracts/webhook_metadata.rs"]
mod webhook_metadata;

#[path = "json_contracts/webhook_changes.rs"]
mod webhook_changes;

#[path = "json_contracts/webhook_repository.rs"]
mod webhook_repository;

#[path = "json_contracts/webhook_root.rs"]
mod webhook_root;

#[path = "json_contracts/webhook_commit.rs"]
mod webhook_commit;

#[path = "json_contracts/webhook_run.rs"]
mod webhook_run;

#[path = "json_contracts/webhook_pull_repository.rs"]
mod webhook_pull_repository;

#[path = "json_contracts/webhook_pull_ref.rs"]
mod webhook_pull_ref;

#[path = "json_contracts/webhook_pull_metadata.rs"]
mod webhook_pull_metadata;

#[path = "json_contracts/webhook_pull.rs"]
mod webhook_pull;

#[path = "json_contracts/webhook_review.rs"]
mod webhook_review;

#[path = "json_contracts/webhook_comment.rs"]
mod webhook_comment;

#[path = "json_contracts/webhook_thread.rs"]
mod webhook_thread;

#[path = "json_contracts/webhook_suite.rs"]
mod webhook_suite;

#[path = "json_contracts/webhook_check_run.rs"]
mod webhook_check_run;

#[path = "json_contracts/workflow_event.rs"]
mod workflow_event;

#[path = "json_contracts/issue_comment_event.rs"]
mod issue_comment_event;

#[path = "json_contracts/issue_activity.rs"]
mod issue_activity;

#[test]
fn reference_inputs_keep_consumed_facts_and_ignore_provider_metadata() {
    let input = include_str!("fixtures/git-reference.json");
    let listing = include_str!("fixtures/git-references.json");
    let record: RefRecord = serde_json::from_str(input).unwrap();
    let records: Vec<RefRecord> = serde_json::from_str(listing).unwrap();
    assert_eq!(records.as_slice(), std::slice::from_ref(&record));
    assert_eq!(record.reference, "refs/heads/github/typed-commit-flow");
    assert_eq!(record.object.kind, amiss_wire::model::ObjectKind::Commit);
    assert_eq!(record.object.sha.object_format(), ObjectFormat::Sha1);
    assert_eq!(
        record.object.sha.as_str(),
        "9f0c1d21a356128a9b0337a446850e6f281ac62f"
    );
    let minimal = serde_json::to_string(&record).unwrap();
    for candidate in [
        minimal.clone(),
        input.replacen('{', r#"{"future":{"nested":[1.5,null,true]},"#, 1),
        input.replacen(r#""object":{"#, r#""object":{"future":"ignored","#, 1),
    ] {
        let (decoded, length): (RefRecord, _) =
            decode_bounded_json(candidate.as_bytes(), None, candidate.len(), |bytes| {
                serde_json::from_slice(bytes)
            })
            .unwrap();
        assert_eq!(decoded, record);
        assert_eq!(length, candidate.len());
    }
    for (old, new) in [
        (r#""type":"commit""#, r#""type":"future_kind""#),
        (r#""type":"commit""#, r#""type":{"commit":null}"#),
        (r#""type":"commit""#, r#""type":0"#),
        (r#""ref":"#, r#""ref":null,"ref":"#),
        (r#""sha":"#, r#""sha":null,"sha":"#),
        (r#""object":{"#, r#""object":null,"object":{"#),
        (
            r#""sha":"9f0c1d21a356128a9b0337a446850e6f281ac62f""#,
            r#""sha":"not-an-oid""#,
        ),
    ] {
        assert_eq!(input.matches(old).count(), 1, "{old}");
        let invalid = input.replacen(old, new, 1);
        assert!(
            serde_json::from_str::<RefRecord>(&invalid).is_err(),
            "{new}"
        );
    }
    for invalid in [
        "{}".to_owned(),
        format!(
            r#"{{"ref":"main","object":{{"sha":"{}"}}}}"#,
            record.object.sha
        ),
        format!(
            r#"{{"object":{}}}"#,
            serde_json::to_string(&record.object).unwrap()
        ),
        format!("{minimal} trailing"),
        format!("{minimal} {{}}"),
    ] {
        assert!(
            serde_json::from_str::<RefRecord>(&invalid).is_err(),
            "{invalid}"
        );
    }
    let positional = (&record.reference, (&record.object.kind, &record.object.sha));
    assert_eq!(
        serde_json::from_slice::<RefRecord>(&serde_json::to_vec(&positional).unwrap()).unwrap(),
        record
    );
    assert!(serde_json::from_str::<RefRecord>(listing).is_err());
    assert!(serde_json::from_str::<Vec<RefRecord>>(&minimal).is_err());
}

#[test]
fn git_commit_inputs_keep_only_the_consumed_commit_graph() {
    for (input, parents) in [
        (
            include_bytes!("fixtures/git-commit-unsigned.json").as_slice(),
            1,
        ),
        (
            include_bytes!("fixtures/git-commit-signed.json").as_slice(),
            2,
        ),
    ] {
        let (commit, length): (GitCommitRecord, _) =
            decode_bounded_json(input, None, input.len(), |bytes| {
                serde_json::from_slice(bytes)
            })
            .unwrap();
        assert_eq!(length, input.len());
        assert_eq!(commit.parents.len(), parents);
        assert_eq!(commit.sha.object_format(), ObjectFormat::Sha1);
        assert_eq!(commit.tree.sha.object_format(), ObjectFormat::Sha1);
        assert_ne!(commit.sha, commit.tree.sha);
        assert!(commit.parents.iter().all(|parent| parent.sha != commit.sha));
        let minimal = serde_json::to_string(&commit).unwrap();
        for candidate in [
            minimal.clone(),
            minimal.replacen(
                '{',
                r#"{"verification":{"reason":"future"},"message":null,"#,
                1,
            ),
            minimal.replacen(r#""tree":{"#, r#""tree":{"future":[false,1.5,null],"#, 1),
        ] {
            assert_eq!(
                serde_json::from_str::<GitCommitRecord>(&candidate).unwrap(),
                commit
            );
        }
        assert!(matches!(
            decode_bounded_json::<GitCommitRecord, _>(input, None, input.len() - 1, |bytes| {
                serde_json::from_slice(bytes)
            }),
            Err(ProviderError::InvalidResponse)
        ));
    }
}

#[test]
fn consumed_commit_fields_remain_required_and_typed() {
    let input = include_str!("fixtures/git-commit-unsigned.json");
    for (old, new) in [
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
        (r#""tree":{"#, r#""tree":null,"tree":{"#),
        (r#""parents":[{"#, r#""parents":[{"sha":null,"#),
        (r#""parents":[{"#, r#""parents":null,"parents":[{"#),
    ] {
        assert_eq!(input.matches(old).count(), 1, "{old}");
        let invalid = input.replacen(old, new, 1);
        assert!(
            serde_json::from_str::<GitCommitRecord>(&invalid).is_err(),
            "{new}"
        );
    }
    for invalid in [
        "{}".to_owned(),
        format!(
            r#"{{"sha":"{}","tree":{{"sha":"{}"}}}}"#,
            "a".repeat(40),
            "b".repeat(40)
        ),
        format!(r#"{{"sha":"{}","tree":{{}},"parents":[]}}"#, "a".repeat(40)),
    ] {
        assert!(
            serde_json::from_str::<GitCommitRecord>(&invalid).is_err(),
            "{invalid}"
        );
    }
}
