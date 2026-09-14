use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use std::borrow::Cow;
use strum::{Display, EnumString};

use crate::de::{Document, Error, ErrorKind, fail};
use crate::model::ArtifactId;
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

impl ConstraintPlatform {
    /// The release artifact that carries this platform's executable.
    #[must_use]
    pub const fn artifact_name(self) -> ArtifactId {
        match self {
            Self::LinuxX8664 => crate::artifact_id!("amiss-linux-x86_64"),
            Self::LinuxAarch64 => crate::artifact_id!("amiss-linux-aarch64"),
            Self::MacosX8664 => crate::artifact_id!("amiss-macos-x86_64"),
            Self::MacosAarch64 => crate::artifact_id!("amiss-macos-aarch64"),
            Self::WindowsX8664 => crate::artifact_id!("amiss-windows-x86_64"),
            Self::WindowsAarch64 => crate::artifact_id!("amiss-windows-aarch64"),
        }
    }
}

/// The admitted name of one required provider check.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct RequiredStatusName(Cow<'static, str>);

impl RequiredStatusName {
    const INVALID: &'static str = "invalid required status name";

    /// # Panics
    /// When the literal is not a status name; `required_status_name!` makes that a build error.
    #[must_use]
    pub const fn from_static(raw: &'static str) -> Self {
        assert!(valid_required_status_name(raw), "{}", Self::INVALID);
        Self(Cow::Borrowed(raw))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for RequiredStatusName {
    type Error = &'static str;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        if valid_required_status_name(&raw) {
            Ok(Self(Cow::Owned(raw)))
        } else {
            Err(Self::INVALID)
        }
    }
}

#[macro_export]
macro_rules! required_status_name {
    ($raw:literal) => {
        const { $crate::controls::RequiredStatusName::from_static($raw) }
    };
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
    pub required_status_name: RequiredStatusName,
    pub schema: ExecutionConstraintSchema,
    pub selected_platform: ConstraintPlatform,
}

#[must_use]
pub const fn valid_required_status_name(raw: &str) -> bool {
    let bytes = raw.as_bytes();
    let (Some((&first, _)), Some((&last, _))) = (bytes.split_first(), bytes.split_last()) else {
        return false;
    };
    if bytes.len() > 160 || !first.is_ascii_alphanumeric() || last == b' ' {
        return false;
    }
    let mut rest = bytes;
    while let Some((&byte, tail)) = rest.split_first() {
        rest = tail;
        if !byte.is_ascii_alphanumeric() && !matches!(byte, b' ' | b'.' | b'_' | b'/' | b':' | b'-')
        {
            return false;
        }
    }
    true
}

impl Document for ExecutionConstraintDescriptor {
    type Defect = Error;

    /// Checks this control's domain rules and resource limits.
    ///
    /// # Errors
    ///
    /// A public field violates the contract enforced by [`parse_execution_constraint`].
    fn validate(&self) -> Result<(), Error> {
        validate_repository("$.action_repository", &self.action_repository)?;
        for (path, oid) in [
            ("$.action_commit_oid", &self.action_commit_oid),
            ("$.action_tree_oid", &self.action_tree_oid),
        ] {
            if oid.object_format() != self.action_object_format {
                return fail(path, ErrorKind::InvalidValue);
            }
        }
        Ok(())
    }
}
