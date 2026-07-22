use crate::de::{self, Error, ErrorKind, Obj, fail};
use crate::digest::{Digest, hj};
use crate::json::{Value, canonical};
use crate::model::{ObjectFormat, Oid, RepoPathText, RepositoryIdentity};

use super::value::{object, repository, text};
use super::{decode_digest, decode_repo_path, decode_repository, root};

const EXECUTION_CONSTRAINT_SCHEMA: &str = "amiss/scanner-execution-constraint";
const ACTION_BOOTSTRAP_CONTRACT: &str = "amiss-action-bootstrap";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConstraintPlatform {
    LinuxX8664,
    LinuxAarch64,
    MacosX8664,
    MacosAarch64,
    WindowsX8664,
    WindowsAarch64,
}

impl ConstraintPlatform {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LinuxX8664 => "linux-x86_64",
            Self::LinuxAarch64 => "linux-aarch64",
            Self::MacosX8664 => "macos-x86_64",
            Self::MacosAarch64 => "macos-aarch64",
            Self::WindowsX8664 => "windows-x86_64",
            Self::WindowsAarch64 => "windows-aarch64",
        }
    }

    /// # Errors
    ///
    /// A value outside the closed six-platform table.
    pub fn decode(path: &str, value: Value) -> Result<Self, Error> {
        match de::string(path, value)?.as_str() {
            "linux-x86_64" => Ok(Self::LinuxX8664),
            "linux-aarch64" => Ok(Self::LinuxAarch64),
            "macos-x86_64" => Ok(Self::MacosX8664),
            "macos-aarch64" => Ok(Self::MacosAarch64),
            "windows-x86_64" => Ok(Self::WindowsX8664),
            "windows-aarch64" => Ok(Self::WindowsAarch64),
            _ => fail(path, ErrorKind::InvalidValue),
        }
    }
}

/// The externally protected allow-list entry for one scanner action tree,
/// release manifest, bootstrap contract, and required provider status name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionConstraintDescriptor {
    pub digest: Digest,
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

fn decode_status_name(path: &str, value: Value) -> Result<String, Error> {
    let raw = de::string(path, value)?;
    if valid_required_status_name(&raw) {
        Ok(raw)
    } else {
        fail(path, ErrorKind::InvalidValue)
    }
}

#[must_use]
pub fn valid_required_status_name(raw: &str) -> bool {
    let bytes = raw.as_bytes();
    let interior = |byte: &u8| {
        byte.is_ascii_alphanumeric() || matches!(byte, b' ' | b'.' | b'_' | b'/' | b'-')
    };
    let edge =
        |byte: &u8| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'/' | b'-');
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
        Self::parse(&canonical(&execution_constraint_value(input)))
    }

    /// # Errors
    ///
    /// Fails on strict-JSON defects, schema-shape violations, and invalid
    /// grammar values.
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        let value = root(bytes)?;
        let digest = hj(EXECUTION_CONSTRAINT_SCHEMA, &value);
        let mut obj = Obj::new("$", value)?;
        de::const_str(
            &obj.field("schema"),
            obj.take("schema")?,
            EXECUTION_CONSTRAINT_SCHEMA,
        )?;
        let action_repository = decode_repository(
            &obj.field("action_repository"),
            obj.take("action_repository")?,
        )?;
        let format_path = obj.field("action_object_format");
        let action_object_format =
            match de::string(&format_path, obj.take("action_object_format")?)?.as_str() {
                "sha1" => ObjectFormat::Sha1,
                "sha256" => ObjectFormat::Sha256,
                _ => return fail(&format_path, ErrorKind::InvalidValue),
            };
        let commit_path = obj.field("action_commit_oid");
        let action_commit_oid = Oid::new(
            action_object_format,
            de::string(&commit_path, obj.take("action_commit_oid")?)?,
        )
        .ok_or_else(|| Error::new(&commit_path, ErrorKind::InvalidValue))?;
        let tree_path = obj.field("action_tree_oid");
        let action_tree_oid = Oid::new(
            action_object_format,
            de::string(&tree_path, obj.take("action_tree_oid")?)?,
        )
        .ok_or_else(|| Error::new(&tree_path, ErrorKind::InvalidValue))?;
        let manifest_path =
            decode_repo_path(&obj.field("manifest_path"), obj.take("manifest_path")?)?;
        let release_manifest_digest = decode_digest(
            &obj.field("release_manifest_digest"),
            obj.take("release_manifest_digest")?,
        )?;
        let selected_platform = ConstraintPlatform::decode(
            &obj.field("selected_platform"),
            obj.take("selected_platform")?,
        )?;
        let required_status_name = decode_status_name(
            &obj.field("required_status_name"),
            obj.take("required_status_name")?,
        )?;
        de::const_str(
            &obj.field("bootstrap_contract"),
            obj.take("bootstrap_contract")?,
            ACTION_BOOTSTRAP_CONTRACT,
        )?;
        let bootstrap_digest = decode_digest(
            &obj.field("bootstrap_digest"),
            obj.take("bootstrap_digest")?,
        )?;
        obj.finish()?;
        Ok(Self {
            digest,
            action_repository,
            action_object_format,
            action_commit_oid,
            action_tree_oid,
            manifest_path,
            release_manifest_digest,
            selected_platform,
            required_status_name,
            bootstrap_digest,
        })
    }

    /// Serializes one valid descriptor to its unique canonical JSON bytes.
    ///
    /// # Errors
    ///
    /// A public field was changed into a value [`Self::parse`] rejects, or
    /// changed without replacing the derived `digest`.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, Error> {
        let bytes = canonical(&execution_constraint_value(self.into()));
        let parsed = Self::parse(&bytes)?;
        if parsed.digest != self.digest {
            return fail("$.digest", ErrorKind::DigestMismatch);
        }
        Ok(bytes)
    }
}

impl From<&ExecutionConstraintDescriptor> for ExecutionConstraintInput {
    fn from(descriptor: &ExecutionConstraintDescriptor) -> Self {
        Self {
            action_repository: descriptor.action_repository.clone(),
            action_object_format: descriptor.action_object_format,
            action_commit_oid: descriptor.action_commit_oid.clone(),
            action_tree_oid: descriptor.action_tree_oid.clone(),
            manifest_path: descriptor.manifest_path.clone(),
            release_manifest_digest: descriptor.release_manifest_digest,
            selected_platform: descriptor.selected_platform,
            required_status_name: descriptor.required_status_name.clone(),
            bootstrap_digest: descriptor.bootstrap_digest,
        }
    }
}

fn execution_constraint_value(input: ExecutionConstraintInput) -> Value {
    let ExecutionConstraintInput {
        action_repository,
        action_object_format,
        action_commit_oid,
        action_tree_oid,
        manifest_path,
        release_manifest_digest,
        selected_platform,
        required_status_name,
        bootstrap_digest,
    } = input;
    object(vec![
        ("schema", text(EXECUTION_CONSTRAINT_SCHEMA)),
        ("action_repository", repository(&action_repository)),
        ("action_object_format", text(action_object_format.as_str())),
        ("action_commit_oid", text(action_commit_oid.as_str())),
        ("action_tree_oid", text(action_tree_oid.as_str())),
        ("manifest_path", text(manifest_path.as_str())),
        (
            "release_manifest_digest",
            text(&release_manifest_digest.to_string()),
        ),
        ("selected_platform", text(selected_platform.as_str())),
        ("required_status_name", Value::String(required_status_name)),
        ("bootstrap_contract", text(ACTION_BOOTSTRAP_CONTRACT)),
        ("bootstrap_digest", text(&bootstrap_digest.to_string())),
    ])
}
