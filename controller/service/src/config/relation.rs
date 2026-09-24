use std::path::Path;

use amiss_controller::{
    OpaqueId, PlanScope, ProviderIdentity, RegisteredRelation, RegisteredSubject, RelationLimits,
    RelationRegistry, RelationStatusDestination, relation_registry,
};
use amiss_wire::controls::{ProjectionKind, ProjectionSource};
use amiss_wire::model::Digest;
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
    identity: ArtifactId,
    context_digest: Digest,
    projection: ProjectionKind,
    subjects: [SubjectFile; 2],
    aggregate_limits: RelationLimits,
    status_destinations: Vec<RelationStatusDestination>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SubjectFile {
    role: ArtifactId,
    scope: ScopeFile,
    target: BranchRef,
    object_format: ObjectFormat,
    credential: OpaqueId,
    source: ProjectionSource,
    limits: RelationLimits,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScopeFile {
    provider: ProviderIdentity,
    integration: OpaqueId,
    repository: RepositoryFile,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RepositoryFile {
    owner: String,
    name: String,
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

fn load_relation(raw: RelationFile) -> Result<RegisteredRelation, ConfigError> {
    let [left, right] = raw.subjects;
    Ok(RegisteredRelation {
        identity: raw.identity,
        context_digest: raw.context_digest,
        projection: raw.projection,
        subjects: [load_subject(left)?, load_subject(right)?],
        aggregate_limits: raw.aggregate_limits,
        status_destinations: raw.status_destinations,
    })
}

fn load_subject(raw: SubjectFile) -> Result<RegisteredSubject, ConfigError> {
    let invalid = || ConfigError::invalid("relation subject identity is invalid");
    let provider = raw.scope.provider;
    let repository = RepositoryIdentity::new(
        provider.instance.as_str().to_owned(),
        raw.scope.repository.owner,
        raw.scope.repository.name,
    )
    .ok_or_else(invalid)?;
    Ok(RegisteredSubject {
        role: raw.role,
        scope: PlanScope {
            provider,
            integration: raw.scope.integration,
            repository,
        },
        target: raw.target,
        object_format: raw.object_format,
        credential: raw.credential,
        source: raw.source,
        limits: raw.limits,
    })
}
