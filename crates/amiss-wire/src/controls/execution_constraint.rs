use super::mapping::wire_fields;
use super::{check_schema, root};
use crate::codec;
use crate::de::{Error, ErrorKind, fail};
use crate::digest::{Digest, hj};
use crate::json::Value;
use crate::model::{ObjectFormat, Oid, RepoPathText, RepositoryIdentity};
use serde::{Deserialize, Serialize};

const EXECUTION_CONSTRAINT_SCHEMA: &str = "amiss/scanner-execution-constraint";
const ACTION_BOOTSTRAP_CONTRACT: &str = "amiss-action-bootstrap";

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    strum::AsRefStr,
    strum::EnumString,
    strum::IntoStaticStr,
    Serialize,
    Deserialize,
)]
pub enum ConstraintPlatform {
    #[strum(serialize = "linux-x86_64")]
    #[serde(rename = "linux-x86_64")]
    LinuxX8664,
    #[strum(serialize = "linux-aarch64")]
    #[serde(rename = "linux-aarch64")]
    LinuxAarch64,
    #[strum(serialize = "macos-x86_64")]
    #[serde(rename = "macos-x86_64")]
    MacosX8664,
    #[strum(serialize = "macos-aarch64")]
    #[serde(rename = "macos-aarch64")]
    MacosAarch64,
    #[strum(serialize = "windows-x86_64")]
    #[serde(rename = "windows-x86_64")]
    WindowsX8664,
    #[strum(serialize = "windows-aarch64")]
    #[serde(rename = "windows-aarch64")]
    WindowsAarch64,
}

/// The externally protected allow-list entry for one scanner action tree,
/// release manifest, bootstrap contract, and required provider status name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionConstraintDescriptor {
    digest: Digest,
    input: ExecutionConstraintInput,
}

/// The controller-owned fields of an execution constraint. The schema and
/// bootstrap contract are fixed by the wire type, and the digest is derived.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionConstraintInput {
    pub action_repository: RepositoryIdentity,
    pub action_object_format: ObjectFormat,
    pub action_commit_oid: Oid,
    pub action_tree_oid: Oid,
    pub manifest_path: RepoPathText,
    pub release_manifest_digest: Digest,
    pub selected_platform: ConstraintPlatform,
    pub required_status_name: String,
    pub bootstrap_digest: Digest,
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

impl ExecutionConstraintDescriptor {
    #[must_use]
    pub const fn digest(&self) -> Digest {
        self.digest
    }

    #[must_use]
    pub fn action_repository(&self) -> &RepositoryIdentity {
        &self.input.action_repository
    }

    #[must_use]
    pub const fn action_object_format(&self) -> ObjectFormat {
        self.input.action_object_format
    }

    #[must_use]
    pub fn action_commit_oid(&self) -> &Oid {
        &self.input.action_commit_oid
    }

    #[must_use]
    pub fn action_tree_oid(&self) -> &Oid {
        &self.input.action_tree_oid
    }

    #[must_use]
    pub fn manifest_path(&self) -> &RepoPathText {
        &self.input.manifest_path
    }

    #[must_use]
    pub const fn release_manifest_digest(&self) -> Digest {
        self.input.release_manifest_digest
    }

    #[must_use]
    pub const fn selected_platform(&self) -> ConstraintPlatform {
        self.input.selected_platform
    }

    #[must_use]
    pub fn required_status_name(&self) -> &str {
        &self.input.required_status_name
    }

    #[must_use]
    pub const fn bootstrap_digest(&self) -> Digest {
        self.input.bootstrap_digest
    }

    /// Builds a descriptor through the same grammar, consistency, and digest
    /// rules used for untrusted wire bytes. Construction does not authenticate
    /// the action repository, tree, manifest, or bootstrap; the controller must
    /// acquire those values independently of the repository-controlled run.
    ///
    /// # Errors
    ///
    /// A field violates [`Self::parse`], including an object ID that does not
    /// match `action_object_format` or an invalid required status name.
    pub fn new(input: ExecutionConstraintInput) -> Result<Self, Error> {
        let payload = Constraint::from(input);
        payload.check()?;
        let digest = codec::digest(EXECUTION_CONSTRAINT_SCHEMA, &payload)?;
        Ok(Self::from_payload(payload, digest))
    }

    /// # Errors
    ///
    /// Fails on strict-JSON defects, schema-shape violations, and invalid
    /// grammar values.
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        Self::from_value(&root(bytes)?)
    }

    /// Checks an already decoded control without serializing it again.
    ///
    /// # Errors
    ///
    /// A field violates the closed shape or the control's domain laws.
    pub fn from_value(value: &Value) -> Result<Self, Error> {
        let payload: Constraint = codec::from_value("$", value)?;
        payload.check()?;
        Ok(Self::from_payload(
            payload,
            hj(EXECUTION_CONSTRAINT_SCHEMA, value),
        ))
    }

    fn from_payload(payload: Constraint, digest: Digest) -> Self {
        Self {
            digest,
            input: payload.into(),
        }
    }

    /// Serializes one valid descriptor to its unique canonical JSON bytes.
    ///
    /// # Errors
    ///
    /// The stored descriptor violates its domain laws or its derived digest
    /// does not match the canonical value.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, Error> {
        let payload = Constraint::from(ExecutionConstraintInput::from(self));
        payload.check()?;
        if codec::digest(EXECUTION_CONSTRAINT_SCHEMA, &payload)? != self.digest {
            return fail("$.digest", ErrorKind::DigestMismatch);
        }
        codec::canonical(&payload)
    }
}

impl From<&ExecutionConstraintDescriptor> for ExecutionConstraintInput {
    fn from(value: &ExecutionConstraintDescriptor) -> Self {
        value.input.clone()
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Constraint {
    schema: String,
    action_repository: RepositoryIdentity,
    action_object_format: ObjectFormat,
    action_commit_oid: Oid,
    action_tree_oid: Oid,
    manifest_path: RepoPathText,
    release_manifest_digest: Digest,
    selected_platform: ConstraintPlatform,
    required_status_name: String,
    bootstrap_contract: String,
    bootstrap_digest: Digest,
}

wire_fields! {
    Constraint <=> ExecutionConstraintInput (input) {
        fields [
            action_repository,
            action_object_format,
            action_commit_oid,
            action_tree_oid,
            manifest_path,
            release_manifest_digest,
            selected_platform,
            required_status_name,
            bootstrap_digest,
        ],
        mapped [],
        wire {
            schema: EXECUTION_CONSTRAINT_SCHEMA.to_owned(),
            bootstrap_contract: ACTION_BOOTSTRAP_CONTRACT.to_owned(),
        },
        domain {}
    }
}

impl Constraint {
    fn check(&self) -> Result<(), Error> {
        check_schema("$.schema", &self.schema, EXECUTION_CONSTRAINT_SCHEMA)?;
        check_schema(
            "$.bootstrap_contract",
            &self.bootstrap_contract,
            ACTION_BOOTSTRAP_CONTRACT,
        )?;
        for (path, oid) in [
            ("$.action_commit_oid", &self.action_commit_oid),
            ("$.action_tree_oid", &self.action_tree_oid),
        ] {
            if oid.object_format() != self.action_object_format {
                return fail(path, ErrorKind::InvalidValue);
            }
        }
        if !valid_required_status_name(&self.required_status_name) {
            return fail("$.required_status_name", ErrorKind::InvalidValue);
        }
        Ok(())
    }
}

#[derive(Serialize)]
struct ConstraintView<'a> {
    action_commit_oid: &'a Oid,
    action_object_format: ObjectFormat,
    action_repository: &'a RepositoryIdentity,
    action_tree_oid: &'a Oid,
    bootstrap_contract: &'static str,
    bootstrap_digest: Digest,
    manifest_path: &'a RepoPathText,
    release_manifest_digest: Digest,
    required_status_name: &'a str,
    schema: &'static str,
    selected_platform: ConstraintPlatform,
}

impl Serialize for ExecutionConstraintDescriptor {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        ConstraintView {
            action_commit_oid: &self.input.action_commit_oid,
            action_object_format: self.input.action_object_format,
            action_repository: &self.input.action_repository,
            action_tree_oid: &self.input.action_tree_oid,
            bootstrap_contract: ACTION_BOOTSTRAP_CONTRACT,
            bootstrap_digest: self.input.bootstrap_digest,
            manifest_path: &self.input.manifest_path,
            release_manifest_digest: self.input.release_manifest_digest,
            required_status_name: &self.input.required_status_name,
            schema: EXECUTION_CONSTRAINT_SCHEMA,
            selected_platform: self.input.selected_platform,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ExecutionConstraintDescriptor {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let payload = Constraint::deserialize(codec::object(deserializer))?;
        payload.check().map_err(serde::de::Error::custom)?;
        let digest = codec::digest(EXECUTION_CONSTRAINT_SCHEMA, &payload)
            .map_err(serde::de::Error::custom)?;
        Ok(Self::from_payload(payload, digest))
    }
}
