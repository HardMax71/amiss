use sha2::Digest as _;
use std::path::{Path, PathBuf};

use amiss_controller::{
    BootstrapJobError, CheckPlan, ExternalPolicy, INTERSPHINX_INVENTORY_BYTES,
    IntersphinxInventory, OpaqueId, PolicyControls, ProviderIdentity, SemanticEvidenceExpectation,
    WorkflowArtifactExpectation, check_plan, intersphinx_evidence,
};
use amiss_wire::controls::{
    Profile, parse_debt_snapshot, parse_execution_constraint, parse_organization_floor,
    parse_waiver_bundle,
};
use amiss_wire::model::Digest;
use amiss_wire::model::{ArtifactId, RepoPathText, RepositoryIdentity};
use amiss_wire::requests::{REQUEST_STREAM_BYTES, RequestTrust, SuppliedControl};
use serde::Deserialize;

use super::{ConfigError, read_regular};

mod tests;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckPlanFiles {
    profile: Profile,
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
    workflow_identity: OpaqueId,
    event: OpaqueId,
    artifact_name: String,
    payload_file: RepoPathText,
    archive_byte_limit: u64,
    file_byte_limit: u64,
    semantic: SemanticEvidenceFile,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SemanticEvidenceFile {
    acquisition_identity: ArtifactId,
    producer_kind: amiss_wire::semantic::SemanticProducerKind,
    producer_identity: ArtifactId,
    producer_version: String,
    context_digest: Digest,
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
    let profile = match raw.profile {
        Profile::Observe => Profile::Observe,
        Profile::Enforce => Profile::Enforce,
        Profile::EnforceIntroduced => {
            return Err(ConfigError::invalid("profile must be observe or enforce"));
        }
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
        organization_floor: load_control(
            raw.organization_floor_file.as_deref(),
            parse_organization_floor,
            amiss_wire::controls::ORGANIZATION_FLOOR_SCHEMA,
            BootstrapJobError::OrganizationFloor,
        )?,
        debt_snapshot: load_control(
            raw.debt_snapshot_file.as_deref(),
            parse_debt_snapshot,
            amiss_wire::controls::DEBT_SNAPSHOT_SCHEMA,
            BootstrapJobError::DebtSnapshot,
        )?,
        waiver_bundle: load_control(
            raw.waiver_bundle_file.as_deref(),
            parse_waiver_bundle,
            amiss_wire::controls::WAIVER_BUNDLE_SCHEMA,
            BootstrapJobError::WaiverBundle,
        )?,
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
            Ok(WorkflowArtifactExpectation {
                provider: provider.clone(),
                repository: repository.clone(),
                workflow_identity: file.workflow_identity.clone(),
                event: file.event.clone(),
                artifact_name: file.artifact_name.clone(),
                payload_file: file.payload_file.clone(),
                archive_byte_limit: file.archive_byte_limit,
                file_byte_limit: file.file_byte_limit,
                semantic: SemanticEvidenceExpectation {
                    acquisition_identity: file.semantic.acquisition_identity.clone(),
                    producer_kind: file.semantic.producer_kind,
                    producer_identity: file.semantic.producer_identity.clone(),
                    producer_version: file.semantic.producer_version.clone(),
                    context_digest: file.semantic.context_digest,
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

fn load_control<T: serde::Serialize, E>(
    path: Option<&Path>,
    parse: impl FnOnce(&[u8]) -> Result<T, E>,
    domain: &str,
    error: BootstrapJobError,
) -> Result<Option<SuppliedControl<T>>, ConfigError> {
    path.map(|path| {
        let bytes = read_regular(path, REQUEST_STREAM_BYTES)?;
        let invalid = |_defect| ConfigError::caused_by("check plan is invalid", error);
        let value = parse(&bytes).map_err(invalid)?;
        let mut writer =
            digest_io::IoWrapper(sha2::Sha256::new_with_prefix(domain).chain_update([0_u8]));
        serde_json_canonicalizer::to_writer(&value, &mut writer)
            .map_err(|_defect| ConfigError::caused_by("check plan is invalid", error))?;
        let expected_digest = Digest::from(writer.0.finalize().0);
        Ok(SuppliedControl {
            value,
            expected_digest,
            trust_source: RequestTrust::OrganizationPolicy,
        })
    })
    .transpose()
}
