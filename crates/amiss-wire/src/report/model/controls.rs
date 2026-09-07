use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

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
    SerializeDisplay,
    DeserializeFromStr,
    Display,
    EnumString,
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
    SerializeDisplay,
    DeserializeFromStr,
    Display,
    EnumString,
    strum::AsRefStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum ControlTrustSource {
    ExternalRequiredCheck,
    None,
    OrganizationPolicy,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
    SerializeDisplay,
    DeserializeFromStr,
    Display,
    EnumString,
    strum::AsRefStr,
)]
pub enum NoControlStatus {
    #[strum(serialize = "none")]
    None,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoExecutionConstraint {
    pub status: NoControlStatus,
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
pub enum VerifiedControlStatus {
    #[strum(serialize = "verified")]
    Verified,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifiedExecutionConstraint {
    #[serde(deserialize_with = "crate::requests::object::deserialize")]
    pub descriptor: ExecutionConstraintDescriptor,
    pub descriptor_digest: Digest,
    pub status: VerifiedControlStatus,
    pub trust_source: RequestTrust,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ExecutionConstraintProvenance {
    None(#[serde(deserialize_with = "crate::requests::object::deserialize")] NoExecutionConstraint),
    Verified(
        #[serde(deserialize_with = "crate::requests::object::deserialize")]
        Box<VerifiedExecutionConstraint>,
    ),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoTrustedTime {
    pub status: NoControlStatus,
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
pub enum TrustedTimeTrustSource {
    #[strum(serialize = "external-required-check")]
    ExternalRequiredCheck,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifiedTrustedTime {
    #[serde(deserialize_with = "crate::requests::object::deserialize")]
    pub statement: TrustedTimeStatement,
    pub statement_digest: Digest,
    pub status: VerifiedControlStatus,
    pub trust_source: TrustedTimeTrustSource,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TrustedTimeProvenance {
    None(#[serde(deserialize_with = "crate::requests::object::deserialize")] NoTrustedTime),
    Verified(
        #[serde(deserialize_with = "crate::requests::object::deserialize")]
        Box<VerifiedTrustedTime>,
    ),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticEvidenceProducer {
    pub identity: ArtifactId,
    pub input_digest: Digest,
    pub kind: crate::semantic::SemanticProducerKind,
    pub version: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticEvidenceProvenance {
    pub payload_digest: Digest,
    #[serde(deserialize_with = "crate::requests::object::deserialize")]
    pub producer: SemanticEvidenceProducer,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedControls {
    #[serde(deserialize_with = "Option::deserialize")]
    pub base_repository_policy_digest: Option<Digest>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub candidate_repository_policy_digest: Option<Digest>,
    #[serde(deserialize_with = "crate::requests::object::deserialize")]
    pub debt_snapshot: ControlProvenance,
    pub execution_constraint: ExecutionConstraintProvenance,
    #[serde(deserialize_with = "crate::requests::object::deserialize")]
    pub organization_floor: ControlProvenance,
    pub profile: Profile,
    #[serde(deserialize_with = "crate::requests::object::deserialize")]
    pub sandbox: SandboxProvenance,
    #[serde(
        default,
        deserialize_with = "json_serde::deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub semantic_evidence: Option<Vec<SemanticEvidenceProvenance>>,
    pub trusted_time_source: TrustedTimeProvenance,
    #[serde(deserialize_with = "crate::requests::object::deserialize")]
    pub waiver_bundle: ControlProvenance,
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Display,
    EnumString,
    SerializeDisplay,
    DeserializeFromStr,
    strum::AsRefStr,
    strum::EnumIter,
)]
#[strum(serialize_all = "kebab-case")]
pub enum ControlsUnavailableReason {
    ControlBindingMismatch,
    InvalidExternalControl,
    InvalidProfile,
    InvalidRepositoryPolicy,
    NotParsed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnavailableControls {
    pub reasons: Vec<ControlsUnavailableReason>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub request_digest: Option<Digest>,
    pub status: UnavailableStatus,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Controls {
    Resolved(
        #[serde(deserialize_with = "crate::requests::object::deserialize")] Box<ResolvedControls>,
    ),
    Unavailable(
        #[serde(deserialize_with = "crate::requests::object::deserialize")] UnavailableControls,
    ),
}
