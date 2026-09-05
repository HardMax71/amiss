use serde::{Deserialize, Serialize};

use crate::controls::ConstraintPlatform;
use crate::digest::Digest;
use crate::model::ArtifactId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SandboxDescriptorSchema {
    #[serde(rename = "amiss/scanner-sandbox-profile")]
    Current,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SandboxProfile {
    #[serde(rename = "scanner-zero-capability")]
    ZeroCapability,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SandboxIsolation {
    Container,
    Process,
    VirtualMachine,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Denied {
    #[serde(rename = "denied")]
    Denied,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Absent {
    #[serde(rename = "absent")]
    Absent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReadOnly {
    #[serde(rename = "read-only")]
    ReadOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScannerProcessEnvironment {
    #[serde(rename = "scanner-process-env")]
    ScannerProcessEnvironment,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrivateBoundedStorage {
    #[serde(rename = "private-bounded")]
    PrivateBounded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryLimit {
    pub maximum_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemporaryStorage {
    pub kind: PrivateBoundedStorage,
    pub maximum_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Watchdog {
    pub maximum_milliseconds: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxDescriptor {
    pub child_processes: Denied,
    pub credentials: Absent,
    pub environment: ScannerProcessEnvironment,
    pub isolation: SandboxIsolation,
    pub network: Denied,
    pub physical_memory: MemoryLimit,
    pub profile: SandboxProfile,
    pub repository_processes: Denied,
    pub schema: SandboxDescriptorSchema,
    pub secrets: Absent,
    pub shared_cache: Denied,
    pub temporary_storage: TemporaryStorage,
    pub watchdog: Watchdog,
    pub workspace: ReadOnly,
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    serde_with::SerializeDisplay,
    serde_with::DeserializeFromStr,
    strum::Display,
    strum::EnumString,
    strum::AsRefStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum SandboxAssurance {
    ProviderVerified,
    SelfAsserted,
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    serde_with::SerializeDisplay,
    serde_with::DeserializeFromStr,
    strum::Display,
    strum::EnumString,
    strum::AsRefStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum SandboxEnforcementSource {
    ExternalRequiredCheck,
    LocalProcess,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SandboxVerificationSchema {
    #[serde(rename = "amiss/scanner-sandbox-verification")]
    Current,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SandboxVerifier {
    #[serde(rename = "external-required-check")]
    ExternalRequiredCheck,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SandboxMechanism {
    MicrovmSandbox,
    OciRootlessSandbox,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxVerification {
    pub evaluation_identity_digest: Digest,
    pub execution_constraint_digest: Digest,
    pub mechanism: SandboxMechanism,
    pub platform: ConstraintPlatform,
    pub provider: ArtifactId,
    pub provider_run_attempt: u64,
    pub provider_run_id: String,
    pub sandbox_descriptor_digest: Digest,
    pub schema: SandboxVerificationSchema,
    pub verifier: SandboxVerifier,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxProvenance {
    pub assurance: SandboxAssurance,
    pub descriptor: SandboxDescriptor,
    pub descriptor_digest: Digest,
    pub enforcement_source: SandboxEnforcementSource,
    #[serde(deserialize_with = "Option::deserialize")]
    pub verification: Option<SandboxVerification>,
}
