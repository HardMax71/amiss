#![expect(
    clippy::unwrap_used,
    reason = "fixed configuration fixtures must fail loudly"
)]

use std::process::Command;

use amiss_controller_fixtures::config::{TrustFiles, artifact_service, paths, plan};
use amiss_controller_gitea_service::ServiceConfig;
use serde_json::{Value, json};

const BINARY: &str = env!("CARGO_BIN_EXE_amiss-controller-gitea");

struct Fixture {
    _trust: TrustFiles,
    config: std::path::PathBuf,
    bootstrap: std::path::PathBuf,
    ledger: std::path::PathBuf,
    token: std::path::PathBuf,
    value: Value,
}

impl Fixture {
    fn new(namespace: &str) -> Self {
        let trust = TrustFiles::new("forge.example", "hardmax71", "amiss").unwrap();
        let scratch = trust.directory("scratch").unwrap();
        let inbox = trust.directory("inbox").unwrap();
        let ledger = trust.directory("ledger").unwrap();
        let artifacts = trust.directory("artifacts").unwrap();
        let artifact_token = trust
            .write("artifact.token", b"gitea-artifact-bearer-token-fixture")
            .unwrap();
        let token = trust
            .write("reviewer.token", b"dedicated-reviewer-token-2026")
            .unwrap();
        let webhook_secret = trust
            .write("webhook.secret", b"gitea-family-webhook-fixture-secret")
            .unwrap();
        let config = trust.path("service.json");
        let value = json!({
            "listen": "127.0.0.1:0",
            "webhook_path": "/webhooks/forge",
            "provider": {
                "namespace": namespace,
                "instance": "forge.example",
                "api_base": "https://forge.example/api/v1",
                "reviewer": {
                    "id": 77,
                    "login": "amiss-controller",
                    "token_file": token
                },
                "webhook_keys": [{
                    "id": "current",
                    "secret_file": webhook_secret,
                    "active_from_unix_millis": 0,
                    "active_until_unix_millis": null
                }]
            },
            "repository": {
                "id": 101,
                "owner": "acme",
                "name": "widget",
                "target_branch": "main"
            },
            "plan": plan(&trust.constraint),
            "paths": paths(&trust.bootstrap, &scratch, &ledger, &artifacts, Some(&inbox)),
            "artifacts": artifact_service("amiss.example", &artifact_token)
        });
        Self {
            bootstrap: trust.bootstrap.clone(),
            _trust: trust,
            config,
            ledger,
            token,
            value,
        }
    }

    fn save(&self) {
        std::fs::write(
            &self.config,
            serde_json::to_vec_pretty(&self.value).unwrap(),
        )
        .unwrap();
    }

    fn field(&mut self, pointer: &str) -> &mut Value {
        self.value.pointer_mut(pointer).unwrap()
    }

    fn insert(&mut self, name: &str, value: Value) {
        self.value
            .as_object_mut()
            .unwrap()
            .insert(name.to_owned(), value);
    }
}

#[test]
fn gitea_and_forgejo_namespaces_load_the_same_closed_lane() {
    for namespace in ["gitea", "forgejo"] {
        let fixture = Fixture::new(namespace);
        fixture.save();
        ServiceConfig::load(&fixture.config).unwrap();
        std::fs::write(fixture.ledger.join("unexpected"), b"invalid state").unwrap();
        let output = Command::new(BINARY)
            .arg("--check")
            .arg(&fixture.config)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            "amiss-controller-gitea: configuration valid\n"
        );
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn provider_namespace_is_open_but_canonical() {
    let compatible = Fixture::new("compatible-fork");
    compatible.save();
    ServiceConfig::load(&compatible.config).unwrap();

    let mut invalid = Fixture::new("Forgejo");
    invalid.save();
    let error = ServiceConfig::load(&invalid.config).err().unwrap();
    assert_eq!(error.to_string(), "provider identity is invalid");

    *invalid.field("/provider/namespace") = json!("bad/name");
    invalid.save();
    let error = ServiceConfig::load(&invalid.config).err().unwrap();
    assert_eq!(error.to_string(), "provider identity is invalid");
}

#[test]
fn reviewer_token_and_api_are_validated_during_configuration() {
    let mut invalid_reviewer = Fixture::new("gitea");
    *invalid_reviewer.field("/provider/reviewer/id") = json!(0);
    invalid_reviewer.save();
    assert_eq!(
        ServiceConfig::load(&invalid_reviewer.config)
            .err()
            .unwrap()
            .to_string(),
        "dedicated reviewer identity is invalid"
    );

    let mut invalid_api = Fixture::new("forgejo");
    *invalid_api.field("/provider/api_base") = json!("https://elsewhere.example/api/v1");
    invalid_api.save();
    assert_eq!(
        ServiceConfig::load(&invalid_api.config)
            .err()
            .unwrap()
            .to_string(),
        "Gitea-family API configuration is invalid"
    );

    let invalid_token = Fixture::new("gitea");
    std::fs::write(&invalid_token.token, b"short\n").unwrap();
    invalid_token.save();
    assert_eq!(
        ServiceConfig::load(&invalid_token.config)
            .err()
            .unwrap()
            .to_string(),
        "provider token is invalid"
    );
}

#[test]
fn action_and_repository_must_stay_on_the_exact_lane() {
    let mut repository = Fixture::new("gitea");
    *repository.field("/repository/owner") = json!("Acme");
    repository.save();
    assert_eq!(
        ServiceConfig::load(&repository.config)
            .err()
            .unwrap()
            .to_string(),
        "Gitea-family repository spelling is not canonical"
    );

    let fixture = Fixture::new("forgejo");
    fixture.save();
    let constraint = fixture
        .value
        .pointer("/plan/execution_constraint_file")
        .unwrap()
        .as_str()
        .unwrap();
    let mut descriptor: Value =
        serde_json::from_slice(&std::fs::read(constraint).unwrap()).unwrap();
    *descriptor.pointer_mut("/action_repository/host").unwrap() = json!("elsewhere.example");
    std::fs::write(constraint, serde_json::to_vec_pretty(&descriptor).unwrap()).unwrap();
    assert_eq!(
        ServiceConfig::load(&fixture.config)
            .err()
            .unwrap()
            .to_string(),
        "action repository must use this SHA-1 provider instance"
    );

    let nested_action = Fixture::new("gitea");
    nested_action.save();
    let constraint = nested_action
        .value
        .pointer("/plan/execution_constraint_file")
        .unwrap()
        .as_str()
        .unwrap();
    let mut descriptor: Value =
        serde_json::from_slice(&std::fs::read(constraint).unwrap()).unwrap();
    *descriptor.pointer_mut("/action_repository/owner").unwrap() = json!("nested/group");
    std::fs::write(constraint, serde_json::to_vec_pretty(&descriptor).unwrap()).unwrap();
    assert_eq!(
        ServiceConfig::load(&nested_action.config)
            .err()
            .unwrap()
            .to_string(),
        "action repository must use this SHA-1 provider instance"
    );
}

#[test]
fn bootstrap_and_storage_roots_remain_bound() {
    let fixture = Fixture::new("gitea");
    fixture.save();
    std::fs::write(&fixture.bootstrap, b"changed after plan creation").unwrap();
    assert_eq!(
        ServiceConfig::load(&fixture.config)
            .err()
            .unwrap()
            .to_string(),
        "bootstrap does not match the execution constraint"
    );

    let mut overlap = Fixture::new("forgejo");
    let scratch = overlap.value.pointer("/paths/scratch").unwrap().clone();
    *overlap.field("/paths/inbox") = scratch;
    overlap.save();
    assert_eq!(
        ServiceConfig::load(&overlap.config)
            .err()
            .unwrap()
            .to_string(),
        "scratch, inbox, ledger, and artifact roots must be separate"
    );
}

#[test]
fn target_and_unknown_fields_fail_closed() {
    let mut target = Fixture::new("gitea");
    *target.field("/repository/target_branch") = json!("refs/heads/main");
    target.save();
    assert_eq!(
        ServiceConfig::load(&target.config)
            .err()
            .unwrap()
            .to_string(),
        "Gitea-family target branch is invalid"
    );

    let mut unknown = Fixture::new("forgejo");
    unknown.insert("unexpected", json!(true));
    unknown.save();
    assert_eq!(
        ServiceConfig::load(&unknown.config)
            .err()
            .unwrap()
            .to_string(),
        "configuration is not strict JSON"
    );
}

#[test]
fn nested_execution_and_queue_limits_are_effective() {
    for limits in [
        json!({ "execution": { "git_request_seconds": 121 } }),
        json!({ "queue": { "idle_poll_millis": 5_001 } }),
    ] {
        let mut fixture = Fixture::new("gitea");
        fixture.insert("limits", limits);
        fixture.save();
        assert!(ServiceConfig::load(&fixture.config).is_err());
    }

    for field in ["api_read_millis", "api_write_millis"] {
        let mut fixture = Fixture::new("forgejo");
        fixture.insert("limits", json!({ "execution": { (field): 4_000 } }));
        fixture.save();
        assert_eq!(
            ServiceConfig::load(&fixture.config)
                .err()
                .unwrap()
                .to_string(),
            "Gitea-family API timeouts are invalid"
        );
    }
}

#[test]
fn unknown_nested_limit_fields_are_rejected() {
    for limits in [
        json!({ "execution": { "unexpected": true } }),
        json!({ "queue": { "unexpected": true } }),
    ] {
        let mut fixture = Fixture::new("forgejo");
        fixture.insert("limits", limits);
        fixture.save();
        assert_eq!(
            ServiceConfig::load(&fixture.config)
                .err()
                .unwrap()
                .to_string(),
            "configuration is not strict JSON"
        );
    }
}

/// Each clause of the token and repository grammars refuses alone: a token of
/// lawful length with one non-graphic byte, and a repository id of zero.
#[test]
fn token_alphabet_and_repository_identity_refuse_alone() {
    let nongraphic = Fixture::new("gitea");
    std::fs::write(&nongraphic.token, b"valid-length-token\x07x").unwrap();
    nongraphic.save();
    assert_eq!(
        ServiceConfig::load(&nongraphic.config)
            .err()
            .unwrap()
            .to_string(),
        "provider token is invalid"
    );

    let mut zero_repository = Fixture::new("gitea");
    *zero_repository.field("/repository/id") = json!(0);
    zero_repository.save();
    assert_eq!(
        ServiceConfig::load(&zero_repository.config)
            .err()
            .unwrap()
            .to_string(),
        "Gitea-family numeric identity must be positive"
    );
}
