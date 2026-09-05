use super::model;
use super::{
    EVALUATOR_MANAGED_MEMORY_BYTES, PRIVATE_TEMPORARY_STORAGE_BYTES, SANDBOX_SCHEMA,
    WATCHDOG_MILLISECONDS,
};
use crate::digest::{Digest, hj_serde};

/// The engine's self-asserted zero-capability descriptor and its exact digest.
///
/// # Errors
/// Returns a serialization error without producing a partial digest.
pub fn sandbox_descriptor() -> serde_json::Result<(model::SandboxDescriptor, Digest)> {
    let descriptor = model::SandboxDescriptor {
        schema: model::SandboxDescriptorSchema::Current,
        profile: model::SandboxProfile::ZeroCapability,
        isolation: model::SandboxIsolation::Process,
        network: model::Denied::Denied,
        child_processes: model::Denied::Denied,
        repository_processes: model::Denied::Denied,
        credentials: model::Absent::Absent,
        secrets: model::Absent::Absent,
        shared_cache: model::Denied::Denied,
        workspace: model::ReadOnly::ReadOnly,
        environment: model::ScannerProcessEnvironment::ScannerProcessEnvironment,
        physical_memory: model::MemoryLimit {
            maximum_bytes: EVALUATOR_MANAGED_MEMORY_BYTES,
        },
        temporary_storage: model::TemporaryStorage {
            kind: model::PrivateBoundedStorage::PrivateBounded,
            maximum_bytes: PRIVATE_TEMPORARY_STORAGE_BYTES,
        },
        watchdog: model::Watchdog {
            maximum_milliseconds: WATCHDOG_MILLISECONDS,
        },
    };
    let digest = hj_serde(SANDBOX_SCHEMA, |writer| {
        serde_json::to_writer(writer, &descriptor)
    })?;
    Ok((descriptor, digest))
}
