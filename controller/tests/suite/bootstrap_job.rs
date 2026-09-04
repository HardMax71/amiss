#![expect(
    clippy::unwrap_used,
    reason = "integration fixtures construct known-valid wire identities"
)]

use std::fs;
use std::path::Path;
use std::sync::Arc;

use amiss_controller::{
    AcquiredControl, AcquiredSemanticTemplate, BootstrapJob, BootstrapJobError, BootstrapJobInput,
    ChangeId, ChangeLocator, CheckPlan, ControllerEvaluationId, DeliveryId, DeliveryIdentity,
    ExternalPolicy, IntegrationId, MAX_WORKFLOW_ARTIFACT_ARCHIVE_BYTES,
    MAX_WORKFLOW_ARTIFACT_FILE_BYTES, OidPair, OpaqueId, PolicyControls, ProviderIdentity,
    ProviderInstance, ProviderNamespace, ProviderRunAttempt, ProviderRunId, ProviderRunIdentity,
    RunIdentity, RunRefs, RunRequest, SemanticEvidenceExpectation, SemanticEvidenceTemplate,
    WorkflowArtifactExpectation, bootstrap_job, check_binding, check_plan,
};
use amiss_wire::controls::{
    ExecutionConstraintDescriptor, Profile, canonical_execution_constraint,
    parse_execution_constraint, parse_trusted_time,
};
use amiss_wire::digest::{Digest, hb};
use amiss_wire::json::{self, Value};
use amiss_wire::model::{
    ArtifactId, BranchRef, ForgeDialect, ObjectFormat, Oid, RepoPathText, RepositoryIdentity,
    UtcInstant,
};
use amiss_wire::requests::{
    ControlsRequest, EvaluationRequest, REQUEST_STREAM_BYTES, RequestTrust, SnapshotRequest,
    commit_candidate_identity_digest,
};
use base64::Engine as _;

const LARGE_INVENTORY_ENTRIES: usize = 4_093;
const MAX_PATH_BYTES: usize = 4_096;

fn example(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../spec/examples")
            .join(name),
    )
    .unwrap()
}

fn inventory_path(index: usize, length: usize) -> String {
    let prefix = format!("inventory/{index:04}/");
    format!(
        "{prefix}{}",
        "a".repeat(length.checked_sub(prefix.len()).unwrap())
    )
}

fn maximal_floor() -> Vec<u8> {
    let inventory = (0..LARGE_INVENTORY_ENTRIES)
        .map(|index| format!("\"{}\"", inventory_path(index, MAX_PATH_BYTES)))
        .collect::<Vec<_>>()
        .join(",");
    let source = String::from_utf8(example("organization-floor.json"))
        .unwrap()
        .replacen("\"README.md\"", &inventory, 1);
    json::canonical(&json::parse(source.as_bytes()).unwrap())
}

fn near_ceiling_floor() -> Vec<u8> {
    let ceiling = usize::try_from(REQUEST_STREAM_BYTES).unwrap();
    let maximal = maximal_floor();
    let floor_length = ceiling.checked_sub(1).unwrap();
    let excess = maximal.len().checked_sub(floor_length).unwrap();
    let last = LARGE_INVENTORY_ENTRIES.checked_sub(1).unwrap();
    let maximal_path = inventory_path(last, MAX_PATH_BYTES);
    let shorter_path = inventory_path(last, MAX_PATH_BYTES.checked_sub(excess).unwrap());
    let floor = String::from_utf8(maximal)
        .unwrap()
        .replacen(&maximal_path, &shorter_path, 1)
        .into_bytes();
    assert_eq!(floor.len(), floor_length);
    floor
}

fn oid(value: char) -> Oid {
    Oid::new(ObjectFormat::Sha1, value.to_string().repeat(40)).unwrap()
}

fn repository() -> RepositoryIdentity {
    RepositoryIdentity::new(
        "gitlab.example.internal".to_owned(),
        "platform/security".to_owned(),
        "docs".to_owned(),
    )
    .unwrap()
}

fn provider() -> ProviderIdentity {
    ProviderIdentity {
        namespace: ProviderNamespace::new("gitlab".to_owned()).unwrap(),
        instance: ProviderInstance::new("gitlab.example.internal".to_owned()).unwrap(),
    }
}

fn run_request(policy: PolicyControls) -> RunRequest {
    let provider = provider();
    let plan = Arc::new(plan(policy));
    let change = ChangeLocator {
        provider: provider.clone(),
        repository: repository(),
        change: ChangeId::new("merge-request/42".to_owned()).unwrap(),
    };
    RunRequest {
        delivery: DeliveryIdentity {
            provider,
            integration: IntegrationId::new("project-hook/7".to_owned()).unwrap(),
            delivery: DeliveryId::new("webhook/9".to_owned()).unwrap(),
        },
        provider_run: ProviderRunIdentity::new(
            ProviderRunId::new("pipeline/987654321:job-42".to_owned()).unwrap(),
            ProviderRunAttempt::new(2).unwrap(),
            ObjectFormat::Sha1,
            oid('3'),
        )
        .unwrap(),
        evaluation_id: ControllerEvaluationId::new("evaluation/11".to_owned()).unwrap(),
        check: check_binding(&plan).unwrap(),
        plan,
        run: RunIdentity::new(
            change,
            RunRefs {
                forge: ForgeDialect::Gitlab,
                candidate: BranchRef::new("refs/heads/amiss-controller".to_owned()).unwrap(),
                target: BranchRef::new("refs/heads/main".to_owned()).unwrap(),
                default_branch: BranchRef::new("refs/heads/main".to_owned()).unwrap(),
            },
            ObjectFormat::Sha1,
            OidPair {
                base: oid('1'),
                candidate: oid('3'),
            },
            OidPair {
                base: oid('2'),
                candidate: oid('4'),
            },
        )
        .unwrap(),
    }
}

fn execution() -> ExecutionConstraintDescriptor {
    parse_execution_constraint(&example("scanner-execution-constraint.json")).unwrap()
}

fn instant(value: &str) -> UtcInstant {
    UtcInstant::new(value.to_owned()).unwrap()
}

fn policy() -> PolicyControls {
    let acquired = |name| AcquiredControl {
        bytes: example(name),
        trust_source: RequestTrust::OrganizationPolicy,
    };
    PolicyControls {
        external_policy: ExternalPolicy::Advisory,
        organization_floor: Some(acquired("organization-floor.json")),
        debt_snapshot: Some(acquired("debt-snapshot.json")),
        waiver_bundle: Some(acquired("waiver-bundle.json")),
        semantic_evidence: super::intersphinx::evidence(),
        semantic_acquisitions: Vec::new(),
        workflow_artifacts: Vec::new(),
    }
}

fn plan(policy: PolicyControls) -> CheckPlan {
    check_plan(Profile::Enforce, policy, execution()).unwrap()
}

fn bootstrap(
    run: &RunRequest,
    acquired_semantic_templates: &[AcquiredSemanticTemplate],
) -> Result<BootstrapJob, BootstrapJobError> {
    bootstrap_job(BootstrapJobInput {
        run,
        evaluation_instant: instant("2026-07-12T10:00:00Z"),
        valid_until: instant("2026-07-12T10:05:00Z"),
        acquired_semantic_templates,
    })
}

fn candidate_identity(run: &RunRequest) -> Digest {
    let job = bootstrap(run, &[]).unwrap();
    let controls = ControlsRequest::parse(&job.streams.controls).unwrap();
    let supplied_time = controls.trusted_time.unwrap();
    parse_trusted_time(&serde_json::to_vec(&supplied_time.value).unwrap())
        .unwrap()
        .candidate_identity_digest
}

fn semantic_template(context_digest: Digest) -> Vec<u8> {
    amiss_wire::semantic::template(SemanticEvidenceTemplate {
        schema: amiss_wire::semantic::TemplateSchema::Current,
        producer: amiss_wire::semantic::SemanticProducer {
            kind: ArtifactId::new("site-build".to_owned()).unwrap(),
            identity: ArtifactId::new("amiss-test-site-build".to_owned()).unwrap(),
            version: "0.5.1".to_owned(),
            context_digest,
            input_digest: hb("amiss/test-site-build", b"output"),
        },
        complete: true,
        observations: Arc::from([]),
    })
    .unwrap()
}

struct SiteAcquisition {
    expectation: SemanticEvidenceExpectation,
    template: AcquiredSemanticTemplate,
}

fn site_acquisition(context_digest: Digest) -> SiteAcquisition {
    let acquisition_identity = ArtifactId::new("test-site-artifact".to_owned()).unwrap();
    let mut bytes = semantic_template(context_digest);
    bytes.push(b'\n');
    SiteAcquisition {
        expectation: SemanticEvidenceExpectation {
            acquisition_identity: acquisition_identity.clone(),
            producer_kind: ArtifactId::new("site-build".to_owned()).unwrap(),
            producer_identity: ArtifactId::new("amiss-test-site-build".to_owned()).unwrap(),
            producer_version: "0.5.1".to_owned(),
            context_digest,
        },
        template: AcquiredSemanticTemplate {
            acquisition_identity,
            bytes: bytes.into(),
        },
    }
}

fn workflow_acquisition(
    acquisition_identity: &str,
    artifact_name: &str,
    context_digest: Digest,
) -> (WorkflowArtifactExpectation, AcquiredSemanticTemplate) {
    let mut acquisition = site_acquisition(context_digest);
    let identity = ArtifactId::new(acquisition_identity.to_owned()).unwrap();
    acquisition.expectation.acquisition_identity = identity.clone();
    acquisition.template.acquisition_identity = identity;
    (
        WorkflowArtifactExpectation {
            provider: provider(),
            repository: repository(),
            workflow_identity: OpaqueId::new("docs-evidence.yml".to_owned()).unwrap(),
            event: OpaqueId::new("merge_request_event".to_owned()).unwrap(),
            artifact_name: artifact_name.to_owned(),
            payload_file: RepoPathText::new("amiss/semantic-template.json".to_owned()).unwrap(),
            archive_byte_limit: MAX_WORKFLOW_ARTIFACT_ARCHIVE_BYTES,
            file_byte_limit: MAX_WORKFLOW_ARTIFACT_FILE_BYTES,
            semantic: acquisition.expectation,
        },
        acquisition.template,
    )
}

#[test]
fn job_construction_binds_the_complete_authenticated_run() {
    let run = run_request(policy());
    let job = bootstrap_job(BootstrapJobInput {
        run: &run,
        evaluation_instant: instant("2026-07-12T10:00:00Z"),
        valid_until: instant("2026-07-12T10:05:00Z"),
        acquired_semantic_templates: &[],
    })
    .unwrap();

    let evaluation = EvaluationRequest::parse(&job.streams.evaluation).unwrap();
    assert_eq!(evaluation.repository, Some(repository()));
    assert_eq!(evaluation.forge, Some(ForgeDialect::Gitlab));
    assert_eq!(
        evaluation.candidate_ref.as_ref().map(BranchRef::as_str),
        Some("refs/heads/amiss-controller")
    );
    assert_eq!(
        evaluation.target_ref.as_ref().map(BranchRef::as_str),
        Some("refs/heads/main")
    );
    assert_eq!(
        SnapshotRequest::parse(&job.streams.snapshot).unwrap(),
        SnapshotRequest::git_objects()
    );

    let controls = ControlsRequest::parse(&job.streams.controls).unwrap();
    let supplied_time = controls.trusted_time.as_ref().unwrap();
    let statement = parse_trusted_time(&serde_json::to_vec(&supplied_time.value).unwrap()).unwrap();
    assert_eq!(statement.provider, "gitlab");
    assert_eq!(statement.provider_run_id, "pipeline/987654321:job-42");
    assert_eq!(statement.provider_run_attempt, 2);
    assert_eq!(
        statement.candidate_identity_digest,
        commit_candidate_identity_digest(&evaluation, &oid('2'), &oid('4')).unwrap()
    );
    assert_eq!(
        controls.execution_constraint.unwrap().trust_source,
        RequestTrust::ExternalRequiredCheck
    );
    assert!(controls.organization_floor.is_some());
    assert!(controls.debt_snapshot.is_some());
    assert!(controls.waiver_bundle.is_some());
    let semantic = amiss_wire::semantic::parse(
        &serde_json::to_vec(&controls.semantic_evidence.first().unwrap().value).unwrap(),
    )
    .unwrap();
    assert_eq!(
        semantic.payload.subject.candidate_identity_digest,
        statement.candidate_identity_digest
    );
    assert_eq!(
        job.constraint,
        canonical_execution_constraint(&execution()).unwrap().0
    );
}

#[test]
fn acquired_semantic_templates_join_the_candidate_and_retain_their_source_bytes() {
    let candidate = candidate_identity(&run_request(policy()));
    let context = hb("amiss/test-site-context", b"english/current");
    let mut policy = policy();
    let acquisition = site_acquisition(context);
    policy.semantic_acquisitions = vec![acquisition.expectation];
    let run = run_request(policy);
    let source = acquisition.template;
    let template = amiss_wire::semantic::parse_template(&source.bytes).unwrap();
    let evidence = amiss_wire::semantic::bind_template(&template, candidate).unwrap();
    let evidence_digest = amiss_wire::semantic::parse(&evidence)
        .unwrap()
        .payload_digest;
    let job = bootstrap(&run, std::slice::from_ref(&source)).unwrap();
    let replayed = bootstrap(&run, std::slice::from_ref(&source)).unwrap();
    assert_eq!(job.semantic_artifact, replayed.semantic_artifact);
    let controls = ControlsRequest::parse(&job.streams.controls).unwrap();
    let payload_digests = controls
        .semantic_evidence
        .iter()
        .map(|supplied| {
            amiss_wire::semantic::parse(&serde_json::to_vec(&supplied.value).unwrap())
                .unwrap()
                .payload_digest
        })
        .collect::<Vec<_>>();

    assert_eq!(controls.semantic_evidence.len(), 2);
    assert!(payload_digests.contains(&evidence_digest));
    assert!(payload_digests.windows(2).all(|pair| pair[0] < pair[1]));

    let artifact_bytes = job.semantic_artifact.as_deref().unwrap();
    let artifact = json::parse(artifact_bytes).unwrap();
    let Value::Array(inputs) = artifact.member("inputs").unwrap() else {
        panic!("the semantic artifact contains its input rows")
    };
    let acquired = inputs
        .iter()
        .find(|input| input.text("acquisition_identity") == Some("test-site-artifact"))
        .unwrap();
    let retained_template = base64::engine::general_purpose::STANDARD
        .decode(acquired.text("template_bytes_base64").unwrap())
        .unwrap();
    let retained_envelope = base64::engine::general_purpose::STANDARD
        .decode(acquired.text("envelope_bytes_base64").unwrap())
        .unwrap();
    let template_digest = amiss_wire::digest::sha256(&source.bytes).to_string();
    let envelope_digest = amiss_wire::digest::sha256(&retained_envelope).to_string();
    let payload_digest = evidence_digest.to_string();
    assert_eq!(retained_template, source.bytes.as_ref());
    assert_eq!(retained_envelope, evidence);
    assert_eq!(
        acquired.text("template_digest"),
        Some(template_digest.as_str())
    );
    assert_eq!(
        acquired.text("envelope_digest"),
        Some(envelope_digest.as_str())
    );
    assert_eq!(
        acquired.text("payload_digest"),
        Some(payload_digest.as_str())
    );
}

#[test]
fn workflow_artifacts_are_normalized_and_feed_semantic_binding() {
    let first = workflow_acquisition(
        "workflow-site-primary",
        "site-primary",
        hb("amiss/test-site-context", b"primary"),
    );
    let second = workflow_acquisition(
        "workflow-site-secondary",
        "site-secondary",
        hb("amiss/test-site-context", b"secondary"),
    );
    let forward = check_plan(
        Profile::Enforce,
        PolicyControls {
            workflow_artifacts: vec![first.0.clone(), second.0.clone()],
            ..PolicyControls::default()
        },
        execution(),
    )
    .unwrap();
    let reverse = check_plan(
        Profile::Enforce,
        PolicyControls {
            workflow_artifacts: vec![second.0.clone(), first.0.clone()],
            ..PolicyControls::default()
        },
        execution(),
    )
    .unwrap();
    assert_eq!(forward.digest, reverse.digest);
    assert_eq!(forward.policy, reverse.policy);

    let run = run_request(forward.policy);
    let job = bootstrap(&run, &[second.1, first.1]).unwrap();
    let controls = ControlsRequest::parse(&job.streams.controls).unwrap();
    assert_eq!(controls.semantic_evidence.len(), 2);
    assert!(job.semantic_artifact.is_some());
}

#[test]
fn workflow_artifact_plans_reject_invalid_or_ambiguous_sources() {
    let (valid, _template) = workflow_acquisition(
        "workflow-site-primary",
        "site-primary",
        hb("amiss/test-site-context", b"primary"),
    );
    let mut wrong_host = valid.clone();
    wrong_host.provider =
        ProviderIdentity::new("gitlab".to_owned(), "other.example".to_owned()).unwrap();
    let mut empty_name = valid.clone();
    empty_name.artifact_name.clear();
    let mut control_name = valid.clone();
    control_name.artifact_name = "site\nprimary".to_owned();
    let mut control_path = valid.clone();
    control_path.payload_file = RepoPathText::new("amiss/site\nprimary.json".to_owned()).unwrap();
    let mut zero_archive = valid.clone();
    zero_archive.archive_byte_limit = 0;
    let mut large_archive = valid.clone();
    large_archive.archive_byte_limit = MAX_WORKFLOW_ARTIFACT_ARCHIVE_BYTES.saturating_add(1);
    let mut zero_file = valid.clone();
    zero_file.file_byte_limit = 0;
    let mut large_file = valid.clone();
    large_file.file_byte_limit = MAX_WORKFLOW_ARTIFACT_FILE_BYTES.saturating_add(1);

    for defect in [
        wrong_host,
        empty_name,
        control_name,
        control_path,
        zero_archive,
        large_archive,
        zero_file,
        large_file,
    ] {
        assert_eq!(
            check_plan(
                Profile::Enforce,
                PolicyControls {
                    workflow_artifacts: vec![defect],
                    ..PolicyControls::default()
                },
                execution(),
            )
            .unwrap_err(),
            BootstrapJobError::WorkflowArtifact
        );
    }

    let (same_artifact, _template) = workflow_acquisition(
        "workflow-site-other",
        "site-primary",
        hb("amiss/test-site-context", b"other"),
    );
    assert_eq!(
        check_plan(
            Profile::Enforce,
            PolicyControls {
                workflow_artifacts: vec![valid.clone(), same_artifact],
                ..PolicyControls::default()
            },
            execution(),
        )
        .unwrap_err(),
        BootstrapJobError::WorkflowArtifact
    );

    assert_eq!(
        check_plan(
            Profile::Enforce,
            PolicyControls {
                semantic_acquisitions: vec![valid.semantic.clone()],
                workflow_artifacts: vec![valid],
                ..PolicyControls::default()
            },
            execution(),
        )
        .unwrap_err(),
        BootstrapJobError::SemanticEvidence
    );
}

#[test]
fn acquired_semantic_templates_must_match_the_planned_identity_and_context() {
    let context = hb("amiss/test-site-context", b"english/current");
    let acquisition = site_acquisition(context);
    let run = run_request(PolicyControls {
        semantic_acquisitions: vec![acquisition.expectation],
        ..PolicyControls::default()
    });
    let defects = [
        AcquiredSemanticTemplate {
            acquisition_identity: ArtifactId::new("test-site-artifact".to_owned()).unwrap(),
            bytes: Arc::from(*b"null"),
        },
        AcquiredSemanticTemplate {
            acquisition_identity: ArtifactId::new("other-site-artifact".to_owned()).unwrap(),
            bytes: semantic_template(context).into(),
        },
        site_acquisition(hb("amiss/test-site-context", b"french/current")).template,
    ];

    for defect in defects {
        assert_eq!(
            bootstrap(&run, std::slice::from_ref(&defect)).unwrap_err(),
            BootstrapJobError::SemanticEvidence
        );
    }

    let duplicate = acquisition.template;
    assert_eq!(
        bootstrap(&run, &[duplicate.clone(), duplicate]).unwrap_err(),
        BootstrapJobError::SemanticEvidence
    );

    let over_limit = vec![
        site_acquisition(context).template;
        amiss_wire::requests::SEMANTIC_EVIDENCE_REQUEST_LIMIT.saturating_add(1)
    ];
    assert_eq!(
        bootstrap(&run, &over_limit).unwrap_err(),
        BootstrapJobError::SemanticEvidence
    );

    let mut repeated_identity = site_acquisition(context).expectation;
    repeated_identity.context_digest = hb("amiss/test-site-context", b"other");
    let duplicate_plan = PolicyControls {
        semantic_acquisitions: vec![site_acquisition(context).expectation, repeated_identity],
        ..PolicyControls::default()
    };
    assert_eq!(
        check_plan(Profile::Enforce, duplicate_plan, execution()).unwrap_err(),
        BootstrapJobError::SemanticEvidence
    );
}

#[test]
fn job_construction_rejects_mismatched_run_control_and_time() {
    let mut run = run_request(PolicyControls::default());
    run.provider_run.candidate_commit = oid('5');
    assert_eq!(
        bootstrap_job(BootstrapJobInput {
            run: &run,
            evaluation_instant: instant("2026-07-12T10:00:00Z"),
            valid_until: instant("2026-07-12T10:05:00Z"),
            acquired_semantic_templates: &[],
        })
        .unwrap_err(),
        BootstrapJobError::RunIdentity
    );

    let wrong_floor = String::from_utf8(example("organization-floor.json"))
        .unwrap()
        .replace(r#""name": "docs""#, r#""name": "other""#)
        .into_bytes();
    let wrong_policy = PolicyControls {
        external_policy: ExternalPolicy::Advisory,
        organization_floor: Some(AcquiredControl {
            bytes: wrong_floor,
            trust_source: RequestTrust::OrganizationPolicy,
        }),
        debt_snapshot: None,
        waiver_bundle: None,
        semantic_evidence: Vec::new(),
        semantic_acquisitions: Vec::new(),
        workflow_artifacts: Vec::new(),
    };
    let run = run_request(wrong_policy);
    assert_eq!(
        bootstrap_job(BootstrapJobInput {
            run: &run,
            evaluation_instant: instant("2026-07-12T10:00:00Z"),
            valid_until: instant("2026-07-12T10:05:00Z"),
            acquired_semantic_templates: &[],
        })
        .unwrap_err(),
        BootstrapJobError::ControlBinding
    );

    let run = run_request(PolicyControls::default());
    assert_eq!(
        bootstrap_job(BootstrapJobInput {
            run: &run,
            evaluation_instant: instant("2026-07-12T10:00:00Z"),
            valid_until: instant("2026-07-12T10:20:00Z"),
            acquired_semantic_templates: &[],
        })
        .unwrap_err(),
        BootstrapJobError::TrustedTime
    );
}

#[test]
fn plan_validation_rejects_an_aggregate_controls_stream_above_the_ceiling() {
    let floor = near_ceiling_floor();
    let policy = PolicyControls {
        external_policy: ExternalPolicy::Advisory,
        organization_floor: Some(AcquiredControl {
            bytes: floor,
            trust_source: RequestTrust::OrganizationPolicy,
        }),
        debt_snapshot: None,
        waiver_bundle: None,
        semantic_evidence: Vec::new(),
        semantic_acquisitions: Vec::new(),
        workflow_artifacts: Vec::new(),
    };
    assert_eq!(
        check_plan(Profile::Enforce, policy, execution(),).unwrap_err(),
        BootstrapJobError::RequestEncoding
    );
}

#[test]
fn a_changed_constraint_gets_a_new_semantic_digest() {
    let original = execution();
    let mut changed = original.clone();
    changed.required_status_name = "amiss / another check".to_owned();
    assert_ne!(
        canonical_execution_constraint(&changed).unwrap().1,
        canonical_execution_constraint(&original).unwrap().1
    );
    assert!(check_plan(Profile::Enforce, PolicyControls::default(), changed,).is_ok());
}

#[test]
fn a_validated_plan_cannot_be_changed_in_place() {
    let mut run = run_request(PolicyControls::default());
    Arc::make_mut(&mut run.plan).profile = Profile::Observe;
    assert_eq!(
        bootstrap_job(BootstrapJobInput {
            run: &run,
            evaluation_instant: instant("2026-07-12T10:00:00Z"),
            valid_until: instant("2026-07-12T10:05:00Z"),
            acquired_semantic_templates: &[],
        })
        .unwrap_err(),
        BootstrapJobError::CheckPlan
    );
}

#[test]
fn a_job_cannot_escape_the_ledger_frozen_plan_binding() {
    let mut run = run_request(PolicyControls::default());
    run.check.plan_digest = hb("amiss/test-plan", b"other");

    assert_eq!(
        bootstrap_job(BootstrapJobInput {
            run: &run,
            evaluation_instant: instant("2026-07-12T10:00:00Z"),
            valid_until: instant("2026-07-12T10:05:00Z"),
            acquired_semantic_templates: &[],
        })
        .unwrap_err(),
        BootstrapJobError::CheckPlan
    );
}
