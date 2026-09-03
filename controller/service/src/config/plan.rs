use std::path::{Path, PathBuf};

use amiss_controller::{
    AcquiredControl, CheckPlan, ExternalPolicy, INTERSPHINX_INVENTORY_BYTES, IntersphinxInventory,
    OpaqueId, PolicyControls, ProviderIdentity, SemanticEvidenceExpectation,
    WorkflowArtifactExpectation, check_plan, intersphinx_evidence,
};
use amiss_wire::controls::{Profile, parse_execution_constraint};
use amiss_wire::digest::Digest;
use amiss_wire::model::{ArtifactId, RepoPathText, RepositoryIdentity};
use amiss_wire::requests::{REQUEST_STREAM_BYTES, RequestTrust};
use serde::Deserialize;

use super::{ConfigError, read_regular};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckPlanFiles {
    profile: String,
    #[serde(default)]
    external_policy: ExternalPolicy,
    execution_constraint_file: PathBuf,
    organization_floor_file: Option<PathBuf>,
    debt_snapshot_file: Option<PathBuf>,
    waiver_bundle_file: Option<PathBuf>,
    #[serde(default)]
    intersphinx_inventories: Vec<IntersphinxInventoryFile>,
    #[serde(default)]
    workflow_artifacts: Vec<WorkflowArtifactFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IntersphinxInventoryFile {
    identity: String,
    base_url: String,
    file: PathBuf,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkflowArtifactFile {
    workflow_identity: String,
    event: String,
    artifact_name: String,
    payload_file: String,
    archive_byte_limit: u64,
    file_byte_limit: u64,
    semantic: SemanticEvidenceFile,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SemanticEvidenceFile {
    acquisition_identity: String,
    producer_kind: String,
    producer_identity: String,
    producer_version: String,
    context_digest: String,
}

/// Loads and binds every trust input named by one service plan. A provider lane supplies its
/// workflow scope only when it implements that acquisition.
///
/// # Errors
///
/// A profile, trust file, workflow artifact, execution constraint, or resulting plan is invalid.
pub fn load_plan(
    raw: &CheckPlanFiles,
    workflow_scope: Option<(&ProviderIdentity, &RepositoryIdentity)>,
) -> Result<CheckPlan, ConfigError> {
    let profile = match raw.profile.as_str() {
        "observe" => Profile::Observe,
        "enforce" => Profile::Enforce,
        _ => return Err(ConfigError::invalid("profile must be observe or enforce")),
    };
    let execution_bytes = read_regular(&raw.execution_constraint_file, REQUEST_STREAM_BYTES)?;
    let execution = parse_execution_constraint(&execution_bytes)
        .map_err(|defect| ConfigError::caused_by("execution constraint is invalid", defect))?;
    let semantic_evidence = intersphinx_evidence(load_intersphinx(&raw.intersphinx_inventories)?)
        .map_err(|defect| {
        ConfigError::caused_by("Intersphinx inventory configuration is invalid", defect)
    })?;
    let workflow_artifacts = load_workflow_artifacts(&raw.workflow_artifacts, workflow_scope)?;
    let policy = PolicyControls {
        external_policy: raw.external_policy,
        organization_floor: load_control(raw.organization_floor_file.as_deref())?,
        debt_snapshot: load_control(raw.debt_snapshot_file.as_deref())?,
        waiver_bundle: load_control(raw.waiver_bundle_file.as_deref())?,
        semantic_evidence,
        semantic_acquisitions: Vec::new(),
        workflow_artifacts,
    };
    check_plan(profile, policy, execution)
        .map_err(|defect| ConfigError::caused_by("check plan is invalid", defect))
}

fn load_workflow_artifacts(
    files: &[WorkflowArtifactFile],
    scope: Option<(&ProviderIdentity, &RepositoryIdentity)>,
) -> Result<Vec<WorkflowArtifactExpectation>, ConfigError> {
    if files.is_empty() {
        return Ok(Vec::new());
    }
    let (provider, repository) = scope.ok_or(ConfigError::invalid(
        "workflow artifacts are unsupported by this provider lane",
    ))?;
    files
        .iter()
        .map(|file| {
            let invalid = || ConfigError::invalid("workflow artifact configuration is invalid");
            Ok(WorkflowArtifactExpectation {
                provider: provider.clone(),
                repository: repository.clone(),
                workflow_identity: OpaqueId::new(file.workflow_identity.clone())
                    .ok_or_else(invalid)?,
                event: OpaqueId::new(file.event.clone()).ok_or_else(invalid)?,
                artifact_name: file.artifact_name.clone(),
                payload_file: RepoPathText::new(file.payload_file.clone()).ok_or_else(invalid)?,
                archive_byte_limit: file.archive_byte_limit,
                file_byte_limit: file.file_byte_limit,
                semantic: SemanticEvidenceExpectation {
                    acquisition_identity: ArtifactId::new(
                        file.semantic.acquisition_identity.clone(),
                    )
                    .ok_or_else(invalid)?,
                    producer_kind: ArtifactId::new(file.semantic.producer_kind.clone())
                        .ok_or_else(invalid)?,
                    producer_identity: ArtifactId::new(file.semantic.producer_identity.clone())
                        .ok_or_else(invalid)?,
                    producer_version: file.semantic.producer_version.clone(),
                    context_digest: Digest::from_wire(&file.semantic.context_digest)
                        .ok_or_else(invalid)?,
                },
            })
        })
        .collect()
}

fn load_intersphinx(
    inventories: &[IntersphinxInventoryFile],
) -> Result<Vec<IntersphinxInventory>, ConfigError> {
    inventories
        .iter()
        .try_fold(
            (
                Vec::with_capacity(inventories.len()),
                INTERSPHINX_INVENTORY_BYTES,
            ),
            |(mut loaded, remaining), inventory| {
                let bytes = read_regular(&inventory.file, remaining)?;
                let length = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
                let remaining = remaining.checked_sub(length).ok_or(ConfigError::invalid(
                    "Intersphinx inventory files exceed their byte ceiling",
                ))?;
                loaded.push(IntersphinxInventory {
                    identity: inventory.identity.clone(),
                    base_url: inventory.base_url.clone(),
                    bytes,
                });
                Ok((loaded, remaining))
            },
        )
        .map(|(loaded, _remaining)| loaded)
}

fn load_control(path: Option<&Path>) -> Result<Option<AcquiredControl>, ConfigError> {
    path.map(|path| {
        read_regular(path, REQUEST_STREAM_BYTES).map(|bytes| AcquiredControl {
            bytes,
            trust_source: RequestTrust::OrganizationPolicy,
        })
    })
    .transpose()
}
