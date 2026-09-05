#![cfg(test)]

use std::io::{Cursor, Write as _};
use std::sync::Arc;

use amiss_controller::{
    MAX_WORKFLOW_ARTIFACT_ARCHIVE_BYTES, MAX_WORKFLOW_ARTIFACT_FILE_BYTES, OpaqueId,
    ProviderIdentity, SemanticEvidenceExpectation, SemanticEvidenceTemplate,
    WorkflowArtifactExpectation,
};
use amiss_wire::digest::hb;
use amiss_wire::model::{ArtifactId, RepoPathText, RepositoryIdentity};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use super::{GitHubArtifactError, decode_workflow_artifact};

const PAYLOAD_FILE: &str = "amiss/semantic-template.json";

#[test]
fn stored_and_deflated_single_payloads_retain_the_exact_template() {
    let expectation = expectation();
    let payload = template(expectation.semantic.context_digest);
    for method in [CompressionMethod::Stored, CompressionMethod::Deflated] {
        let archive = archive(&[(PAYLOAD_FILE, payload.as_slice())], method);
        let acquired = decode_workflow_artifact(&expectation, &archive).unwrap();
        assert_eq!(
            acquired.acquisition_identity,
            expectation.semantic.acquisition_identity
        );
        assert_eq!(acquired.bytes.as_ref(), payload);
    }
}

#[test]
fn both_byte_limits_are_inclusive_and_independent() {
    let mut expectation = expectation();
    let payload = template(expectation.semantic.context_digest);
    let archive = archive(
        &[(PAYLOAD_FILE, payload.as_slice())],
        CompressionMethod::Deflated,
    );
    expectation.archive_byte_limit = u64::try_from(archive.len()).unwrap();
    expectation.file_byte_limit = u64::try_from(payload.len()).unwrap();
    assert!(decode_workflow_artifact(&expectation, &archive).is_ok());

    expectation.archive_byte_limit = expectation.archive_byte_limit.saturating_sub(1);
    assert_eq!(
        decode_workflow_artifact(&expectation, &archive),
        Err(GitHubArtifactError::ArchiveBytes)
    );
    expectation.archive_byte_limit = u64::try_from(archive.len()).unwrap();
    expectation.file_byte_limit = expectation.file_byte_limit.saturating_sub(1);
    assert_eq!(
        decode_workflow_artifact(&expectation, &archive),
        Err(GitHubArtifactError::PayloadBytes)
    );
}

#[test]
fn malformed_ambiguous_unsafe_and_encrypted_archives_are_refused() {
    let expectation = expectation();
    let payload = template(expectation.semantic.context_digest);
    let regular = archive(
        &[(PAYLOAD_FILE, payload.as_slice())],
        CompressionMethod::Stored,
    );
    let cases = [
        b"not a zip".to_vec(),
        archive(&[], CompressionMethod::Stored),
        archive(
            &[(PAYLOAD_FILE, payload.as_slice()), ("extra", b"extra")],
            CompressionMethod::Stored,
        ),
        archive(
            &[("../semantic-template.json", payload.as_slice())],
            CompressionMethod::Stored,
        ),
        directory_archive(PAYLOAD_FILE),
        symlink_archive(PAYLOAD_FILE),
        commented_archive(PAYLOAD_FILE, &payload),
        [b"prepended junk".as_slice(), regular.as_slice()].concat(),
        encrypted(regular),
    ];
    for defect in cases {
        assert_eq!(
            decode_workflow_artifact(&expectation, &defect),
            Err(GitHubArtifactError::Archive)
        );
    }
}

#[test]
fn the_payload_must_be_the_planned_semantic_template() {
    let expectation = expectation();
    let invalid = archive(
        &[(PAYLOAD_FILE, b"not semantic JSON")],
        CompressionMethod::Stored,
    );
    assert_eq!(
        decode_workflow_artifact(&expectation, &invalid),
        Err(GitHubArtifactError::Semantic)
    );

    let other = template(hb("amiss/test-workflow-context", b"other"));
    let mismatched = archive(
        &[(PAYLOAD_FILE, other.as_slice())],
        CompressionMethod::Stored,
    );
    assert_eq!(
        decode_workflow_artifact(&expectation, &mismatched),
        Err(GitHubArtifactError::Semantic)
    );
}

#[test]
fn an_unchecked_non_github_or_unbounded_expectation_is_refused() {
    let mut expectation = expectation();
    let payload = template(expectation.semantic.context_digest);
    let archive = archive(
        &[(PAYLOAD_FILE, payload.as_slice())],
        CompressionMethod::Stored,
    );
    expectation.provider =
        ProviderIdentity::new("gitea".to_owned(), "github.com".to_owned()).unwrap();
    assert_eq!(
        decode_workflow_artifact(&expectation, &archive),
        Err(GitHubArtifactError::Expectation)
    );
    expectation.provider = provider();
    expectation.repository = RepositoryIdentity::new(
        "github.com".to_owned(),
        "acme/tools".to_owned(),
        "widget".to_owned(),
    )
    .unwrap();
    assert_eq!(
        decode_workflow_artifact(&expectation, &archive),
        Err(GitHubArtifactError::Expectation)
    );
    expectation.repository =
        RepositoryIdentity::github("acme".to_owned(), "widget".to_owned()).unwrap();
    expectation.artifact_name.clear();
    assert_eq!(
        decode_workflow_artifact(&expectation, &archive),
        Err(GitHubArtifactError::Expectation)
    );
    expectation.artifact_name = "amiss-semantic-evidence".to_owned();
    expectation.file_byte_limit = MAX_WORKFLOW_ARTIFACT_FILE_BYTES.saturating_add(1);
    assert_eq!(
        decode_workflow_artifact(&expectation, &archive),
        Err(GitHubArtifactError::Expectation)
    );
}

fn expectation() -> WorkflowArtifactExpectation {
    let context_digest = hb("amiss/test-workflow-context", b"site/current");
    WorkflowArtifactExpectation {
        provider: provider(),
        repository: RepositoryIdentity::github("acme".to_owned(), "widget".to_owned()).unwrap(),
        workflow_identity: OpaqueId::new("docs-evidence.yml".to_owned()).unwrap(),
        event: OpaqueId::new("pull_request".to_owned()).unwrap(),
        artifact_name: "amiss-semantic-evidence".to_owned(),
        payload_file: RepoPathText::new(PAYLOAD_FILE.to_owned()).unwrap(),
        archive_byte_limit: MAX_WORKFLOW_ARTIFACT_ARCHIVE_BYTES,
        file_byte_limit: MAX_WORKFLOW_ARTIFACT_FILE_BYTES,
        semantic: SemanticEvidenceExpectation {
            acquisition_identity: ArtifactId::new("github-docs-evidence".to_owned()).unwrap(),
            producer_kind: ArtifactId::new("site-build".to_owned()).unwrap(),
            producer_identity: ArtifactId::new("test-site-builder".to_owned()).unwrap(),
            producer_version: "0.5.1".to_owned(),
            context_digest,
        },
    }
}

fn provider() -> ProviderIdentity {
    ProviderIdentity::new("github".to_owned(), "github.com".to_owned()).unwrap()
}

fn template(context_digest: amiss_wire::digest::Digest) -> Vec<u8> {
    amiss_wire::semantic::template(SemanticEvidenceTemplate::<serde_json::Value> {
        schema: amiss_wire::semantic::TemplateSchema::Current,
        producer: amiss_wire::semantic::SemanticProducer {
            kind: ArtifactId::new("site-build".to_owned()).unwrap(),
            identity: ArtifactId::new("test-site-builder".to_owned()).unwrap(),
            version: "0.5.1".to_owned(),
            context_digest,
            input_digest: hb("amiss/test-workflow-input", b"completed site"),
        },
        complete: true,
        observations: Arc::from([]),
    })
    .unwrap()
}

fn archive(entries: &[(&str, &[u8])], method: CompressionMethod) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(method);
    for (name, bytes) in entries {
        writer.start_file(*name, options).unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn directory_archive(name: &str) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .add_directory(name, SimpleFileOptions::default())
        .unwrap();
    writer.finish().unwrap().into_inner()
}

fn symlink_archive(name: &str) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .add_symlink(name, "elsewhere", SimpleFileOptions::default())
        .unwrap();
    writer.finish().unwrap().into_inner()
}

fn commented_archive(name: &str, bytes: &[u8]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    writer.set_comment("ambiguous trailing identity").unwrap();
    writer
        .start_file(name, SimpleFileOptions::default())
        .unwrap();
    writer.write_all(bytes).unwrap();
    writer.finish().unwrap().into_inner()
}

fn encrypted(mut archive: Vec<u8>) -> Vec<u8> {
    for (signature, flag_offset) in [(b"PK\x03\x04", 6), (b"PK\x01\x02", 8)] {
        let start = archive
            .windows(signature.len())
            .position(|window| window == signature)
            .unwrap();
        let flag = start.checked_add(flag_offset).unwrap();
        archive[flag] |= 1;
    }
    archive
}
