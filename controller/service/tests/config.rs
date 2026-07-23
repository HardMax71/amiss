use std::fs::{self, File};
use std::time::Duration;

use amiss_controller_service::{
    ExecutionLimits, ExecutionPaths, ServiceLimits, ServicePaths, framed_route_id,
    load_execution_limits, load_limits, read_regular,
};
use fs_at::{LinkEntryType, OpenOptions};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tempfile::TempDir;

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
    let root_file = File::open(root.path())?;
    OpenOptions::default().symlink_at(&root_file, "linked", LinkEntryType::File, "target")?;
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
    assert_eq!(loaded.evaluation.max_concurrent_evaluations, 4);
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
        assert_eq!(loaded.receiver.max_concurrent_deliveries, value);
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
fn queued_paths_add_only_the_inbox_field() {
    let execution = json!({
        "bootstrap": "/controller/amiss-bootstrap",
        "scratch": "/controller/scratch",
        "ledger": "/controller/ledger"
    });
    assert!(serde_json::from_value::<ExecutionPaths>(execution.clone()).is_ok());
    assert!(
        serde_json::from_value::<ExecutionPaths>(json!({
            "bootstrap": "/controller/amiss-bootstrap",
            "scratch": "/controller/scratch",
            "ledger": "/controller/ledger",
            "inbox": "/controller/inbox"
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<ServicePaths>(json!({
            "bootstrap": "/controller/amiss-bootstrap",
            "scratch": "/controller/scratch",
            "ledger": "/controller/ledger",
            "inbox": "/controller/inbox"
        }))
        .is_ok()
    );
    assert!(
        serde_json::from_value::<ServicePaths>(json!({
            "bootstrap": "/controller/amiss-bootstrap",
            "scratch": "/controller/scratch",
            "ledger": "/controller/ledger",
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
