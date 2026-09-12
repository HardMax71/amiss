use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

use crate::de::{self, Error, ErrorKind, fail};
use crate::model::Digest;
use crate::model::{ObjectFormat, Oid, RepoPathText, RepositoryIdentity};

use super::validate_repository;

pub const EXECUTION_CONSTRAINT_SCHEMA: &str = "amiss/scanner-execution-constraint";
pub const ACTION_BOOTSTRAP_CONTRACT: &str = "amiss-action-bootstrap";

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum ExecutionConstraintSchema {
    #[strum(serialize = "amiss/scanner-execution-constraint")]
    Current,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum ActionBootstrapContract {
    #[strum(serialize = "amiss-action-bootstrap")]
    Current,
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Display,
    SerializeDisplay,
    DeserializeFromStr,
    strum::AsRefStr,
    EnumString,
    strum::IntoStaticStr,
)]
pub enum ConstraintPlatform {
    #[strum(serialize = "linux-x86_64")]
    LinuxX8664,
    #[strum(serialize = "linux-aarch64")]
    LinuxAarch64,
    #[strum(serialize = "macos-x86_64")]
    MacosX8664,
    #[strum(serialize = "macos-aarch64")]
    MacosAarch64,
    #[strum(serialize = "windows-x86_64")]
    WindowsX8664,
    #[strum(serialize = "windows-aarch64")]
    WindowsAarch64,
}

/// The externally protected allow-list entry for one scanner action tree,
/// release manifest, bootstrap contract, and required provider status name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(remote = "Self", deny_unknown_fields)]
pub struct ExecutionConstraintDescriptor {
    pub action_commit_oid: Oid,
    pub action_object_format: ObjectFormat,
    pub action_repository: RepositoryIdentity,
    pub action_tree_oid: Oid,
    pub bootstrap_contract: ActionBootstrapContract,
    pub bootstrap_digest: Digest,
    pub manifest_path: RepoPathText,
    pub release_manifest_digest: Digest,
    pub required_status_name: String,
    pub schema: ExecutionConstraintSchema,
    pub selected_platform: ConstraintPlatform,
}

impl Serialize for ExecutionConstraintDescriptor {
    fn serialize<Ser: serde::Serializer>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for ExecutionConstraintDescriptor {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(serde_with::with_prefix::WithPrefix {
            delegate: deserializer,
            prefix: "",
        })
    }
}

#[must_use]
pub fn valid_required_status_name(raw: &str) -> bool {
    let bytes = raw.as_bytes();
    let interior = |byte: &u8| {
        byte.is_ascii_alphanumeric() || matches!(byte, b' ' | b'.' | b'_' | b'/' | b':' | b'-')
    };
    let edge = |byte: &u8| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'/' | b':' | b'-')
    };
    match (bytes.first(), bytes.last()) {
        (Some(first), Some(last)) => {
            bytes.len() <= 160
                && first.is_ascii_alphanumeric()
                && (bytes.len() == 1 || edge(last))
                && bytes.iter().all(interior)
        }
        _ => false,
    }
}

/// Parses and validates one execution constraint.
///
/// # Errors
///
/// Fails on strict-JSON defects, schema-shape violations, invalid grammar
/// values, or object IDs inconsistent with the declared object format.
pub fn parse_execution_constraint(bytes: &[u8]) -> Result<ExecutionConstraintDescriptor, Error> {
    de::JsonProfile::validate(bytes)?;
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    deserializer.disable_recursion_limit();
    let descriptor: ExecutionConstraintDescriptor =
        serde_path_to_error::deserialize(&mut deserializer)
            .map_err(|defect| de::deserialize_error("$", &defect))?;
    deserializer
        .end()
        .map_err(|defect| Error::new("$", ErrorKind::Json(defect.to_string())))?;
    descriptor.validate()?;
    Ok(descriptor)
}

impl ExecutionConstraintDescriptor {
    /// Checks this control's domain rules and resource limits.
    ///
    /// # Errors
    ///
    /// A public field violates the contract enforced by [`parse_execution_constraint`].
    pub fn validate(&self) -> Result<(), Error> {
        validate_repository("$.action_repository", &self.action_repository)?;
        for (path, oid) in [
            ("$.action_commit_oid", &self.action_commit_oid),
            ("$.action_tree_oid", &self.action_tree_oid),
        ] {
            if oid.object_format() != self.action_object_format {
                return fail(path, ErrorKind::InvalidValue);
            }
        }
        valid_required_status_name(&self.required_status_name)
            .then_some(())
            .ok_or_else(|| Error::new("$.required_status_name", ErrorKind::InvalidValue))
    }
}
