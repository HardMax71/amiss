use amiss_git::{GitResources, ObjectKind, Repository, ValueCap};
use amiss_wire::controls::{GitMode, ProjectionKind, ProjectionSource, ResourceName};
use amiss_wire::digest::{sha256, sha256_stream};
use amiss_wire::model::{Oid, RepoPath};
use amiss_wire::relation::RelationProjectedValue;

use crate::discovery::{SnapshotDiscovery, WalkMode, discover_walk};
use crate::resolve::{LineRange, named_region_bytes, selected_line_bytes};
use crate::resources::crossing;
use crate::{Error, lfs};

use super::{inventory, normalized_line_endings};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RepositoryProjectionLimits {
    pub records: u64,
    pub bytes: u64,
}

pub struct RepositoryProjectionRequest<'a> {
    pub repository: &'a Repository,
    pub git: &'a mut GitResources,
    pub tree: &'a Oid,
    pub projection: ProjectionKind,
    pub source: &'a ProjectionSource,
    pub limits: RepositoryProjectionLimits,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RepositoryProjectionOutcome {
    pub value: Option<RelationProjectedValue>,
    pub records: u64,
    pub bytes: u64,
}

/// Projects one operator-selected source from an already acquired exact tree.
/// Missing, unsupported, or incomplete sources return a null value with the
/// work measured before proof stopped.
///
/// # Errors
///
/// The selector is inconsistent with its projection, a Git object cannot be
/// trusted, or the selected record/byte work crosses the supplied ceiling.
pub fn project_repository(
    mut request: RepositoryProjectionRequest<'_>,
) -> Result<RepositoryProjectionOutcome, Error> {
    amiss_wire::controls::check_projection_source(request.projection, request.source)
        .map_err(|_defect| Error::Internal)?;
    if matches!(
        request.source,
        ProjectionSource::RecordValue(_) | ProjectionSource::RecordSet(_)
    ) {
        return Ok(RepositoryProjectionOutcome {
            value: None,
            records: 0,
            bytes: 0,
        });
    }
    let discovery = discover_walk(
        request.repository,
        request.git,
        request.tree,
        WalkMode::Entries,
    )?;
    match request.source {
        ProjectionSource::BlobLines(_) | ProjectionSource::NamedRegion(_) => {
            project_blob(&mut request, &discovery)
        }
        ProjectionSource::TreePaths(selection) => project_tree(&request, &discovery, selection),
        ProjectionSource::RecordValue(_) | ProjectionSource::RecordSet(_) => Err(Error::Internal),
    }
}

fn project_blob(
    request: &mut RepositoryProjectionRequest<'_>,
    discovery: &SnapshotDiscovery,
) -> Result<RepositoryProjectionOutcome, Error> {
    let path = match request.source {
        ProjectionSource::BlobLines(selection) => RepoPath::from(&selection.path),
        ProjectionSource::NamedRegion(selection) => RepoPath::from(&selection.path),
        ProjectionSource::TreePaths(_)
        | ProjectionSource::RecordValue(_)
        | ProjectionSource::RecordSet(_) => return Err(Error::Internal),
    };
    let Some((mode, oid)) = discovery.entries.get(&path) else {
        return Ok(unavailable(0, 0));
    };
    if !matches!(mode, GitMode::RegularFile | GitMode::ExecutableFile) {
        return Ok(unavailable(0, 0));
    }
    within(
        1,
        request.limits.records,
        ResourceName::ProjectionRecordsComparedPerSnapshot,
    )?;
    let object = request.repository.read_expected_capped(
        request.git,
        oid,
        ObjectKind::Blob,
        ValueCap {
            resource: ResourceName::AggregateProjectionSelectedBytesPerSnapshot,
            limit: request.limits.bytes,
        },
    )?;
    let selected_bytes = u64::try_from(object.body.len()).unwrap_or(u64::MAX);
    within(
        selected_bytes,
        request.limits.bytes,
        ResourceName::AggregateProjectionSelectedBytesPerSnapshot,
    )?;
    if lfs::is_pointer(&object.body) {
        return Ok(unavailable(1, selected_bytes));
    }

    let selected = match request.source {
        ProjectionSource::BlobLines(selection) => selected_line_bytes(
            &object.body,
            LineRange {
                first: selection.first_line,
                last: selection.last_line,
            },
        ),
        ProjectionSource::NamedRegion(selection) => {
            named_region_bytes(&object.body, selection).ok()
        }
        ProjectionSource::TreePaths(_)
        | ProjectionSource::RecordValue(_)
        | ProjectionSource::RecordSet(_) => return Err(Error::Internal),
    };
    let Some(selected) = selected else {
        return Ok(unavailable(1, selected_bytes));
    };
    let normalized = normalized_line_endings(selected);
    let projected = normalized
        .as_ref()
        .strip_suffix(b"\n")
        .unwrap_or(normalized.as_ref());
    let projected_bytes = u64::try_from(projected.len()).unwrap_or(u64::MAX);
    within(
        projected_bytes,
        request.limits.bytes,
        ResourceName::AggregateProjectionProjectedBytesPerSnapshot,
    )?;
    Ok(RepositoryProjectionOutcome {
        value: Some(RelationProjectedValue {
            value_digest: sha256(projected),
            value_bytes: projected_bytes,
        }),
        records: 1,
        bytes: selected_bytes.max(projected_bytes),
    })
}

fn project_tree(
    request: &RepositoryProjectionRequest<'_>,
    discovery: &SnapshotDiscovery,
    selection: &amiss_wire::controls::TreePathSelection,
) -> Result<RepositoryProjectionOutcome, Error> {
    if selection_has_path_defect(discovery, selection) {
        return Ok(unavailable(0, 0));
    }
    let paths = match inventory::selected_paths(discovery, selection) {
        Ok(paths) => paths,
        Err(_reason) => return Ok(unavailable(0, 0)),
    };
    let mut records = 0_u64;
    let mut selected_bytes = 0_u64;
    let mut rows = Vec::new();
    for path in paths {
        records = records.saturating_add(1);
        within(
            records,
            request.limits.records,
            ResourceName::ProjectionRecordsComparedPerSnapshot,
        )?;
        selected_bytes =
            selected_bytes.saturating_add(u64::try_from(path.len()).unwrap_or(u64::MAX));
        within(
            selected_bytes,
            request.limits.bytes,
            ResourceName::AggregateProjectionSelectedBytesPerSnapshot,
        )?;
        let Ok(row) = std::str::from_utf8(path) else {
            return Ok(unavailable(records, selected_bytes));
        };
        if row.chars().any(char::is_control) {
            return Ok(unavailable(records, selected_bytes));
        }
        rows.push(row);
    }

    let (digest, projected_bytes) = match request.projection {
        ProjectionKind::SortedRowsV1 => {
            let projected_bytes = inventory::projected_bytes(&rows);
            let digest = sha256_stream(|write| {
                for (index, row) in rows.iter().enumerate() {
                    if index != 0 {
                        write(b"\n");
                    }
                    write(row.as_bytes());
                }
            });
            (digest, projected_bytes)
        }
        ProjectionKind::DecimalCountV1 => {
            let count = records.to_string();
            (
                sha256(count.as_bytes()),
                u64::try_from(count.len()).unwrap_or(u64::MAX),
            )
        }
        ProjectionKind::CodeTextV1 => return Err(Error::Internal),
    };
    within(
        projected_bytes,
        request.limits.bytes,
        ResourceName::AggregateProjectionProjectedBytesPerSnapshot,
    )?;
    Ok(RepositoryProjectionOutcome {
        value: Some(RelationProjectedValue {
            value_digest: digest,
            value_bytes: projected_bytes,
        }),
        records,
        bytes: selected_bytes.max(projected_bytes),
    })
}

fn selection_has_path_defect(
    discovery: &SnapshotDiscovery,
    selection: &amiss_wire::controls::TreePathSelection,
) -> bool {
    let root = selection.root.as_str().as_bytes();
    discovery.path_defects.iter().any(|defect| {
        let Some(raw) = &defect.raw else {
            return true;
        };
        raw == root
            || raw
                .strip_prefix(root)
                .is_some_and(|relative| relative.starts_with(b"/"))
    })
}

fn unavailable(records: u64, bytes: u64) -> RepositoryProjectionOutcome {
    RepositoryProjectionOutcome {
        value: None,
        records,
        bytes,
    }
}

fn within(value: u64, limit: u64, resource: ResourceName) -> Result<(), Error> {
    (value <= limit)
        .then_some(())
        .ok_or_else(|| crossing(resource, limit, value))
}
