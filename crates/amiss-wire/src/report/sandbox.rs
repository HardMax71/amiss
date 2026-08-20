use crate::digest::{Digest, hj};
use crate::json::Value;

use super::{
    EVALUATOR_MANAGED_MEMORY_BYTES, PRIVATE_TEMPORARY_STORAGE_BYTES, SANDBOX_SCHEMA,
    WATCHDOG_MILLISECONDS, object, string,
};

/// The zero-capability sandbox descriptor the engine asserts for itself, and
/// its digest. A future wrapper verifies rather than asserts it.
#[must_use]
pub fn sandbox_descriptor() -> (Value, Digest) {
    let descriptor = object(vec![
        ("schema", string(SANDBOX_SCHEMA)),
        ("profile", string("scanner-zero-capability")),
        ("isolation", string("process")),
        ("network", string("denied")),
        ("child_processes", string("denied")),
        ("repository_processes", string("denied")),
        ("credentials", string("absent")),
        ("secrets", string("absent")),
        ("shared_cache", string("denied")),
        ("workspace", string("read-only")),
        ("environment", string("scanner-process-env")),
        (
            "physical_memory",
            object(vec![(
                "maximum_bytes",
                Value::Integer(i64::try_from(EVALUATOR_MANAGED_MEMORY_BYTES).unwrap_or(i64::MAX)),
            )]),
        ),
        (
            "temporary_storage",
            object(vec![
                ("kind", string("private-bounded")),
                (
                    "maximum_bytes",
                    Value::Integer(
                        i64::try_from(PRIVATE_TEMPORARY_STORAGE_BYTES).unwrap_or(i64::MAX),
                    ),
                ),
            ]),
        ),
        (
            "watchdog",
            object(vec![(
                "maximum_milliseconds",
                Value::Integer(i64::try_from(WATCHDOG_MILLISECONDS).unwrap_or(i64::MAX)),
            )]),
        ),
    ]);
    let digest = hj(SANDBOX_SCHEMA, &descriptor);
    (descriptor, digest)
}
