#![expect(
    clippy::unwrap_used,
    reason = "fixed configuration fixtures must fail loudly"
)]

use std::process::Command;
use std::sync::LazyLock;

use amiss_controller_fixtures::config::{TrustFiles, artifact_service, paths, plan};
use amiss_controller_fixtures::{RsaKeys, rsa_keys};
use amiss_controller_gitlab_service::ServiceConfig;
use serde_json::{Value, json};

const BINARY: &str = env!("CARGO_BIN_EXE_amiss-controller-gitlab");

#[expect(
    clippy::expect_used,
    reason = "the generated RSA fixture must remain valid"
)]
static RSA_KEYS: LazyLock<RsaKeys> =
    LazyLock::new(|| rsa_keys().expect("the RSA fixture is valid"));

struct Fixture {
    _trust: TrustFiles,
    config: std::path::PathBuf,
    api_token: std::path::PathBuf,
    constraint: std::path::PathBuf,
    ledger: std::path::PathBuf,
    value: Value,
}

impl Fixture {
    fn new() -> Self {
        let trust = TrustFiles::new("gitlab.example", "security", "amiss-action").unwrap();
        let scratch = trust.directory("scratch").unwrap();
        let ledger = trust.directory("ledger").unwrap();
        let artifacts = trust.directory("artifacts").unwrap();
        let artifact_token = trust
            .write("artifact.token", b"gitlab-artifact-bearer-token-fixture")
            .unwrap();
        let api_token = trust
            .write("api.token", b"gitlab-api-token-fixture-2026")
            .unwrap();
        let git_token = trust
            .write("git.token", b"gitlab-git-token-fixture-2026")
            .unwrap();
        let public_key = trust
            .write("oidc-public.pem", &RSA_KEYS.public_pem)
            .unwrap();
        let config = trust.path("service.json");
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
            "plan": plan(&trust.constraint),
            "paths": paths(&trust.bootstrap, &scratch, &ledger, &artifacts, None),
            "artifacts": artifact_service("amiss.example", &artifact_token)
        });
        Self {
            constraint: trust.constraint.clone(),
            _trust: trust,
            config,
            api_token,
            ledger,
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
    std::fs::write(fixture.ledger.join("unexpected"), b"invalid state").unwrap();
    let output = Command::new(BINARY)
        .arg("--check")
        .arg(&fixture.config)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "amiss-controller-gitlab: configuration valid\n"
    );
    assert!(output.stderr.is_empty());
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

    for (reason, token) in [
        (
            "too short to be a token, printable throughout",
            "glpat-abcdef".as_bytes(),
        ),
        (
            "long enough, with a byte no header may carry",
            b"glpat-0123456789 abcdefghij".as_slice(),
        ),
    ] {
        let short_or_blank = Fixture::new();
        std::fs::write(&short_or_blank.api_token, token).unwrap();
        short_or_blank.save();
        assert_eq!(
            ServiceConfig::load(&short_or_blank.config)
                .err()
                .unwrap()
                .to_string(),
            "GitLab token is invalid",
            "{reason}"
        );
    }
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

    for target in ["refs/heads/main", "bad..branch"] {
        let mut invalid_target = Fixture::new();
        *invalid_target.field("/policy/target_branch") = json!(target);
        invalid_target.save();
        assert_eq!(
            ServiceConfig::load(&invalid_target.config)
                .err()
                .unwrap()
                .to_string(),
            "GitLab policy target branch is invalid"
        );
    }

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

/// A self-hosted runner id of zero refuses on the positive clause alone.
#[test]
fn a_zero_runner_id_is_refused_alone() {
    let mut zero_runner = Fixture::new();
    *zero_runner.field("/policy/self_hosted_runner_ids") = serde_json::json!([0]);
    zero_runner.save();
    assert_eq!(
        ServiceConfig::load(&zero_runner.config)
            .err()
            .unwrap()
            .to_string(),
        "GitLab runner trust is invalid"
    );
}
