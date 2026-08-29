mod tests;

use std::io::{Cursor, Read as _};
use std::sync::Arc;

use amiss_controller::{
    AcquiredSemanticTemplate, MAX_WORKFLOW_ARTIFACT_ARCHIVE_BYTES,
    MAX_WORKFLOW_ARTIFACT_FILE_BYTES, SemanticEvidenceExpectation, WorkflowArtifactExpectation,
};
use zip::ZipArchive;
use zip::read::{ArchiveOffset, Config};

const EOCD_BYTES: usize = 22;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum GitHubArtifactError {
    #[error("the workflow artifact expectation is invalid for GitHub")]
    Expectation,
    #[error("the workflow artifact archive exceeds its planned byte limit")]
    ArchiveBytes,
    #[error("the workflow artifact is not one exact regular ZIP member")]
    Archive,
    #[error("the workflow artifact payload exceeds its planned byte limit")]
    PayloadBytes,
    #[error("the workflow artifact payload does not match its planned semantic producer")]
    Semantic,
}

/// Decodes one strict GitHub workflow artifact without extracting it to the filesystem.
///
/// # Errors
///
/// The expectation is not a bounded GitHub source, the archive is malformed or ambiguous, its
/// sole payload exceeds a limit, or the payload is not the exact planned semantic template.
pub fn decode_workflow_artifact(
    expectation: &WorkflowArtifactExpectation,
    archive_bytes: &[u8],
) -> Result<AcquiredSemanticTemplate, GitHubArtifactError> {
    if !valid_github_expectation(expectation) {
        return Err(GitHubArtifactError::Expectation);
    }
    let archive_length = u64::try_from(archive_bytes.len()).unwrap_or(u64::MAX);
    if archive_length > expectation.archive_byte_limit {
        return Err(GitHubArtifactError::ArchiveBytes);
    }
    if !single_entry_eocd(archive_bytes) {
        return Err(GitHubArtifactError::Archive);
    }

    let mut archive = ZipArchive::with_config(
        Config {
            archive_offset: ArchiveOffset::Known(0),
        },
        Cursor::new(archive_bytes),
    )
    .map_err(|_defect| GitHubArtifactError::Archive)?;
    if archive.len() != 1 {
        return Err(GitHubArtifactError::Archive);
    }
    let declared_size = {
        let entry = archive
            .by_index_raw(0)
            .map_err(|_defect| GitHubArtifactError::Archive)?;
        let expected_name = expectation.payload_file.as_str();
        if entry.name() != expected_name
            || entry.name_raw() != expected_name.as_bytes()
            || !entry.is_file()
            || entry.encrypted()
            || entry.compressed_size() > archive_length
        {
            return Err(GitHubArtifactError::Archive);
        }
        if entry.size() > expectation.file_byte_limit {
            return Err(GitHubArtifactError::PayloadBytes);
        }
        entry.size()
    };

    let entry = archive
        .by_index(0)
        .map_err(|_defect| GitHubArtifactError::Archive)?;
    let capacity =
        usize::try_from(declared_size).map_err(|_defect| GitHubArtifactError::Archive)?;
    let mut payload = Vec::with_capacity(capacity);
    entry
        .take(expectation.file_byte_limit.saturating_add(1))
        .read_to_end(&mut payload)
        .map_err(|_defect| GitHubArtifactError::Archive)?;
    let observed_size = u64::try_from(payload.len()).unwrap_or(u64::MAX);
    if observed_size > expectation.file_byte_limit {
        return Err(GitHubArtifactError::PayloadBytes);
    }
    if observed_size != declared_size {
        return Err(GitHubArtifactError::Archive);
    }

    let template = amiss_wire::semantic::parse_template(&payload)
        .map_err(|_defect| GitHubArtifactError::Semantic)?;
    let actual = SemanticEvidenceExpectation {
        acquisition_identity: expectation.semantic.acquisition_identity.clone(),
        producer_kind: template.producer_kind,
        producer_identity: template.producer_identity,
        producer_version: template.producer_version,
        context_digest: template.context_digest,
    };
    if actual != expectation.semantic {
        return Err(GitHubArtifactError::Semantic);
    }
    Ok(AcquiredSemanticTemplate {
        acquisition_identity: expectation.semantic.acquisition_identity.clone(),
        bytes: Arc::from(payload),
    })
}

pub(crate) fn valid_github_expectation(expectation: &WorkflowArtifactExpectation) -> bool {
    expectation.provider.namespace.as_str() == "github"
        && expectation.repository.host() == expectation.provider.instance.as_str()
        && crate::acquisition::canonical_github_repository(&expectation.repository)
        && !expectation.artifact_name.is_empty()
        && expectation.artifact_name.len() <= 256
        && !expectation.artifact_name.chars().any(char::is_control)
        && !(expectation.payload_file.as_str().is_empty()
            || expectation
                .payload_file
                .as_str()
                .bytes()
                .any(|byte| byte.is_ascii_control()))
        && (1..=MAX_WORKFLOW_ARTIFACT_ARCHIVE_BYTES).contains(&expectation.archive_byte_limit)
        && (1..=MAX_WORKFLOW_ARTIFACT_FILE_BYTES).contains(&expectation.file_byte_limit)
}

fn single_entry_eocd(bytes: &[u8]) -> bool {
    bytes
        .len()
        .checked_sub(EOCD_BYTES)
        .and_then(|start| bytes.get(start..))
        .is_some_and(|record| {
            record.get(..4) == Some(b"PK\x05\x06".as_slice())
                && record.get(4..8) == Some([0, 0, 0, 0].as_slice())
                && record.get(8..12) == Some([1, 0, 1, 0].as_slice())
                && record.get(20..22) == Some([0, 0].as_slice())
        })
}
