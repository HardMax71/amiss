use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

use crate::de::{self, Error, ErrorKind, fail};
use crate::digest::{Digest, hb};
use crate::model::{ObjectFormat, Oid, RepoPathText, RepositoryIdentity};

use super::{root, validate_repository};

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
#[serde(deny_unknown_fields)]
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
    root(bytes)?;
    let descriptor = de::deserialize_json(bytes)?;
    validate_execution_constraint(&descriptor)?;
    Ok(descriptor)
}

/// Produces one valid execution constraint's canonical bytes and
/// domain-separated digest together.
///
/// # Errors
///
/// A public field violates the same laws [`parse_execution_constraint`]
/// enforces, or the typed value cannot be serialized.
pub fn canonical_execution_constraint(
    descriptor: &ExecutionConstraintDescriptor,
) -> Result<(Vec<u8>, Digest), Error> {
    validate_execution_constraint(descriptor)?;
    let bytes = serde_json_canonicalizer::to_vec(descriptor)
        .map_err(|_defect| Error::new("$", ErrorKind::InvalidValue))?;
    let digest = hb(EXECUTION_CONSTRAINT_SCHEMA, &bytes);
    Ok((bytes, digest))
}

fn validate_execution_constraint(descriptor: &ExecutionConstraintDescriptor) -> Result<(), Error> {
    validate_repository("$.action_repository", &descriptor.action_repository)?;
    for (path, oid) in [
        ("$.action_commit_oid", &descriptor.action_commit_oid),
        ("$.action_tree_oid", &descriptor.action_tree_oid),
    ] {
        if oid.object_format() != descriptor.action_object_format {
            return fail(path, ErrorKind::InvalidValue);
        }
    }
    valid_required_status_name(&descriptor.required_status_name)
        .then_some(())
        .ok_or_else(|| Error::new("$.required_status_name", ErrorKind::InvalidValue))
}
