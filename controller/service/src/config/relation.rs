use std::path::Path;

use amiss_controller::{
    IntegrationId, OpaqueId, PlanScope, ProviderIdentity, RelationLimits, RelationPlan,
    RelationRegistry, RelationStatusDestination, RelationSubject, relation_registry,
};
use amiss_wire::controls::{ProjectionKind, parse_projection_source};
use amiss_wire::digest::Digest;
use amiss_wire::model::{ArtifactId, BranchRef, ObjectFormat, RepositoryIdentity};
use amiss_wire::requests::REQUEST_STREAM_BYTES;
use serde::Deserialize;

use super::{ConfigError, read_regular};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RegistryFile {
    relations: Vec<RelationFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RelationFile {
    identity: String,
    context_digest: String,
    projection: String,
    subjects: [SubjectFile; 2],
    aggregate_limits: RelationLimits,
    status_destinations: Vec<DestinationFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SubjectFile {
    role: String,
    scope: ScopeFile,
    target: String,
    object_format: String,
    credential: String,
    source: serde_json::Value,
    limits: RelationLimits,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScopeFile {
    provider: ProviderFile,
    integration: String,
    repository: RepositoryFile,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProviderFile {
    namespace: String,
    instance: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RepositoryFile {
    owner: String,
    name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DestinationFile {
    subject_role: String,
    required_status_name: String,
}

/// Loads one bounded operator-owned relation file and atomically freezes its complete registry.
///
/// Repository hosts are derived from their provider instances rather than accepted as a second
/// spelling. The file contains opaque credential identities only; this boundary reads no secrets
/// and performs no provider I/O.
///
/// # Errors
///
/// The path is not a bounded regular file, the JSON shape or a typed field is invalid, or the
/// complete registry violates a relation identity, projection, limit, or destination law.
pub fn load_relation_registry(path: &Path) -> Result<RelationRegistry, ConfigError> {
    let bytes = read_regular(path, REQUEST_STREAM_BYTES)?;
    amiss_wire::json::parse(&bytes)
        .map_err(|defect| ConfigError::caused_by("relation registry is not strict JSON", defect))?;
    let raw: RegistryFile = serde_json::from_slice(&bytes)
        .map_err(|defect| ConfigError::caused_by("relation registry is not strict JSON", defect))?;
    let plans = raw
        .relations
        .into_iter()
        .map(load_relation)
        .collect::<Result<Vec<_>, _>>()?;
    relation_registry(plans)
        .map_err(|defect| ConfigError::caused_by("relation registry is invalid", defect))
}

fn load_relation(raw: RelationFile) -> Result<RelationPlan, ConfigError> {
    let invalid = || ConfigError::invalid("relation identity or projection is invalid");
    let projection = raw
        .projection
        .parse::<ProjectionKind>()
        .map_err(|_defect| invalid())?;
    let [left, right] = raw.subjects;
    Ok(RelationPlan {
        identity: ArtifactId::new(raw.identity).ok_or_else(invalid)?,
        context_digest: Digest::from_wire(&raw.context_digest).ok_or_else(invalid)?,
        projection,
        subjects: [
            load_subject(left, projection)?,
            load_subject(right, projection)?,
        ],
        aggregate_limits: raw.aggregate_limits,
        status_destinations: raw
            .status_destinations
            .into_iter()
            .map(|destination| {
                Ok(RelationStatusDestination {
                    subject_role: ArtifactId::new(destination.subject_role).ok_or_else(invalid)?,
                    required_status_name: destination.required_status_name,
                })
            })
            .collect::<Result<Vec<_>, ConfigError>>()?,
    })
}

fn load_subject(
    raw: SubjectFile,
    projection: ProjectionKind,
) -> Result<RelationSubject, ConfigError> {
    let invalid = || ConfigError::invalid("relation subject identity is invalid");
    let provider = ProviderIdentity::new(raw.scope.provider.namespace, raw.scope.provider.instance)
        .ok_or_else(invalid)?;
    let repository = RepositoryIdentity::new(
        provider.instance.as_str().to_owned(),
        raw.scope.repository.owner,
        raw.scope.repository.name,
    )
    .ok_or_else(invalid)?;
    let source = serde_json::to_vec(&raw.source).map_err(|defect| {
        ConfigError::caused_by("relation projection source is invalid", defect)
    })?;
    let source = parse_projection_source(&source, projection).map_err(|defect| {
        ConfigError::caused_by("relation projection source is invalid", defect)
    })?;
    Ok(RelationSubject {
        role: ArtifactId::new(raw.role).ok_or_else(invalid)?,
        scope: PlanScope {
            provider,
            integration: IntegrationId::new(raw.scope.integration).ok_or_else(invalid)?,
            repository,
        },
        target: BranchRef::new(raw.target).ok_or_else(invalid)?,
        object_format: raw
            .object_format
            .parse::<ObjectFormat>()
            .map_err(|_defect| invalid())?,
        credential: OpaqueId::new(raw.credential).ok_or_else(invalid)?,
        source,
        limits: raw.limits,
    })
}
