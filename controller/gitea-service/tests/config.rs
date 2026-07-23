#![expect(
    clippy::unwrap_used,
    reason = "fixed configuration fixtures must fail loudly"
)]

use amiss_bootstrap::BOOTSTRAP_DOMAIN;
use amiss_controller_gitea_service::ServiceConfig;
use amiss_wire::digest::hb;
use serde_json::{Value, json};
use tempfile::TempDir;

struct Fixture {
    _root: TempDir,
    config: std::path::PathBuf,
    bootstrap: std::path::PathBuf,
    token: std::path::PathBuf,
    value: Value,
}

impl Fixture {
    fn new(namespace: &str) -> Self {
        let root = TempDir::new().unwrap();
        let scratch = directory(&root, "scratch");
        let inbox = directory(&root, "inbox");
        let ledger = directory(&root, "ledger");
        let bootstrap = root.path().join("amiss-bootstrap");
        let bootstrap_bytes = b"trusted bootstrap fixture";
        std::fs::write(&bootstrap, bootstrap_bytes).unwrap();
        let token = root.path().join("reviewer.token");
        std::fs::write(&token, b"dedicated-reviewer-token-2026").unwrap();
        let webhook_secret = root.path().join("webhook.secret");
        std::fs::write(&webhook_secret, b"gitea-family-webhook-fixture-secret").unwrap();
        let constraint = root.path().join("execution.json");
        std::fs::write(
            &constraint,
            serde_json::to_vec_pretty(&json!({
                "schema": "amiss/scanner-execution-constraint",
                "action_repository": {
                    "host": "forge.example",
                    "owner": "hardmax71",
                    "name": "amiss"
                },
                "action_object_format": "sha1",
                "action_commit_oid": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "action_tree_oid": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "manifest_path": "release/manifest.json",
                "release_manifest_digest": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
                "selected_platform": "linux-x86_64",
                "required_status_name": "amiss / documentation assurance",
                "bootstrap_contract": "amiss-action-bootstrap",
                "bootstrap_digest": hb(BOOTSTRAP_DOMAIN, bootstrap_bytes).to_string()
            }))
            .unwrap(),
        )
        .unwrap();
        let config = root.path().join("service.json");
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
            "plan": {
                "profile": "enforce",
                "execution_constraint_file": constraint,
                "organization_floor_file": null,
                "debt_snapshot_file": null,
                "waiver_bundle_file": null
            },
            "paths": {
                "bootstrap": bootstrap,
                "scratch": scratch,
                "inbox": inbox,
                "ledger": ledger
            }
        });
        Self {
            _root: root,
            config,
            bootstrap,
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
        "scratch, inbox, and ledger roots must be separate"
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

fn directory(root: &TempDir, name: &str) -> std::path::PathBuf {
    let path = root.path().join(name);
    std::fs::create_dir(&path).unwrap();
    path
}
