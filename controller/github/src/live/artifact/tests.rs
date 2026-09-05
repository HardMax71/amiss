#![cfg(test)]

use std::io::{Cursor, Write as _};
use std::sync::Arc;

use amiss_controller::{
    MAX_WORKFLOW_ARTIFACT_ARCHIVE_BYTES, MAX_WORKFLOW_ARTIFACT_FILE_BYTES, OpaqueId, ProviderError,
    ProviderIdentity, SemanticEvidenceExpectation, SemanticEvidenceTemplate,
    WorkflowArtifactExpectation,
};
use amiss_wire::digest::{hb, sha256};
use amiss_wire::model::{ArtifactId, ObjectFormat, Oid, RepoPathText, RepositoryIdentity};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use super::{
    ArtifactRunRecord, SelectedArtifact, WorkflowArtifactPage, WorkflowArtifactRecord,
    WorkflowRepositoryRecord, WorkflowRunPage, WorkflowRunRecord, finish_workflow_artifact,
    select_workflow_artifact, select_workflow_run, validate_workflow_request,
};
use crate::live::Config;
use crate::live::model::OwnerRecord;

const PAYLOAD_FILE: &str = "amiss/semantic-template.json";

#[test]
fn exact_provider_records_select_and_retain_the_planned_template() {
    let (config, expectation, candidate) = fixture();
    let run = select_workflow_run(&config, &expectation, &candidate, run_page(&candidate)).unwrap();
    let payload = template(expectation.semantic.context_digest);
    let archive = archive(&payload);
    let selected = select_workflow_artifact(
        &expectation,
        &run,
        artifact_page(&run, &expectation.artifact_name, &archive),
    )
    .unwrap();
    let acquired = finish_workflow_artifact(&expectation, selected, &archive).unwrap();
    assert_eq!(
        acquired.acquisition_identity,
        expectation.semantic.acquisition_identity
    );
    assert_eq!(acquired.bytes.as_ref(), payload);
}

#[test]
fn every_workflow_run_binding_clause_fails_closed() {
    let (config, expectation, candidate) = fixture();
    let defects: [fn(&mut WorkflowRunPage); 14] = [
        |page| page.total_count = 2,
        |page| page.workflow_runs.push(page.workflow_runs[0].clone()),
        |page| page.workflow_runs[0].id = 0,
        |page| page.workflow_runs[0].head_sha = "f".repeat(40),
        |page| page.workflow_runs[0].event = "push".to_owned(),
        |page| page.workflow_runs[0].status = "queued".to_owned(),
        |page| page.workflow_runs[0].conclusion = Some("failure".to_owned()),
        |page| page.workflow_runs[0].workflow_id = 0,
        |page| page.workflow_runs[0].run_attempt = 0,
        |page| page.workflow_runs[0].repository.id = 0,
        |page| page.workflow_runs[0].repository.full_name = "other/widget".to_owned(),
        |page| page.workflow_runs[0].head_repository.id = 0,
        |page| page.workflow_runs[0].head_repository.owner.login = "bad owner".to_owned(),
        |page| page.workflow_runs.clear(),
    ];
    for defect in defects {
        let mut page = run_page(&candidate);
        defect(&mut page);
        assert!(
            select_workflow_run(&config, &expectation, &candidate, page).is_err(),
            "each provider response clause is load bearing"
        );
    }

    let mut numeric = expectation.clone();
    numeric.workflow_identity = OpaqueId::new("123".to_owned()).unwrap();
    assert_eq!(
        select_workflow_run(&config, &numeric, &candidate, run_page(&candidate)).err(),
        Some(ProviderError::InvalidResponse)
    );
    let mut matching = run_page(&candidate);
    matching.workflow_runs[0].workflow_id = 123;
    assert!(select_workflow_run(&config, &numeric, &candidate, matching).is_ok());
}

#[test]
fn every_artifact_binding_clause_fails_closed() {
    let (config, expectation, candidate) = fixture();
    let run = select_workflow_run(&config, &expectation, &candidate, run_page(&candidate)).unwrap();
    let archive = archive(&template(expectation.semantic.context_digest));
    let defects: [fn(&mut WorkflowArtifactPage); 14] = [
        |page| page.total_count = 2,
        |page| page.artifacts.push(page.artifacts[0].clone()),
        |page| page.artifacts.clear(),
        |page| page.artifacts[0].id = 0,
        |page| page.artifacts[0].name = "other".to_owned(),
        |page| page.artifacts[0].size_in_bytes = 0,
        |page| page.artifacts[0].size_in_bytes = u64::MAX,
        |page| page.artifacts[0].expired = true,
        |page| page.artifacts[0].digest = "sha256:bad".to_owned(),
        |page| page.artifacts[0].workflow_run = None,
        |page| page.artifacts[0].workflow_run.as_mut().unwrap().id += 1,
        |page| {
            page.artifacts[0]
                .workflow_run
                .as_mut()
                .unwrap()
                .repository_id += 1;
        },
        |page| {
            page.artifacts[0]
                .workflow_run
                .as_mut()
                .unwrap()
                .head_repository_id += 1;
        },
        |page| page.artifacts[0].workflow_run.as_mut().unwrap().head_sha = "f".repeat(40),
    ];
    for defect in defects {
        let mut page = artifact_page(&run, &expectation.artifact_name, &archive);
        defect(&mut page);
        assert!(
            select_workflow_artifact(&expectation, &run, page).is_err(),
            "each artifact response clause is load bearing"
        );
    }
}

#[test]
fn request_and_download_metadata_are_independently_exact() {
    let (config, mut expectation, candidate) = fixture();
    assert_eq!(
        validate_workflow_request(&config, &expectation, &candidate),
        Ok(())
    );
    expectation.provider =
        ProviderIdentity::new("gitea".to_owned(), "github.com".to_owned()).unwrap();
    assert_eq!(
        validate_workflow_request(&config, &expectation, &candidate),
        Err(ProviderError::InvalidResponse)
    );

    let archive = b"exact archive";
    let selected = SelectedArtifact {
        id: 7,
        size: u64::try_from(archive.len()).unwrap(),
        digest: sha256(archive),
    };
    let (_, expectation, _) = fixture();
    let wrong_size = SelectedArtifact {
        size: selected.size + 1,
        ..selected
    };
    assert_eq!(
        finish_workflow_artifact(&expectation, wrong_size, archive),
        Err(ProviderError::InvalidResponse)
    );
    let wrong_digest = SelectedArtifact {
        digest: sha256(b"other"),
        ..selected
    };
    assert_eq!(
        finish_workflow_artifact(&expectation, wrong_digest, archive),
        Err(ProviderError::InvalidResponse)
    );
}

fn fixture() -> (Config, WorkflowArtifactExpectation, Oid) {
    let provider = ProviderIdentity::new("github".to_owned(), "github.com".to_owned()).unwrap();
    let candidate = Oid::new(ObjectFormat::Sha1, "a".repeat(40)).unwrap();
    let context_digest = hb("amiss/test-workflow-context", b"site/current");
    (
        Config {
            provider: provider.clone(),
            app_id: 99,
            installation_id: 7,
            required_status_name: "amiss".to_owned(),
        },
        WorkflowArtifactExpectation {
            provider,
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
        },
        candidate,
    )
}

fn repository(id: u64, owner: &str, name: &str) -> WorkflowRepositoryRecord {
    WorkflowRepositoryRecord {
        id,
        name: name.to_owned(),
        full_name: format!("{owner}/{name}"),
        owner: OwnerRecord {
            login: owner.to_owned(),
        },
    }
}

fn run_page(candidate: &Oid) -> WorkflowRunPage {
    WorkflowRunPage {
        total_count: 1,
        workflow_runs: vec![WorkflowRunRecord {
            id: 41,
            head_sha: candidate.as_str().to_owned(),
            event: "pull_request".to_owned(),
            status: "completed".to_owned(),
            conclusion: Some("success".to_owned()),
            workflow_id: 321,
            run_attempt: 2,
            repository: repository(101, "Acme", "Widget"),
            head_repository: repository(202, "Contributor", "Widget-Fork"),
        }],
    }
}

fn artifact_page(run: &WorkflowRunRecord, name: &str, archive: &[u8]) -> WorkflowArtifactPage {
    WorkflowArtifactPage {
        total_count: 1,
        artifacts: vec![WorkflowArtifactRecord {
            id: 73,
            name: name.to_owned(),
            size_in_bytes: u64::try_from(archive.len()).unwrap(),
            expired: false,
            digest: sha256(archive).to_string(),
            workflow_run: Some(ArtifactRunRecord {
                id: run.id,
                repository_id: run.repository.id,
                head_repository_id: run.head_repository.id,
                head_sha: run.head_sha.clone(),
            }),
        }],
    }
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

fn archive(payload: &[u8]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file(
            PAYLOAD_FILE,
            SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
        )
        .unwrap();
    writer.write_all(payload).unwrap();
    writer.finish().unwrap().into_inner()
}
