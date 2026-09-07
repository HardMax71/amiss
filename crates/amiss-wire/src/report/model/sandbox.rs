use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

use crate::controls::ConstraintPlatform;
use crate::digest::Digest;
use crate::model::ArtifactId;

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum SandboxDescriptorSchema {
    #[strum(serialize = "amiss/scanner-sandbox-profile")]
    Current,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum SandboxProfile {
    #[strum(serialize = "scanner-zero-capability")]
    ZeroCapability,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum SandboxIsolation {
    Container,
    Process,
    VirtualMachine,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum Denied {
    #[strum(serialize = "denied")]
    Denied,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum Absent {
    #[strum(serialize = "absent")]
    Absent,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum ReadOnly {
    #[strum(serialize = "read-only")]
    ReadOnly,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum ScannerProcessEnvironment {
    #[strum(serialize = "scanner-process-env")]
    ScannerProcessEnvironment,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum PrivateBoundedStorage {
    #[strum(serialize = "private-bounded")]
    PrivateBounded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryLimit {
    pub maximum_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemporaryStorage {
    pub kind: PrivateBoundedStorage,
    pub maximum_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Watchdog {
    pub maximum_milliseconds: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SandboxDescriptor {
    pub child_processes: Denied,
    pub credentials: Absent,
    pub environment: ScannerProcessEnvironment,
    pub isolation: SandboxIsolation,
    pub network: Denied,
    #[serde(deserialize_with = "crate::requests::object::deserialize")]
    pub physical_memory: MemoryLimit,
    pub profile: SandboxProfile,
    pub repository_processes: Denied,
    pub schema: SandboxDescriptorSchema,
    pub secrets: Absent,
    pub shared_cache: Denied,
    #[serde(deserialize_with = "crate::requests::object::deserialize")]
    pub temporary_storage: TemporaryStorage,
    #[serde(deserialize_with = "crate::requests::object::deserialize")]
    pub watchdog: Watchdog,
    pub workspace: ReadOnly,
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    SerializeDisplay,
    DeserializeFromStr,
    Display,
    EnumString,
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
    SerializeDisplay,
    DeserializeFromStr,
    Display,
    EnumString,
    strum::AsRefStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum SandboxEnforcementSource {
    ExternalRequiredCheck,
    LocalProcess,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum SandboxVerificationSchema {
    #[strum(serialize = "amiss/scanner-sandbox-verification")]
    Current,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum SandboxVerifier {
    #[strum(serialize = "external-required-check")]
    ExternalRequiredCheck,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum SandboxMechanism {
    MicrovmSandbox,
    OciRootlessSandbox,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct SandboxProvenance {
    pub assurance: SandboxAssurance,
    #[serde(deserialize_with = "crate::requests::object::deserialize")]
    pub descriptor: SandboxDescriptor,
    pub descriptor_digest: Digest,
    pub enforcement_source: SandboxEnforcementSource,
    #[serde(deserialize_with = "Option::deserialize")]
    pub verification: Option<SandboxVerification>,
}
