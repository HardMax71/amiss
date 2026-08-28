#![expect(
    clippy::unwrap_used,
    reason = "integration fixtures construct known-valid wire identities"
)]

use std::fs;
use std::path::Path;
use std::sync::Arc;

use amiss_controller::{
    AcquiredControl, BootstrapJob, BootstrapJobError, BootstrapJobInput, ChangeId, ChangeLocator,
    CheckPlan, ControllerEvaluationId, DeliveryId, DeliveryIdentity, ExternalPolicy, IntegrationId,
    OidPair, PolicyControls, ProviderIdentity, ProviderInstance, ProviderNamespace,
    ProviderRunAttempt, ProviderRunId, ProviderRunIdentity, RunIdentity, RunRefs, RunRequest,
    SemanticEvidenceExpectation, bootstrap_job, check_binding, check_plan,
};
use amiss_wire::controls::{
    ExecutionConstraintDescriptor, ExecutionConstraintInput, Profile, TrustedTimeStatement,
};
use amiss_wire::digest::{Digest, hb};
use amiss_wire::json::{self, Value};
use amiss_wire::model::{
    ArtifactId, BranchRef, ForgeDialect, ObjectFormat, Oid, RepositoryIdentity, UtcInstant,
};
use amiss_wire::requests::{
    ControlsRequest, EvaluationRequest, REQUEST_STREAM_BYTES, RequestTrust, SnapshotRequest,
    commit_candidate_identity_digest,
};

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
    ExecutionConstraintDescriptor::parse(&example("scanner-execution-constraint.json")).unwrap()
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
    }
}

fn plan(policy: PolicyControls) -> CheckPlan {
    check_plan(Profile::Enforce, policy, execution()).unwrap()
}

fn bootstrap(
    run: &RunRequest,
    acquired_semantic_evidence: &[Value],
) -> Result<BootstrapJob, BootstrapJobError> {
    bootstrap_job(BootstrapJobInput {
        run,
        evaluation_instant: instant("2026-07-12T10:00:00Z"),
        valid_until: instant("2026-07-12T10:05:00Z"),
        acquired_semantic_evidence,
    })
}

fn candidate_identity(run: &RunRequest) -> Digest {
    let job = bootstrap(run, &[]).unwrap();
    let controls = ControlsRequest::parse(&job.streams.controls).unwrap();
    let supplied_time = controls.trusted_time.unwrap();
    TrustedTimeStatement::parse(&json::canonical(&supplied_time.value))
        .unwrap()
        .candidate_identity_digest()
}

fn semantic_evidence(
    candidate_identity_digest: Digest,
    source_report_payload_digest: Option<Digest>,
    context_digest: Digest,
) -> Value {
    amiss_wire::semantic::envelope(amiss_wire::semantic::SemanticEvidence {
        candidate_identity_digest,
        source_report_payload_digest,
        producer_kind: ArtifactId::new("site-build".to_owned()).unwrap(),
        producer_identity: ArtifactId::new("amiss-test-site-build".to_owned()).unwrap(),
        producer_version: "0.5.1".to_owned(),
        context_digest,
        input_digest: hb("amiss/test-site-build", b"output"),
        complete: true,
        observations: Vec::new(),
    })
    .unwrap()
}

fn site_expectation(context_digest: Digest) -> SemanticEvidenceExpectation {
    SemanticEvidenceExpectation {
        producer_kind: ArtifactId::new("site-build".to_owned()).unwrap(),
        producer_identity: ArtifactId::new("amiss-test-site-build".to_owned()).unwrap(),
        producer_version: "0.5.1".to_owned(),
        context_digest,
    }
}

#[test]
fn job_construction_binds_the_complete_authenticated_run() {
    let run = run_request(policy());
    let job = bootstrap_job(BootstrapJobInput {
        run: &run,
        evaluation_instant: instant("2026-07-12T10:00:00Z"),
        valid_until: instant("2026-07-12T10:05:00Z"),
        acquired_semantic_evidence: &[],
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
    let supplied_time = controls.trusted_time.unwrap();
    let statement = TrustedTimeStatement::parse(&json::canonical(&supplied_time.value)).unwrap();
    assert_eq!(statement.provider(), "gitlab");
    assert_eq!(statement.provider_run_id(), "pipeline/987654321:job-42");
    assert_eq!(statement.provider_run_attempt(), 2);
    assert_eq!(
        statement.candidate_identity_digest(),
        commit_candidate_identity_digest(&evaluation, &oid('2'), &oid('4')).unwrap()
    );
    assert_eq!(
        controls.execution_constraint.unwrap().trust_source,
        RequestTrust::ExternalRequiredCheck
    );
    assert!(controls.organization_floor.is_some());
    assert!(controls.debt_snapshot.is_some());
    assert!(controls.waiver_bundle.is_some());
    let semantic = amiss_wire::semantic::parse(&json::canonical(
        &controls.semantic_evidence.first().unwrap().value,
    ))
    .unwrap();
    assert_eq!(
        semantic.payload.candidate_identity_digest,
        statement.candidate_identity_digest()
    );
    assert_eq!(job.constraint, execution().canonical_bytes().unwrap());
}

#[test]
fn acquired_semantic_evidence_joins_the_exact_candidate() {
    let candidate = candidate_identity(&run_request(policy()));
    let context = hb("amiss/test-site-context", b"english/current");
    let mut policy = policy();
    policy.semantic_acquisitions = vec![site_expectation(context)];
    let run = run_request(policy);
    let evidence = semantic_evidence(candidate, None, context);
    let evidence_digest = amiss_wire::semantic::parse(&json::canonical(&evidence))
        .unwrap()
        .payload_digest;
    let job = bootstrap(&run, std::slice::from_ref(&evidence)).unwrap();
    let controls = ControlsRequest::parse(&job.streams.controls).unwrap();
    let payload_digests = controls
        .semantic_evidence
        .iter()
        .map(|supplied| {
            amiss_wire::semantic::parse(&json::canonical(&supplied.value))
                .unwrap()
                .payload_digest
        })
        .collect::<Vec<_>>();

    assert_eq!(controls.semantic_evidence.len(), 2);
    assert!(payload_digests.contains(&evidence_digest));
    assert!(payload_digests.windows(2).all(|pair| pair[0] < pair[1]));
}

#[test]
fn acquired_semantic_evidence_must_be_pre_scan_candidate_and_context_bound() {
    let candidate = candidate_identity(&run_request(PolicyControls::default()));
    let context = hb("amiss/test-site-context", b"english/current");
    let run = run_request(PolicyControls {
        semantic_acquisitions: vec![site_expectation(context)],
        ..PolicyControls::default()
    });
    let defects = [
        Value::Null,
        semantic_evidence(hb("amiss/test-candidate", b"other"), None, context),
        semantic_evidence(candidate, Some(hb("amiss/test-report", b"report")), context),
        semantic_evidence(
            candidate,
            None,
            hb("amiss/test-site-context", b"french/current"),
        ),
    ];

    for defect in defects {
        assert_eq!(
            bootstrap(&run, std::slice::from_ref(&defect)).unwrap_err(),
            BootstrapJobError::SemanticEvidence
        );
    }

    let duplicate = semantic_evidence(candidate, None, context);
    assert_eq!(
        bootstrap(&run, &[duplicate.clone(), duplicate]).unwrap_err(),
        BootstrapJobError::SemanticEvidence
    );

    let over_limit =
        vec![Value::Null; amiss_wire::requests::SEMANTIC_EVIDENCE_REQUEST_LIMIT.saturating_add(1)];
    assert_eq!(
        bootstrap(&run, &over_limit).unwrap_err(),
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
            acquired_semantic_evidence: &[],
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
    };
    let run = run_request(wrong_policy);
    assert_eq!(
        bootstrap_job(BootstrapJobInput {
            run: &run,
            evaluation_instant: instant("2026-07-12T10:00:00Z"),
            valid_until: instant("2026-07-12T10:05:00Z"),
            acquired_semantic_evidence: &[],
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
            acquired_semantic_evidence: &[],
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
    };
    assert_eq!(
        check_plan(Profile::Enforce, policy, execution(),).unwrap_err(),
        BootstrapJobError::RequestEncoding
    );
}

#[test]
fn a_changed_constraint_gets_a_new_semantic_digest() {
    let original = execution();
    let mut input = ExecutionConstraintInput::from(&original);
    input.required_status_name = "amiss / another check".to_owned();
    let changed = ExecutionConstraintDescriptor::new(input).unwrap();
    assert_ne!(changed.digest(), original.digest());
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
            acquired_semantic_evidence: &[],
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
            acquired_semantic_evidence: &[],
        })
        .unwrap_err(),
        BootstrapJobError::CheckPlan
    );
}
