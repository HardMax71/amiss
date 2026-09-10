use amiss_controller_github::webhook::Committer;
use amiss_controller_github::workflow::WorkflowCommit;
use amiss_wire::assessment::Nullable;

const COMMIT: &str = include_str!("../fixtures/webhook-workflow-commit.json");

#[test]
fn webhook_commit_retains_its_declared_author_metadata() {
    let input = COMMIT.replacen(
        "\"email\": \"matfax@users.noreply.github.com\"",
        "\"email\": null, \"date\": \"2020-10-05T16:32:07Z\", \"username\": \"fixture-author\"",
        1,
    );
    assert_ne!(input, COMMIT);
    for bytes in [COMMIT.as_bytes(), input.as_bytes()] {
        let commit: WorkflowCommit<Committer> = serde_json::from_slice(bytes).unwrap();
        assert_ne!(commit.id, commit.tree_id);
        assert_eq!(
            amiss_wire::read_json::<WorkflowCommit<Committer>>(bytes, u64::MAX).unwrap(),
            commit
        );
        assert_eq!(
            amiss_fixtures::canonical_json(bytes).unwrap(),
            amiss_fixtures::canonical_json(&serde_json::to_vec(&commit).unwrap()).unwrap()
        );
    }
    assert!(serde_json::from_str::<WorkflowCommit>(&input).is_err());

    let mut commit: WorkflowCommit<Committer> = serde_json::from_str(COMMIT).unwrap();
    for author in [&mut commit.author, &mut commit.committer] {
        author.email = Nullable::Null;
        author.date = Some("2020-10-05T16:32:07Z".to_owned());
        author.username = Some("fixture-author".to_owned());
        let input = serde_json::to_string(author).unwrap();
        assert_eq!(serde_json::from_str::<Committer>(&input).unwrap(), *author);
        for member in [
            "\"email\":null,".to_owned(),
            format!("\"name\":{},", serde_json::to_string(&author.name).unwrap()),
        ] {
            assert_eq!(input.matches(&member).count(), 1);
            let changed = input.replacen(&member, "", 1);
            assert!(serde_json::from_str::<Committer>(&changed).is_err());
        }
        for field in ["date", "username"] {
            let null = format!("{{\"name\":\"Author\",\"email\":null,\"{field}\":null}}");
            assert!(serde_json::from_str::<Committer>(&null).is_err(), "{field}");
        }
    }
}

#[test]
fn workflow_commit_specializations_require_every_shared_field() {
    let commit: WorkflowCommit<Committer> = serde_json::from_str(COMMIT).unwrap();
    let input = serde_json::to_string(&commit).unwrap();
    assert!(serde_json::from_str::<WorkflowCommit>(&input).is_ok());
    for member in [
        format!("\"id\":{},", serde_json::to_string(&commit.id).unwrap()),
        format!(
            "\"tree_id\":{},",
            serde_json::to_string(&commit.tree_id).unwrap()
        ),
        format!(
            "\"message\":{},",
            serde_json::to_string(&commit.message).unwrap()
        ),
        format!(
            "\"timestamp\":{},",
            serde_json::to_string(&commit.timestamp).unwrap()
        ),
        format!(
            "\"author\":{},",
            serde_json::to_string(&commit.author).unwrap()
        ),
        format!(
            ",\"committer\":{}",
            serde_json::to_string(&commit.committer).unwrap()
        ),
    ] {
        assert_eq!(input.matches(&member).count(), 1, "{member}");
        let missing = input.replacen(&member, "", 1);
        assert!(
            serde_json::from_str::<WorkflowCommit>(&missing).is_err(),
            "{member}"
        );
        assert!(
            serde_json::from_str::<WorkflowCommit<Committer>>(&missing).is_err(),
            "{member}"
        );
    }

    for (old, new) in [
        ("{", "{\"unknown\":true,"),
        ("\"author\":{", "\"author\":{\"unknown\":true,"),
        ("\"committer\":{", "\"committer\":{\"unknown\":true,"),
        ("\"id\":", "\"\\u0069d\":null,\"id\":"),
        ("\"email\":", "\"\\u0065mail\":null,\"email\":"),
        (
            "\"id\":\"3484a3fb816e0859fd6e1cea078d76385ff50625\"",
            "\"id\":\"bad\"",
        ),
        (
            "\"tree_id\":\"c30f6c3377009c4c91c85d1f15deb6f2b7c2eb37\"",
            "\"tree_id\":null",
        ),
    ] {
        assert!(input.contains(old), "{old}");
        let changed = input.replacen(old, new, 1);
        assert!(
            serde_json::from_str::<WorkflowCommit>(&changed).is_err(),
            "{new}"
        );
        assert!(
            serde_json::from_str::<WorkflowCommit<Committer>>(&changed).is_err(),
            "{new}"
        );
    }
    let authors = [
        serde_json::to_string(&commit.author).unwrap(),
        serde_json::to_string(&commit.committer).unwrap(),
    ];
    for author in authors {
        assert_eq!(input.matches(&author).count(), 1);
        for invalid in [
            "null",
            "false",
            "{}",
            "{\"name\":\"Author\",\"email\":false}",
        ] {
            let changed = input.replacen(&author, invalid, 1);
            assert!(
                serde_json::from_str::<WorkflowCommit<Committer>>(&changed).is_err(),
                "{invalid}"
            );
        }
    }
}

#[test]
fn strict_commit_boundary_refuses_positional_and_trailing_inputs() {
    let commit: WorkflowCommit<Committer> = serde_json::from_str(COMMIT).unwrap();
    let input = serde_json::to_string(&commit).unwrap();
    let author = serde_json::to_string(&commit.author).unwrap();
    assert_eq!(input.matches(&author).count(), 1);
    let positional_author = serde_json::to_string(&(
        &commit.author.name,
        &commit.author.email,
        &commit.author.date,
        &commit.author.username,
    ))
    .unwrap();
    let positional_commit = serde_json::to_string(&(
        &commit.id,
        &commit.tree_id,
        &commit.message,
        &commit.timestamp,
        &commit.author,
        &commit.committer,
    ))
    .unwrap();
    for invalid in [
        input.replacen(&author, &positional_author, 1),
        positional_commit,
        format!("{input} {{}}"),
        "[]".to_owned(),
    ] {
        assert!(
            amiss_wire::read_json::<WorkflowCommit<Committer>>(invalid.as_bytes(), u64::MAX)
                .is_err()
        );
    }
    assert!(amiss_wire::read_json::<WorkflowCommit<Committer>>(COMMIT.as_bytes(), 0).is_err());
}
