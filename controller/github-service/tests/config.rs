#![expect(
    clippy::unwrap_used,
    reason = "fixed configuration fixtures must fail loudly"
)]

use amiss_bootstrap::BOOTSTRAP_DOMAIN;
use amiss_controller_github_service::ServiceConfig;
use amiss_wire::digest::hb;
use serde_json::{Value, json};
use tempfile::TempDir;

struct Fixture {
    _root: TempDir,
    config: std::path::PathBuf,
    bootstrap: std::path::PathBuf,
    value: Value,
}

impl Fixture {
    fn new() -> Self {
        let root = TempDir::new().unwrap();
        let scratch = directory(&root, "scratch");
        let inbox = directory(&root, "inbox");
        let ledger = directory(&root, "ledger");
        let bootstrap = root.path().join("amiss-bootstrap");
        let bootstrap_bytes = b"trusted bootstrap fixture";
        std::fs::write(&bootstrap, bootstrap_bytes).unwrap();
        let private_key = root.path().join("app.pem");
        std::fs::write(&private_key, vec![b'k'; 512]).unwrap();
        let webhook_secret = root.path().join("webhook.secret");
        std::fs::write(&webhook_secret, b"github-webhook-fixture-secret").unwrap();
        let constraint = root.path().join("execution.json");
        std::fs::write(
            &constraint,
            serde_json::to_vec_pretty(&json!({
                "schema": "amiss/scanner-execution-constraint",
                "action_repository": {
                    "host": "github.com",
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
            "webhook_path": "/webhooks/github",
            "github": {
                "instance": "github.com",
                "api_base": "https://api.github.com",
                "app_id": 71,
                "installation_id": 72,
                "private_key_file": private_key,
                "webhook_keys": [{
                    "id": "current",
                    "secret_file": webhook_secret,
                    "active_from_unix_millis": 0,
                    "active_until_unix_millis": null
                }]
            },
            "repository": {
                "id": 73,
                "owner": "hardmax71",
                "name": "amiss",
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
fn one_closed_configuration_loads_every_trust_input() {
    let fixture = Fixture::new();
    fixture.save();

    ServiceConfig::load(&fixture.config).unwrap();
}

#[test]
fn bootstrap_bytes_must_match_the_loaded_constraint() {
    let fixture = Fixture::new();
    fixture.save();
    std::fs::write(&fixture.bootstrap, b"changed after constraint creation").unwrap();

    let error = ServiceConfig::load(&fixture.config).err().unwrap();
    assert_eq!(
        error.to_string(),
        "bootstrap does not match the execution constraint"
    );
}

#[test]
fn action_repository_stays_on_the_github_lane() {
    for (field, value) in [
        ("/action_repository/host", json!("elsewhere.example")),
        ("/action_repository/owner", json!("nested/group")),
    ] {
        let fixture = Fixture::new();
        fixture.save();
        let constraint = fixture
            .value
            .pointer("/plan/execution_constraint_file")
            .unwrap()
            .as_str()
            .unwrap();
        let mut descriptor: Value =
            serde_json::from_slice(&std::fs::read(constraint).unwrap()).unwrap();
        *descriptor.pointer_mut(field).unwrap() = value;
        std::fs::write(constraint, serde_json::to_vec_pretty(&descriptor).unwrap()).unwrap();

        let error = ServiceConfig::load(&fixture.config).err().unwrap();
        assert_eq!(
            error.to_string(),
            "action repository must use this SHA-1 GitHub instance"
        );
    }

    let fixture = Fixture::new();
    fixture.save();
    let constraint = fixture
        .value
        .pointer("/plan/execution_constraint_file")
        .unwrap()
        .as_str()
        .unwrap();
    let mut descriptor: Value =
        serde_json::from_slice(&std::fs::read(constraint).unwrap()).unwrap();
    *descriptor.pointer_mut("/action_object_format").unwrap() = json!("sha256");
    *descriptor.pointer_mut("/action_commit_oid").unwrap() = json!("a".repeat(64));
    *descriptor.pointer_mut("/action_tree_oid").unwrap() = json!("b".repeat(64));
    std::fs::write(constraint, serde_json::to_vec_pretty(&descriptor).unwrap()).unwrap();
    let error = ServiceConfig::load(&fixture.config).err().unwrap();
    assert_eq!(
        error.to_string(),
        "action repository must use this SHA-1 GitHub instance"
    );
}

#[test]
fn writable_roots_must_not_overlap() {
    let mut fixture = Fixture::new();
    let scratch = fixture.value.pointer("/paths/scratch").unwrap().clone();
    *fixture.field("/paths/inbox") = scratch;
    fixture.save();

    let error = ServiceConfig::load(&fixture.config).err().unwrap();
    assert_eq!(
        error.to_string(),
        "scratch, inbox, and ledger roots must be separate"
    );
}

#[test]
fn execution_and_storage_limits_fail_during_configuration() {
    for (section, field, value) in [
        ("execution", "git_request_seconds", json!(121)),
        ("queue", "idle_poll_millis", json!(5_001)),
        ("queue", "inbox_record_bytes", json!(1_024)),
    ] {
        let mut fixture = Fixture::new();
        fixture.insert("limits", json!({ (section): { (field): value } }));
        fixture.save();
        assert!(ServiceConfig::load(&fixture.config).is_err(), "{field}");
    }
}

#[test]
fn unknown_configuration_fields_are_rejected() {
    for (field, value) in [
        ("unexpected", json!(true)),
        ("limits", json!({ "unexpected": true })),
    ] {
        let mut fixture = Fixture::new();
        fixture.insert(field, value);
        fixture.save();

        let error = ServiceConfig::load(&fixture.config).err().unwrap();
        assert_eq!(error.to_string(), "configuration is not strict JSON");
    }
}

#[test]
fn target_branch_is_one_full_git_branch_name() {
    let mut fixture = Fixture::new();
    *fixture.field("/repository/target_branch") = json!("refs/heads/main");
    fixture.save();

    let error = ServiceConfig::load(&fixture.config).err().unwrap();
    assert_eq!(error.to_string(), "GitHub target branch is invalid");
}

fn directory(root: &TempDir, name: &str) -> std::path::PathBuf {
    let path = root.path().join(name);
    std::fs::create_dir(&path).unwrap();
    path
}
