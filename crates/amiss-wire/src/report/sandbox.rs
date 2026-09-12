use super::{
    EVALUATOR_MANAGED_MEMORY_BYTES, PRIVATE_TEMPORARY_STORAGE_BYTES, SANDBOX_SCHEMA,
    WATCHDOG_MILLISECONDS,
};
use crate::codec;
use crate::de::Error;
use crate::digest::Digest;
use crate::json::Value;
use serde::Serialize;

#[derive(Serialize)]
struct SandboxDescriptor {
    child_processes: &'static str,
    credentials: &'static str,
    environment: &'static str,
    isolation: &'static str,
    network: &'static str,
    physical_memory: Memory,
    profile: &'static str,
    repository_processes: &'static str,
    schema: &'static str,
    secrets: &'static str,
    shared_cache: &'static str,
    temporary_storage: Storage,
    watchdog: Watchdog,
    workspace: &'static str,
}

#[derive(Serialize)]
struct Memory {
    maximum_bytes: u64,
}

#[derive(Serialize)]
struct Storage {
    kind: &'static str,
    maximum_bytes: u64,
}

#[derive(Serialize)]
struct Watchdog {
    maximum_milliseconds: u64,
}

/// The zero-capability sandbox descriptor the engine asserts for itself.
///
/// # Errors
///
/// A declared resource ceiling cannot be represented by the wire profile.
pub fn sandbox_descriptor() -> Result<(Value, Digest), Error> {
    let descriptor = SandboxDescriptor {
        child_processes: "denied",
        credentials: "absent",
        environment: "scanner-process-env",
        isolation: "process",
        network: "denied",
        physical_memory: Memory {
            maximum_bytes: EVALUATOR_MANAGED_MEMORY_BYTES,
        },
        profile: "scanner-zero-capability",
        repository_processes: "denied",
        schema: SANDBOX_SCHEMA,
        secrets: "absent",
        shared_cache: "denied",
        temporary_storage: Storage {
            kind: "private-bounded",
            maximum_bytes: PRIVATE_TEMPORARY_STORAGE_BYTES,
        },
        watchdog: Watchdog {
            maximum_milliseconds: WATCHDOG_MILLISECONDS,
        },
        workspace: "read-only",
    };
    let digest = codec::digest(SANDBOX_SCHEMA, &descriptor)?;
    Ok((codec::to_value(&descriptor)?, digest))
}
