use std::fs;
use std::io::Write as _;
use std::time::Duration;

use amiss_controller::{ExternalPolicy, ProviderIdentity};
use amiss_controller_service::{
    ExecutionLimits, ExecutionPaths, ServiceLimits, ServicePaths, framed_route_id,
    load_execution_limits, load_limits, load_plan, read_regular,
};
use amiss_wire::controls::Profile;
use amiss_wire::model::RepositoryIdentity;
use cap_std::ambient_authority;
use cap_std::fs::Dir;
use flate2::Compression;
use flate2::write::ZlibEncoder;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tempfile::TempDir;

fn sphinx_inventory(body: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(body)?;
    let compressed = encoder.finish()?;
    let mut inventory = b"# Sphinx inventory version 2\n# Project: Test\n# Version: 1\n# The remainder of this file is compressed using zlib.\n".to_vec();
    inventory.extend_from_slice(&compressed);
    Ok(inventory)
}

fn load_intersphinx_plan(
    directory: &TempDir,
    constraint: &std::path::Path,
) -> Result<amiss_controller::CheckPlan, Box<dyn std::error::Error>> {
    let inventory_path = directory.path().join("objects.inv");
    fs::write(
        &inventory_path,
        sphinx_inventory(b"except-star std:label -1 reference/compound_stmts.html -\n")?,
    )?;
    let files: amiss_controller_service::CheckPlanFiles = serde_json::from_value(json!({
        "profile": "enforce",
        "execution_constraint_file": constraint,
        "intersphinx_inventories": [{
            "identity": "python",
            "base_url": "https://docs.python.org/3/",
            "file": inventory_path,
        }],
    }))?;
    load_plan(&files, None).map_err(Into::into)
}

#[test]
fn limit_shapes_accept_only_owned_fields() {
    assert_json_shape::<ServiceLimits>(
        [json!({
            "execution": {
                "api_request_millis": 10_000
            },
            "queue": {
                "inbox_records": 32,
                "retry_min_millis": 500,
                "idle_poll_millis": 100
            }
        })],
        [
            json!({ "unexpected": true }),
            json!({ "execution": { "unexpected": true } }),
            json!({ "queue": { "unexpected": true } }),
        ],
    );
    assert_json_shape::<ExecutionLimits>(
        [json!({ "api_request_millis": 10_000 })],
        [
            json!({ "inbox_records": 32 }),
            json!({ "unexpected": true }),
        ],
    );
}

#[test]
fn trust_files_are_read_through_one_regular_nofollow_handle()
-> Result<(), Box<dyn std::error::Error>> {
    let root = TempDir::new()?;
    let regular = root.path().join("regular");
    fs::write(&regular, b"trusted")?;
    assert_eq!(read_regular(&regular, 7)?, b"trusted");

    let target = root.path().join("target");
    fs::write(&target, b"replacement")?;
    let root_dir = Dir::open_ambient_dir(root.path(), ambient_authority())?;
    cap_fs_ext::DirExt::symlink_file(&root_dir, "target", "linked")?;
    assert!(read_regular(&root.path().join("linked"), 32).is_err());

    let directory = root.path().join("directory");
    fs::create_dir(&directory)?;
    assert!(read_regular(&directory, 32).is_err());
    Ok(())
}

#[test]
fn queued_limits_reject_duplicate_execution_and_queue_fields() {
    for raw in [
        r#"{"execution":{"api_request_millis":10000,"api_request_millis":10001}}"#,
        r#"{"queue":{"inbox_records":32,"inbox_records":33}}"#,
        r#"{"execution":{},"execution":{}}"#,
        r#"{"queue":{},"queue":{}}"#,
    ] {
        assert!(serde_json::from_str::<ServiceLimits>(raw).is_err());
    }
}

#[test]
fn route_fields_have_one_unambiguous_frame() {
    let left = framed_route_id("amiss/test-route", "test", &["a", "bc"]);
    let right = framed_route_id("amiss/test-route", "test", &["ab", "c"]);

    assert!(left.is_some());
    assert!(right.is_some());
    assert_ne!(left, right);
    assert!(framed_route_id("amiss/test-route", "Test", &["value"]).is_none());
}

#[test]
fn execution_clock_policy_is_returned_with_ingress()
-> Result<(), amiss_controller_service::ConfigError> {
    let loaded = load_execution_limits(
        &ExecutionLimits::default(),
        "/provider/evaluate".to_owned(),
        4,
    )?;

    assert_eq!(loaded.signed_age, Duration::from_mins(5));
    assert_eq!(loaded.future_skew, Duration::from_secs(5));
    assert_eq!(loaded.evaluation.max_concurrent_requests, 4);
    Ok(())
}

#[test]
fn synchronous_concurrency_must_be_positive() {
    for value in [0, 65] {
        assert!(
            load_execution_limits(
                &ExecutionLimits::default(),
                "/provider/evaluate".to_owned(),
                value
            )
            .is_err()
        );
    }
}

#[test]
fn endpoint_paths_fail_during_configuration() {
    for path in ["/", "/healthz", "relative", "/double//part"] {
        assert!(
            load_limits(&ServiceLimits::default(), path.to_owned()).is_err(),
            "{path}"
        );
        assert!(
            load_execution_limits(&ExecutionLimits::default(), path.to_owned(), 4).is_err(),
            "{path}"
        );
    }
}

#[test]
fn future_skew_has_a_small_hard_ceiling() -> Result<(), Box<dyn std::error::Error>> {
    for seconds in [0, 5, 300] {
        let raw: ExecutionLimits =
            serde_json::from_value(json!({ "future_skew_seconds": seconds }))?;
        let loaded = load_execution_limits(&raw, "/provider/evaluate".to_owned(), 4)?;
        assert_eq!(loaded.future_skew, Duration::from_secs(seconds));
    }
    let too_large: ExecutionLimits = serde_json::from_value(json!({ "future_skew_seconds": 301 }))?;
    assert!(load_execution_limits(&too_large, "/provider/evaluate".to_owned(), 4).is_err());
    Ok(())
}

#[test]
fn queued_concurrency_has_a_hard_ceiling() -> Result<(), Box<dyn std::error::Error>> {
    for value in [1, 16, 64] {
        let raw: ServiceLimits = serde_json::from_value(json!({
            "queue": { "max_concurrent_deliveries": value }
        }))?;
        let loaded = load_limits(&raw, "/provider/delivery".to_owned())?;
        assert_eq!(loaded.receiver.max_concurrent_requests, value);
    }
    for value in [0, 65] {
        let raw: ServiceLimits = serde_json::from_value(json!({
            "queue": { "max_concurrent_deliveries": value }
        }))?;
        assert!(load_limits(&raw, "/provider/delivery".to_owned()).is_err());
    }
    Ok(())
}

#[test]
fn configured_limits_have_hard_ceilings() {
    assert_hard_ceilings::<ExecutionLimits>(
        &[
            ("body_bytes", 8_u64 * 1_024 * 1_024),
            ("header_count", 128),
            ("header_bytes", 32 * 1_024),
            ("ledger_records", 100_000),
            ("artifact_retention_seconds", 365 * 24 * 60 * 60),
            ("artifact_records", 100_000),
            ("artifact_bytes", 64_u64 * 1_024 * 1_024 * 1_024),
            ("artifact_record_bytes", 1_024_u64 * 1_024 * 1_024),
        ],
        |field, value| json!({ (field): value }),
        |raw| load_execution_limits(raw, "/provider/evaluate".to_owned(), 4).is_ok(),
    );
    assert_hard_ceilings::<ServiceLimits>(
        &[
            ("inbox_records", 1_024_u64),
            ("inbox_bytes", 128 * 1_024 * 1_024),
            ("inbox_record_bytes", 16 * 1_024 * 1_024),
            ("retry_max_millis", 24 * 60 * 60 * 1_000),
        ],
        |field, value| json!({ "queue": { (field): value } }),
        |raw| load_limits(raw, "/provider/delivery".to_owned()).is_ok(),
    );
}

#[test]
fn one_artifact_must_fit_inside_the_total_budget() {
    let raw: ExecutionLimits = serde_json::from_value(json!({
        "artifact_bytes": 1_024,
        "artifact_record_bytes": 1_025
    }))
    .unwrap();
    assert!(load_execution_limits(&raw, "/provider/evaluate".to_owned(), 4).is_err());
}

#[test]
fn queued_paths_add_only_the_inbox_field() {
    let execution = json!({
        "bootstrap": "/controller/amiss-bootstrap",
        "scratch": "/controller/scratch",
        "ledger": "/controller/ledger",
        "artifacts": "/controller/artifacts"
    });
    assert!(serde_json::from_value::<ExecutionPaths>(execution.clone()).is_ok());
    assert!(
        serde_json::from_value::<ExecutionPaths>(json!({
            "bootstrap": "/controller/amiss-bootstrap",
            "scratch": "/controller/scratch",
            "ledger": "/controller/ledger",
            "artifacts": "/controller/artifacts",
            "inbox": "/controller/inbox"
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<ServicePaths>(json!({
            "bootstrap": "/controller/amiss-bootstrap",
            "scratch": "/controller/scratch",
            "ledger": "/controller/ledger",
            "artifacts": "/controller/artifacts",
            "inbox": "/controller/inbox"
        }))
        .is_ok()
    );
    assert!(
        serde_json::from_value::<ServicePaths>(json!({
            "bootstrap": "/controller/amiss-bootstrap",
            "scratch": "/controller/scratch",
            "ledger": "/controller/ledger",
            "artifacts": "/controller/artifacts",
            "inbox": "/controller/inbox",
            "unexpected": true
        }))
        .is_err()
    );
    assert!(
        serde_json::from_str::<ServicePaths>(
            r#"{
                "bootstrap":"/controller/one",
                "bootstrap":"/controller/two",
                "scratch":"/controller/scratch",
                "ledger":"/controller/ledger",
                "artifacts":"/controller/artifacts",
                "inbox":"/controller/inbox"
            }"#
        )
        .is_err()
    );
}

fn assert_json_shape<T: DeserializeOwned>(
    accepted: impl IntoIterator<Item = Value>,
    rejected: impl IntoIterator<Item = Value>,
) {
    assert!(
        accepted
            .into_iter()
            .all(|value| serde_json::from_value::<T>(value).is_ok())
    );
    assert!(
        rejected
            .into_iter()
            .all(|value| serde_json::from_value::<T>(value).is_err())
    );
}

fn assert_hard_ceilings<T: DeserializeOwned>(
    limits: &[(&str, u64)],
    raw: impl Fn(&str, u64) -> Value,
    accepted: impl Fn(&T) -> bool,
) {
    for &(field, maximum) in limits {
        let at_maximum = serde_json::from_value(raw(field, maximum));
        let above_maximum = serde_json::from_value(raw(field, maximum.saturating_add(1)));
        assert!(
            at_maximum.is_ok_and(|value| accepted(&value)),
            "{field} rejected its hard ceiling"
        );
        assert!(
            above_maximum.is_ok_and(|value| !accepted(&value)),
            "{field} accepted a value above its hard ceiling"
        );
    }
}

#[expect(clippy::expect_used, reason = "test fixture helper")]
fn execution_with(overrides: Value) -> ExecutionLimits {
    let mut base = json!({});
    if let (Value::Object(target), Value::Object(extra)) = (&mut base, overrides) {
        target.extend(extra);
    }
    serde_json::from_value(base).expect("the override shape is owned")
}

/// Every validation clause refuses alone: one field off at a time, each
/// against otherwise-default limits.
#[test]
fn each_limit_clause_refuses_on_its_own() {
    for (field, value) in [
        ("header_count", json!(129)),
        ("header_bytes", json!(32_769)),
        ("api_connect_millis", json!(0)),
        ("api_request_millis", json!(30_001)),
        ("api_read_millis", json!(20_001)),
        ("statement_validity_seconds", json!(0)),
    ] {
        let raw = execution_with(json!({ field: value }));
        assert!(
            load_execution_limits(&raw, "/provider/evaluate".to_owned(), 4).is_err(),
            "{field}"
        );
    }
    let equal = execution_with(json!({ "ledger_lease_seconds": 20, "api_request_millis": 20_000 }));
    assert!(
        load_execution_limits(&equal, "/provider/evaluate".to_owned(), 4).is_err(),
        "a request timeout equal to the ledger lease must refuse"
    );
}

/// The route grammar admits digits and hyphens in a prefix and refuses an
/// empty field list on that clause alone.
#[test]
fn route_prefixes_and_field_lists_answer_their_own_clauses() {
    assert!(framed_route_id("amiss/test-route", "a1-b", &["x"]).is_some());
    assert!(framed_route_id("amiss/test-route", "test", &[]).is_none());
    assert!(framed_route_id("amiss/test-route", "test", &[""]).is_none());
}

#[test]
fn a_config_error_displays_its_reason() {
    let invalid = amiss_controller_service::ConfigError::invalid("endpoint limits are invalid");
    assert_eq!(invalid.to_string(), "endpoint limits are invalid");
    assert!(std::error::Error::source(&invalid).is_none());

    let caused = amiss_controller_service::ConfigError::caused_by(
        "configuration cannot be loaded",
        std::io::Error::other("broken input"),
    );
    assert_eq!(
        std::error::Error::source(&caused)
            .map(ToString::to_string)
            .as_deref(),
        Some("broken input")
    );
}

/// The plan loader maps both profile spellings, refuses the rest, and
/// carries a named floor file's exact bytes into the plan.
#[test]
fn a_plan_binds_its_profile_and_carries_its_floor() {
    let dir = TempDir::new().unwrap();
    let constraint = br#"{
  "schema": "amiss/scanner-execution-constraint",
  "action_repository": { "host": "github.com", "owner": "acme", "name": "amiss-action" },
  "action_object_format": "sha1",
  "action_commit_oid": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "action_tree_oid": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
  "manifest_path": "release/manifest.json",
  "release_manifest_digest": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
  "selected_platform": "linux-x86_64",
  "required_status_name": "amiss / documentation assurance",
  "bootstrap_contract": "amiss-action-bootstrap",
  "bootstrap_digest": "sha256:3333333333333333333333333333333333333333333333333333333333333333"
}"#;
    let constraint_path = dir.path().join("constraint.json");
    fs::write(&constraint_path, constraint).unwrap();
    let floor_bytes: &[u8] = br#"{
  "schema": "amiss/organization-floor",
  "floor_id": "acme/scanner-floor-2026-07",
  "repository": { "host": "github.com", "owner": "acme", "name": "docs" },
  "ref": "refs/heads/main",
  "minimum_profile": "observe",
  "minimum_dispositions": [],
  "protected_inventory": [],
  "protected_control_paths": [],
  "waivable_finding_kinds": [],
  "authorized_debt_owners": [],
  "authorized_waiver_issuers": [],
  "resource_limits": []
}"#;
    let floor_path = dir.path().join("floor.json");
    fs::write(&floor_path, floor_bytes).unwrap();

    let files = |profile: &str,
                 floor: bool,
                 external_policy: Option<&str>|
     -> amiss_controller_service::CheckPlanFiles {
        let mut value = json!({
            "profile": profile,
            "execution_constraint_file": constraint_path,
            "organization_floor_file": floor.then(|| floor_path.clone()),
        });
        if let Some(external_policy) = external_policy {
            value
                .as_object_mut()
                .unwrap()
                .insert("external_policy".to_owned(), json!(external_policy));
        }
        serde_json::from_value(value).unwrap()
    };
    let observed = load_plan(&files("observe", true, None), None).unwrap();
    assert_eq!(observed.profile, Profile::Observe);
    assert_eq!(observed.policy.external_policy, ExternalPolicy::Advisory);
    let floor = observed
        .policy
        .organization_floor
        .as_ref()
        .expect("the named floor file lands in the plan");
    assert_eq!(floor.bytes, floor_bytes);
    let advisory = load_plan(&files("enforce", false, None), None).unwrap();
    assert_eq!(advisory.profile, Profile::Enforce);
    assert_eq!(advisory.policy.external_policy, ExternalPolicy::Advisory);
    let with_inventory = load_intersphinx_plan(&dir, &constraint_path).unwrap();
    assert_eq!(with_inventory.policy.semantic_evidence.len(), 1);
    assert_ne!(with_inventory.digest, advisory.digest);
    let off = load_plan(&files("enforce", false, Some("off")), None).unwrap();
    assert_eq!(off.policy.external_policy, ExternalPolicy::Off);
    assert_ne!(off.digest, advisory.digest);
    let blocking = load_plan(
        &files("enforce", false, Some("block-confirmed-refutations")),
        None,
    )
    .unwrap();
    assert_eq!(
        blocking.policy.external_policy,
        ExternalPolicy::BlockConfirmedRefutations
    );
    assert_ne!(blocking.digest, advisory.digest);
    assert!(load_plan(&files("Observe", false, None), None).is_err());
    assert!(
        serde_json::from_value::<amiss_controller_service::CheckPlanFiles>(json!({
            "profile": "enforce",
            "external_policy": "block-everything",
            "execution_constraint_file": constraint_path,
        }))
        .is_err()
    );
}

#[test]
fn workflow_artifacts_require_and_reproduce_the_provider_scope() {
    let directory = TempDir::new().unwrap();
    let constraint = directory.path().join("constraint.json");
    fs::write(
        &constraint,
        include_bytes!("../../../../spec/examples/scanner-execution-constraint.json"),
    )
    .unwrap();
    let raw = json!({
        "profile": "enforce",
        "execution_constraint_file": constraint,
        "workflow_artifacts": [{
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
        }]
    });
    let files: amiss_controller_service::CheckPlanFiles =
        serde_json::from_value(raw.clone()).unwrap();
    assert_eq!(
        load_plan(&files, None).unwrap_err().to_string(),
        "workflow artifacts are unsupported by this provider lane"
    );

    let provider = ProviderIdentity::new("github".to_owned(), "github.com".to_owned()).unwrap();
    let repository = RepositoryIdentity::github("acme".to_owned(), "widget".to_owned()).unwrap();
    let plan = load_plan(&files, Some((&provider, &repository))).unwrap();
    let [artifact] = plan.policy.workflow_artifacts.as_slice() else {
        panic!("one configured artifact")
    };
    assert_eq!(artifact.provider, provider);
    assert_eq!(artifact.repository, repository);
    assert_eq!(artifact.workflow_identity.as_str(), "docs-evidence.yml");
    assert_eq!(
        artifact.semantic.acquisition_identity.as_str(),
        "github-docs-evidence"
    );

    let without_artifacts: amiss_controller_service::CheckPlanFiles =
        serde_json::from_value(json!({
            "profile": "enforce",
            "execution_constraint_file": raw["execution_constraint_file"]
        }))
        .unwrap();
    assert_ne!(
        load_plan(&without_artifacts, None).unwrap().digest,
        plan.digest
    );
}

#[test]
fn a_queued_service_error_displays_its_reason() {
    assert_eq!(
        amiss_controller_service::QueuedServiceError::InboxOpen(
            amiss_controller_service::InboxError::Corrupt,
        )
        .to_string(),
        "delivery inbox cannot be opened"
    );
}

/// The service-side errors name themselves distinctly.
#[test]
fn service_errors_name_themselves() {
    use amiss_controller_service::{EndpointConfigError, SupervisionError};
    let endpoint = [EndpointConfigError::Path, EndpointConfigError::Limits];
    let messages: Vec<String> = endpoint.iter().map(ToString::to_string).collect();
    assert!(messages.iter().all(|message| !message.is_empty()));
    assert_ne!(messages[0], messages[1]);
    assert_eq!(
        SupervisionError::Worker(amiss_controller_service::DeliveryWorkerError::Clock).to_string(),
        "delivery worker stopped"
    );
}
