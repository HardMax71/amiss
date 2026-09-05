use serde::{Deserialize, Serialize};

use crate::controls::{ExecutionConstraintDescriptor, Profile, TrustedTimeStatement};
use crate::digest::Digest;
use crate::model::ArtifactId;
use crate::requests::RequestTrust;

use super::{SandboxProvenance, UnavailableStatus};

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
pub enum ControlStatus {
    None,
    Verified,
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
pub enum ControlTrustSource {
    ExternalRequiredCheck,
    None,
    OrganizationPolicy,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlProvenance {
    #[serde(deserialize_with = "Option::deserialize")]
    pub digest: Option<Digest>,
    pub status: ControlStatus,
    pub trust_source: ControlTrustSource,
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
pub enum NoControlStatus {
    #[strum(serialize = "none")]
    None,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoExecutionConstraint {
    pub status: NoControlStatus,
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
pub enum VerifiedControlStatus {
    #[strum(serialize = "verified")]
    Verified,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifiedExecutionConstraint {
    pub descriptor: ExecutionConstraintDescriptor,
    pub descriptor_digest: Digest,
    pub status: VerifiedControlStatus,
    pub trust_source: RequestTrust,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ExecutionConstraintProvenance {
    None(NoExecutionConstraint),
    Verified(Box<VerifiedExecutionConstraint>),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoTrustedTime {
    pub status: NoControlStatus,
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
pub enum TrustedTimeTrustSource {
    #[strum(serialize = "external-required-check")]
    ExternalRequiredCheck,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifiedTrustedTime {
    pub statement: TrustedTimeStatement,
    pub statement_digest: Digest,
    pub status: VerifiedControlStatus,
    pub trust_source: TrustedTimeTrustSource,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TrustedTimeProvenance {
    None(NoTrustedTime),
    Verified(Box<VerifiedTrustedTime>),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticEvidenceProducer {
    pub identity: ArtifactId,
    pub input_digest: Digest,
    pub kind: ArtifactId,
    pub version: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticEvidenceProvenance {
    pub payload_digest: Digest,
    pub producer: SemanticEvidenceProducer,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedControls {
    #[serde(deserialize_with = "Option::deserialize")]
    pub base_repository_policy_digest: Option<Digest>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub candidate_repository_policy_digest: Option<Digest>,
    pub debt_snapshot: ControlProvenance,
    pub execution_constraint: ExecutionConstraintProvenance,
    pub organization_floor: ControlProvenance,
    pub profile: Profile,
    pub sandbox: SandboxProvenance,
    #[serde(
        default,
        deserialize_with = "json_serde::deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub semantic_evidence: Option<Vec<SemanticEvidenceProvenance>>,
    pub trusted_time_source: TrustedTimeProvenance,
    pub waiver_bundle: ControlProvenance,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, strum::AsRefStr, strum::EnumIter,
)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum ControlsUnavailableReason {
    ControlBindingMismatch,
    InvalidExternalControl,
    InvalidProfile,
    InvalidRepositoryPolicy,
    NotParsed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnavailableControls {
    pub reasons: Vec<ControlsUnavailableReason>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub request_digest: Option<Digest>,
    pub status: UnavailableStatus,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Controls {
    Resolved(Box<ResolvedControls>),
    Unavailable(UnavailableControls),
}
