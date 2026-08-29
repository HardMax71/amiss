#![expect(
    clippy::unwrap_used,
    reason = "fixed configuration fixtures must fail loudly"
)]

use std::ffi::OsString;
use std::process::Command;
use std::sync::LazyLock;

use amiss_controller_fixtures::config::{TrustFiles, artifact_service, paths, plan};
use amiss_controller_fixtures::{RsaKeys, rsa_keys};
use amiss_controller_github_service::ServiceConfig;
use amiss_wire::action::host_platform;
use amiss_wire::controls::ConstraintPlatform;
use serde_json::{Value, json};
use tempfile::TempDir;

const BINARY: &str = env!("CARGO_BIN_EXE_amiss-controller-github");

#[expect(
    clippy::expect_used,
    reason = "the generated RSA fixture must remain valid"
)]
static RSA_KEYS: LazyLock<RsaKeys> =
    LazyLock::new(|| rsa_keys().expect("the RSA fixture is valid"));

struct Fixture {
    _trust: TrustFiles,
    config: std::path::PathBuf,
    bootstrap: std::path::PathBuf,
    constraint: std::path::PathBuf,
    ledger: std::path::PathBuf,
    private_key: std::path::PathBuf,
    value: Value,
}

impl Fixture {
    fn new() -> Self {
        let trust = TrustFiles::new("github.com", "hardmax71", "amiss").unwrap();
        let scratch = trust.directory("scratch").unwrap();
        let inbox = trust.directory("inbox").unwrap();
        let ledger = trust.directory("ledger").unwrap();
        let artifacts = trust.directory("artifacts").unwrap();
        let artifact_token = trust
            .write("artifact.token", b"github-artifact-bearer-token-fixture")
            .unwrap();
        let private_key = trust.write("app.pem", &RSA_KEYS.private_pem).unwrap();
        let webhook_secret = trust
            .write("webhook.secret", b"github-webhook-fixture-secret")
            .unwrap();
        let config = trust.path("service.json");
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
            "plan": plan(&trust.constraint),
            "paths": paths(&trust.bootstrap, &scratch, &ledger, &artifacts, Some(&inbox)),
            "artifacts": artifact_service("amiss.example", &artifact_token)
        });
        Self {
            bootstrap: trust.bootstrap.clone(),
            constraint: trust.constraint.clone(),
            _trust: trust,
            config,
            ledger,
            private_key,
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

fn workflow_artifact() -> Value {
    json!({
        "workflow_identity": "docs-evidence.yml",
        "event": "pull_request",
        "artifact_name": "amiss-semantic-evidence",
        "payload_file": "amiss/semantic-template.json",
        "archive_byte_limit": 33_554_432,
        "file_byte_limit": 16_777_216,
        "semantic": {
            "acquisition_identity": "github-docs-evidence",
            "producer_kind": "site-build",
            "producer_identity": "docs-site",
            "producer_version": "0.5.1",
            "context_digest": "sha256:1111111111111111111111111111111111111111111111111111111111111111"
        }
    })
}

#[test]
fn one_closed_configuration_loads_every_trust_input() {
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
        "amiss-controller-github: configuration valid\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn workflow_artifacts_are_github_plan_inputs() {
    let mut fixture = Fixture::new();
    fixture.field("/plan").as_object_mut().unwrap().insert(
        "workflow_artifacts".to_owned(),
        json!([workflow_artifact()]),
    );
    fixture.save();
    ServiceConfig::load(&fixture.config).unwrap();

    *fixture.field("/plan/workflow_artifacts/0/semantic/context_digest") = json!("not-a-digest");
    fixture.save();
    assert_eq!(
        ServiceConfig::load(&fixture.config)
            .err()
            .unwrap()
            .to_string(),
        "workflow artifact configuration is invalid"
    );
}

#[test]
fn execution_constraint_must_target_the_host() {
    let fixture = Fixture::new();
    fixture.save();
    let wrong_platform = [
        ConstraintPlatform::LinuxX8664,
        ConstraintPlatform::WindowsAarch64,
    ]
    .into_iter()
    .find(|candidate| Some(*candidate) != host_platform())
    .unwrap();
    let mut constraint: Value =
        serde_json::from_slice(&std::fs::read(&fixture.constraint).unwrap()).unwrap();
    *constraint.pointer_mut("/selected_platform").unwrap() = json!(wrong_platform.as_ref());
    std::fs::write(
        &fixture.constraint,
        serde_json::to_vec_pretty(&constraint).unwrap(),
    )
    .unwrap();

    assert_eq!(
        ServiceConfig::load(&fixture.config)
            .err()
            .unwrap()
            .to_string(),
        "execution constraint does not target this host"
    );
}

#[test]
fn app_credentials_and_transport_fail_during_configuration() {
    let invalid_key = Fixture::new();
    invalid_key.save();
    std::fs::write(&invalid_key.private_key, vec![b'k'; 512]).unwrap();
    let output = Command::new(BINARY)
        .arg("--check")
        .arg(&invalid_key.config)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "amiss-controller-github: GitHub App configuration is invalid\n"
    );

    let mut invalid_api = Fixture::new();
    *invalid_api.field("/github/api_base") = json!("https://attacker.example");
    invalid_api.save();
    assert_eq!(
        ServiceConfig::load(&invalid_api.config)
            .err()
            .unwrap()
            .to_string(),
        "GitHub App configuration is invalid"
    );
}

#[test]
fn service_command_grammar_is_closed() {
    let root = TempDir::new().unwrap();
    let absolute = root.path().join("config.json");
    for arguments in [
        Vec::new(),
        vec![OsString::from("--check")],
        vec![OsString::from("relative.json")],
        vec![OsString::from("--check"), OsString::from("relative.json")],
        vec![absolute.into_os_string(), OsString::from("extra")],
        vec![OsString::from("--version"), OsString::from("extra")],
    ] {
        let output = Command::new(BINARY).args(arguments).output().unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert_eq!(
            String::from_utf8(output.stderr).unwrap(),
            "amiss-controller-github: expected ABS_CONFIG, --check ABS_CONFIG, or --version\n"
        );
    }
}

/// One service covers the path: all three reach it through one `service_main`.
#[test]
fn the_service_reports_its_own_version() {
    let output = Command::new(BINARY).arg("--version").output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!("amiss-controller-github {}\n", env!("CARGO_PKG_VERSION"))
    );
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

/// The instance and the repository spelling are each several clauses, and a
/// configuration that trips two proves neither, so each is bent alone. The
/// message matters as much as the refusal: a later stage refuses these too,
/// and a run that answers with its message is a run that skipped this one.
#[test]
fn every_canonical_spelling_clause_stands_alone() {
    let instance = "GitHub instance is not canonical";
    let repository = "GitHub repository spelling is not canonical";
    let identity = "GitHub numeric identity must be positive";
    let cases = [
        ("/github/instance", json!("GitHub.com"), instance),
        ("/github/instance", json!("github.com/enterprise"), instance),
        ("/github/instance", json!(""), instance),
        ("/repository/owner", json!("HardMax71"), repository),
        ("/repository/name", json!("Amiss"), repository),
        ("/repository/owner", json!("nested/group"), repository),
        ("/repository/id", json!(0), identity),
    ];
    for (pointer, value, message) in cases {
        let mut fixture = Fixture::new();
        *fixture.field(pointer) = value.clone();
        fixture.save();
        let defect = ServiceConfig::load(&fixture.config)
            .err()
            .unwrap_or_else(|| panic!("{pointer} of {value} loaded"));
        assert_eq!(defect.to_string(), message, "{pointer} of {value}");
    }

    let fixture = Fixture::new();
    fixture.save();
    assert!(ServiceConfig::load(&fixture.config).is_ok());
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
        "scratch, inbox, ledger, and artifact roots must be separate"
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
