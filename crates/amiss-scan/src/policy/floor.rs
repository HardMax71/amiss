use std::collections::BTreeMap;

use amiss_git::{GitResources, ObjectKind, Repository, ValueCap};
use amiss_wire::controls::{GitMode, ResourceName};
use amiss_wire::digest::Digest;
use amiss_wire::model::{Oid, RepoPath, RepoPathText};
use amiss_wire::report::{Disposition, FindingKind};

use super::FloorInput;
use super::effects::{ControlSeed, InventoryState, disposition_rows};
use crate::resources::{Aggregate, ScanResources};
use crate::{Error, lfs};

/// One protected control path's state on one side: absent, present with its
/// exact protected-control-evidence digest, or unsupported because the entry
/// is a tree, symlink, gitlink, or recognized LFS pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProtectedState {
    Absent,
    Unsupported,
    Present(Digest),
}

pub const PROTECTED_CONTROL_EVIDENCE_DOMAIN: &str = "amiss/scanner-protected-control-evidence";

#[derive(serde::Serialize)]
struct ProtectedControlEvidence<'a> {
    git_mode: GitMode,
    path: &'a str,
    raw_digest: Digest,
}

/// Reads one protected control path's state from a snapshot: the evidence
/// digest binds path, mode, and raw digest; the blob is size-checked under
/// the selected-control resources and never parsed or executed.
///
/// # Errors
///
/// Snapshot-level acquisition defects and control byte crossings.
pub fn protected_state(
    repo: &Repository,
    git: &mut GitResources,
    scan: &mut ScanResources,
    entries: &BTreeMap<RepoPath, (GitMode, Oid)>,
    path: &str,
) -> Result<ProtectedState, Error> {
    let Some((mode, oid)) = entries.get(path.as_bytes()) else {
        return Ok(ProtectedState::Absent);
    };
    match mode {
        GitMode::Tree | GitMode::Gitlink | GitMode::Symlink => {
            return Ok(ProtectedState::Unsupported);
        }
        GitMode::RegularFile | GitMode::ExecutableFile => {}
    }
    let cap = ValueCap {
        resource: ResourceName::SelectedControlBlobBytes,
        limit: scan.limits().selected_control_blob_bytes,
    };
    let object = repo
        .read_expected_capped(git, oid, ObjectKind::Blob, cap)
        .map_err(Error::from)?;
    scan.charge(
        Aggregate::SelectedControlBytes,
        u64::try_from(object.body.len()).unwrap_or(u64::MAX),
    )?;
    if lfs::is_pointer(&object.body) {
        return Ok(ProtectedState::Unsupported);
    }
    let raw = amiss_wire::digest::hb(crate::resolve::RAW_EVIDENCE_DOMAIN, &object.body);
    let descriptor = ProtectedControlEvidence {
        git_mode: *mode,
        path,
        raw_digest: raw,
    };
    amiss_wire::digest::hj_serde(PROTECTED_CONTROL_EVIDENCE_DOMAIN, |mut writer| {
        serde_json_canonicalizer::to_writer(&descriptor, &mut writer)
    })
    .map(ProtectedState::Present)
    .map_err(|_defect| Error::Internal)
}

/// The floor inventory obligation over the candidate: every protected
/// inventory path that is not a scanned candidate document emits its exact
/// floor coverage rule.
#[must_use]
pub fn floor_inventory(
    input: &FloorInput,
    candidate_documents: &dyn Fn(&str) -> InventoryState,
) -> Vec<ControlSeed> {
    let mut controls = Vec::new();
    for path in &input.floor.protected_inventory {
        let rule = match candidate_documents(path.as_str()) {
            InventoryState::Scanned => continue,
            InventoryState::Missing => "coverage/floor-inventory-missing",
            InventoryState::Unsupported => "coverage/floor-inventory-unsupported",
            InventoryState::Outside => "coverage/floor-inventory-outside",
        };
        controls.push(ControlSeed {
            kind: FindingKind::CoverageReduced,
            rule_id: rule.to_owned(),
            control_path: Some(RepoPath::from(path)),
        });
    }
    controls
}

pub(crate) fn protected_control(
    path: &RepoPathText,
    (base, candidate): (ProtectedState, ProtectedState),
) -> Option<ControlSeed> {
    (!matches!(
        (base, candidate),
        (ProtectedState::Present(left), ProtectedState::Present(right)) if left == right
    ))
    .then(|| ControlSeed {
        kind: FindingKind::ControlPlaneChanged,
        rule_id: "control/protected-path".to_owned(),
        control_path: Some(RepoPath::from(path)),
    })
}

/// The floor's raise-only disposition rows, applied after the repository
/// steps in the policy trace.
#[must_use]
pub fn floor_raises(input: &FloorInput) -> Vec<(FindingKind, Disposition)> {
    disposition_rows(&input.floor.minimum_dispositions)
}

/// A floor may only tighten built-in limits, never raise them; unmapped
/// resources belong to layers the local scanner does not own.
#[must_use]
pub fn tightened_limits(
    scan: crate::resources::ScanLimits,
    git: amiss_git::GitLimits,
    floor: &amiss_wire::controls::OrganizationFloor,
) -> (crate::resources::ScanLimits, amiss_git::GitLimits) {
    let mut scan = scan;
    let mut git = git;
    for row in &floor.resource_limits {
        let maximum = u64::try_from(row.maximum).unwrap_or(u64::MAX);
        let slot: Option<&mut u64> = match row.resource {
            ResourceName::DocumentsPerSnapshot => Some(&mut scan.documents_per_snapshot),
            ResourceName::DocumentBlobBytes => Some(&mut scan.document_blob_bytes),
            ResourceName::AggregateDocumentBytesPerSnapshot => {
                Some(&mut scan.aggregate_document_bytes_per_snapshot)
            }
            ResourceName::RawLinkDestinationBytes => Some(&mut scan.raw_link_destination_bytes),
            ResourceName::ParserNesting => Some(&mut scan.parser_nesting),
            ResourceName::ParserNodesPerDocument => Some(&mut scan.parser_nodes_per_document),
            ResourceName::ParserNodesPerSnapshot => Some(&mut scan.parser_nodes_per_snapshot),
            ResourceName::AggregateEmbeddedCodeEvaluationBytesPerSnapshot => {
                Some(&mut scan.aggregate_embedded_code_evaluation_bytes_per_snapshot)
            }
            ResourceName::ReferencesPerDocument => Some(&mut scan.references_per_document),
            ResourceName::ReferencesPerSnapshot => Some(&mut scan.references_per_snapshot),
            ResourceName::DeclaredLabelsPerSnapshot => Some(&mut scan.declared_labels_per_snapshot),
            ResourceName::ReferencedTargetBlobBytes => Some(&mut scan.referenced_target_blob_bytes),
            ResourceName::AggregateReferencedTargetBytesPerSnapshot => {
                Some(&mut scan.aggregate_referenced_target_bytes_per_snapshot)
            }
            ResourceName::IgnoreDeclarationBlobBytes => {
                Some(&mut scan.ignore_declaration_blob_bytes)
            }
            ResourceName::AggregateIgnoreDeclarationBytesPerSnapshot => {
                Some(&mut scan.aggregate_ignore_declaration_bytes_per_snapshot)
            }
            ResourceName::AggregateLineFragmentEvaluationBytesPerSnapshot => {
                Some(&mut scan.aggregate_line_fragment_evaluation_bytes_per_snapshot)
            }
            ResourceName::AggregateHeadingAnchorEvaluationBytesPerSnapshot => {
                Some(&mut scan.aggregate_heading_anchor_evaluation_bytes_per_snapshot)
            }
            ResourceName::ProjectionAssertionsPerSnapshot => {
                Some(&mut scan.projection_assertions_per_snapshot)
            }
            ResourceName::AggregateProjectionSelectedBytesPerSnapshot => {
                Some(&mut scan.aggregate_projection_selected_bytes_per_snapshot)
            }
            ResourceName::ProjectionRecordsComparedPerSnapshot => {
                Some(&mut scan.projection_records_compared_per_snapshot)
            }
            ResourceName::AggregateProjectionProjectedBytesPerSnapshot => {
                Some(&mut scan.aggregate_projection_projected_bytes_per_snapshot)
            }
            ResourceName::AggregateProjectionPreviewBytesPerSnapshot => {
                Some(&mut scan.aggregate_projection_preview_bytes_per_snapshot)
            }
            ResourceName::SelectedControlBlobBytes => Some(&mut scan.selected_control_blob_bytes),
            ResourceName::AggregateSelectedControlBytesPerSnapshot => {
                Some(&mut scan.aggregate_selected_control_bytes_per_snapshot)
            }
            ResourceName::GitObjectBytes => Some(&mut git.inflated_object_bytes),
            ResourceName::GitCompressedObjectBytes => Some(&mut git.compressed_stream_bytes),
            ResourceName::AggregateGitCompressedObjectBytesPerEvaluation => {
                Some(&mut git.aggregate_compressed_bytes)
            }
            ResourceName::GitPackDirectoryEntries => Some(&mut git.pack_directory_entries),
            ResourceName::GitPackFiles => Some(&mut git.pack_files),
            ResourceName::GitPackIndexBytes => Some(&mut git.pack_index_bytes),
            ResourceName::AggregateGitPackIndexBytes => Some(&mut git.aggregate_pack_index_bytes),
            ResourceName::GitDeltaDepth => Some(&mut git.delta_depth),
            ResourceName::GitIndexBytes => Some(&mut git.index_bytes),
            ResourceName::GitTreeEntriesPerSnapshot => Some(&mut git.tree_entries_per_snapshot),
            ResourceName::RawPathBytes => Some(&mut git.raw_path_bytes),
            ResourceName::ControlInputBytes => Some(&mut scan.control_input_bytes),
            ResourceName::RepositoryPolicyEntries => Some(&mut scan.repository_policy_entries),
            ResourceName::DebtItems => Some(&mut scan.debt_items),
            ResourceName::WaiverItems => Some(&mut scan.waiver_items),
            ResourceName::TypedAnalysisErrorsRetained => Some(&mut scan.errors_retained),
            ResourceName::CompleteFindings => Some(&mut scan.complete_findings),
            ResourceName::OrganizationPolicyEntries
            | ResourceName::MachineJsonBytes
            | ResourceName::PrivateTemporaryStorageBytes
            | ResourceName::EvaluatorManagedMemoryBytes => None,
        };
        if let Some(limit) = slot {
            *limit = (*limit).min(maximum);
        }
    }
    (scan, git)
}
