#![expect(
    clippy::unwrap_used,
    reason = "fixed configuration fixtures must fail loudly"
)]

use amiss_bootstrap::BOOTSTRAP_DOMAIN;
use amiss_controller_gitlab_service::ServiceConfig;
use amiss_wire::digest::hb;
use serde_json::{Value, json};
use tempfile::TempDir;

struct Fixture {
    _root: TempDir,
    config: std::path::PathBuf,
    api_token: std::path::PathBuf,
    constraint: std::path::PathBuf,
    value: Value,
}

impl Fixture {
    fn new() -> Self {
        let root = TempDir::new().unwrap();
        let scratch = directory(&root, "scratch");
        let ledger = directory(&root, "ledger");
        let bootstrap = root.path().join("amiss-bootstrap");
        let bootstrap_bytes = b"trusted bootstrap fixture";
        std::fs::write(&bootstrap, bootstrap_bytes).unwrap();
        let api_token = root.path().join("api.token");
        std::fs::write(&api_token, b"gitlab-api-token-fixture-2026").unwrap();
        let git_token = root.path().join("git.token");
        std::fs::write(&git_token, b"gitlab-git-token-fixture-2026").unwrap();
        let public_key = root.path().join("oidc-public.pem");
        std::fs::write(
            &public_key,
            include_bytes!("../../gitlab/tests/fixtures/public.pem"),
        )
        .unwrap();
        let constraint = root.path().join("execution.json");
        std::fs::write(
            &constraint,
            serde_json::to_vec_pretty(&json!({
                "schema": "amiss/scanner-execution-constraint",
                "action_repository": {
                    "host": "gitlab.example",
                    "owner": "security",
                    "name": "amiss-action"
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
            "evaluation_path": "/gitlab/policy/evaluate",
            "max_concurrent_evaluations": 4,
            "gitlab": {
                "instance": "gitlab.example",
                "api_base": "https://gitlab.example/api/v4",
                "api_token_file": api_token,
                "git": {
                    "username": "oauth2",
                    "token_file": git_token
                },
                "oidc": {
                    "issuer": "https://gitlab.example",
                    "audience": "amiss-controller",
                    "trust_set": "gitlab-oidc",
                    "keys": [{
                        "kid": "current",
                        "anchor": "gitlab-key/current",
                        "public_key_file": public_key
                    }]
                }
            },
            "policy": {
                "integration": "pipeline-execution-policy/1",
                "project_id": 101,
                "project_path": "acme/widget",
                "target_branch": "main",
                "job_name": "amiss:policy",
                "config_url": "https://gitlab.example/security/policy.yml",
                "config_commit": "ffffffffffffffffffffffffffffffffffffffff",
                "gitlab_hosted_runners": true,
                "self_hosted_runner_ids": [77]
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
                "ledger": ledger
            }
        });
        Self {
            _root: root,
            config,
            api_token,
            constraint,
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
}

#[test]
fn closed_gitlab_policy_lane_loads() {
    let fixture = Fixture::new();
    fixture.save();
    ServiceConfig::load(&fixture.config).unwrap();
}

#[test]
fn api_oidc_and_git_credentials_are_independent_and_strict() {
    let mut wrong_api = Fixture::new();
    *wrong_api.field("/gitlab/api_base") = json!("https://elsewhere.example/api/v4");
    wrong_api.save();
    assert_eq!(
        ServiceConfig::load(&wrong_api.config)
            .err()
            .unwrap()
            .to_string(),
        "GitLab API configuration is invalid"
    );

    let mut wrong_issuer = Fixture::new();
    *wrong_issuer.field("/gitlab/oidc/issuer") = json!("https://elsewhere.example");
    wrong_issuer.save();
    assert_eq!(
        ServiceConfig::load(&wrong_issuer.config)
            .err()
            .unwrap()
            .to_string(),
        "GitLab OIDC configuration is invalid"
    );

    let bad_token = Fixture::new();
    std::fs::write(&bad_token.api_token, b"short\n").unwrap();
    bad_token.save();
    assert_eq!(
        ServiceConfig::load(&bad_token.config)
            .err()
            .unwrap()
            .to_string(),
        "GitLab token is invalid"
    );
}

#[test]
fn policy_runner_and_action_bindings_fail_closed() {
    let mut duplicate_runner = Fixture::new();
    *duplicate_runner.field("/policy/self_hosted_runner_ids") = json!([77, 77]);
    duplicate_runner.save();
    assert_eq!(
        ServiceConfig::load(&duplicate_runner.config)
            .err()
            .unwrap()
            .to_string(),
        "GitLab runner trust is invalid"
    );

    let wrong_action = Fixture::new();
    wrong_action.save();
    let mut descriptor: Value =
        serde_json::from_slice(&std::fs::read(&wrong_action.constraint).unwrap()).unwrap();
    *descriptor.pointer_mut("/action_repository/host").unwrap() = json!("elsewhere.example");
    std::fs::write(
        &wrong_action.constraint,
        serde_json::to_vec_pretty(&descriptor).unwrap(),
    )
    .unwrap();
    assert_eq!(
        ServiceConfig::load(&wrong_action.config)
            .err()
            .unwrap()
            .to_string(),
        "action repository must use this SHA-1 GitLab instance"
    );
}

#[test]
fn synchronous_capacity_and_configuration_shape_are_closed() {
    for value in [0, 65] {
        let mut invalid = Fixture::new();
        *invalid.field("/max_concurrent_evaluations") = json!(value);
        invalid.save();
        assert!(ServiceConfig::load(&invalid.config).is_err());
    }

    let mut unknown = Fixture::new();
    unknown
        .value
        .as_object_mut()
        .unwrap()
        .insert("queue".to_owned(), json!({}));
    unknown.save();
    assert_eq!(
        ServiceConfig::load(&unknown.config)
            .err()
            .unwrap()
            .to_string(),
        "configuration is not strict JSON"
    );
}

fn directory(root: &TempDir, name: &str) -> std::path::PathBuf {
    let path = root.path().join(name);
    std::fs::create_dir(&path).unwrap();
    path
}
