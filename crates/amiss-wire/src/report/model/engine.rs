use serde::{Deserialize, Serialize};

use crate::controls::ConstraintPlatform;
use crate::digest::Digest;
use crate::manifest::ReleaseManifest;
use crate::model::{Adapter, ObjectFormat, Oid, RepoPathText, RepositoryIdentity};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdapterContractSchema {
    #[serde(rename = "amiss/scanner-adapter-contract")]
    Current,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FrontmatterContract {
    Frontmatter,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceProjection {
    None,
    SourceProjection,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StructuralAddressKind {
    AsciidocBlockPath,
    MarkdownAstNodePath,
    MdxAstNodePath,
    None,
    RstBlockPath,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterContractDescriptor {
    pub adapter_id: Adapter,
    pub frontmatter_contract: FrontmatterContract,
    pub grammar_profile: String,
    pub parser_name: String,
    pub parser_version: String,
    pub schema: AdapterContractSchema,
    pub source_projection: SourceProjection,
    pub structural_address: StructuralAddressKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportAdapter {
    pub adapter_id: Adapter,
    pub contract_descriptor: AdapterContractDescriptor,
    pub contract_digest: Digest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LocalActionKind {
    #[serde(rename = "local")]
    Local,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalActionProvenance {
    pub kind: LocalActionKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ForgeActionKind {
    #[serde(rename = "forge-action")]
    ForgeAction,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForgeActionProvenance {
    pub action_commit_oid: Oid,
    pub action_object_format: ObjectFormat,
    pub action_repository: RepositoryIdentity,
    pub action_tree_oid: Oid,
    pub dependency_lock_digest: Digest,
    pub kind: ForgeActionKind,
    pub manifest_path: RepoPathText,
    pub release_manifest: ReleaseManifest,
    pub release_manifest_digest: Digest,
    pub selected_artifact_name: String,
    pub selected_platform: ConstraintPlatform,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ActionProvenance {
    ForgeAction(Box<ForgeActionProvenance>),
    Local(LocalActionProvenance),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EngineContract {
    #[serde(rename = "amiss/scanner")]
    Current,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuiltInPolicy {
    #[serde(rename = "scanner-policy-defaults")]
    Current,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Engine {
    pub action_provenance: ActionProvenance,
    pub adapters: Vec<ReportAdapter>,
    pub built_in_policy: BuiltInPolicy,
    pub engine_contract: EngineContract,
    pub engine_digest: Digest,
    pub engine_version: String,
}
