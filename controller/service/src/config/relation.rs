use std::path::Path;

use amiss_controller::{
    IntegrationId, OpaqueId, PlanScope, ProviderIdentity, RelationLimits, RelationPlan,
    RelationRegistry, RelationStatusDestination, RelationSubject, relation_registry,
};
use amiss_wire::controls::{ProjectionKind, ProjectionSource};
use amiss_wire::digest::Digest;
use amiss_wire::model::{ArtifactId, BranchRef, ObjectFormat, RepositoryIdentity};
use amiss_wire::requests::REQUEST_STREAM_BYTES;
use serde::{Deserialize, Serialize};

use super::{ConfigError, read_regular};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RegistryFile {
    relations: Vec<RelationFile>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RelationFile {
    identity: ArtifactId,
    context_digest: Digest,
    projection: ProjectionKind,
    subjects: [SubjectFile; 2],
    aggregate_limits: RelationLimits,
    status_destinations: Vec<RelationStatusDestination>,
}

#[derive(Serialize, Deserialize)]
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

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ScopeFile {
    provider: ProviderIdentity,
    integration: IntegrationId,
    repository: RepositoryFile,
}

#[derive(Serialize, Deserialize)]
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
    let (raw, _digest): (RegistryFile, _) =
        amiss_wire::de::deserialize_json(&bytes, "amiss/relation-registry").map_err(|defect| {
            ConfigError::caused_by("relation registry is not strict JSON", defect)
        })?;
    let plans = raw
        .relations
        .into_iter()
        .map(|raw| {
            let [left, right] = raw.subjects;
            Ok(RelationPlan {
                identity: raw.identity,
                context_digest: raw.context_digest,
                projection: raw.projection,
                subjects: [load_subject(left)?, load_subject(right)?],
                aggregate_limits: raw.aggregate_limits,
                status_destinations: raw.status_destinations,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    relation_registry(plans)
        .map_err(|defect| ConfigError::caused_by("relation registry is invalid", defect))
}

fn load_subject(raw: SubjectFile) -> Result<RelationSubject, ConfigError> {
    let repository = RepositoryIdentity::new(
        raw.scope.provider.instance.as_str().to_owned(),
        raw.scope.repository.owner,
        raw.scope.repository.name,
    )
    .ok_or_else(|| ConfigError::invalid("relation subject identity is invalid"))?;
    Ok(RelationSubject {
        role: raw.role,
        scope: PlanScope {
            provider: raw.scope.provider,
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
