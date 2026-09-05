use std::fs::File;
use std::io::Read as _;
use std::path::Path;

use amiss_bootstrap::supervise::{SealedControlExpectation, SealedExpectations};
use amiss_git::{GitLimits, GitResources, ObjectKind, Repository};
use amiss_wire::controls::{
    ExecutionConstraintDescriptor, TrustedTimeStatement, canonical_execution_constraint,
    canonical_trusted_time,
};
use amiss_wire::report::model::{SemanticEvidenceProducer, SemanticEvidenceProvenance};
use amiss_wire::requests::{
    ControlsRequest, EvaluationRequest, REQUEST_STREAM_BYTES, RequestMode, RequestStreams,
    SnapshotMaterialization, SnapshotRequest,
};
use serde::Deserialize;

use super::{Args, Execution, Failure, SealedRun, tampered, unavailable};

#[derive(Clone, Copy)]
enum ReadDefect {
    Unavailable,
    Oversized,
}

const fn input_failure(
    defect: ReadDefect,
    unavailable_diagnostic: &'static str,
    invalid_diagnostic: &'static str,
) -> Failure {
    match defect {
        ReadDefect::Unavailable => unavailable(unavailable_diagnostic),
        ReadDefect::Oversized => tampered(invalid_diagnostic),
    }
}

pub(super) fn capture_requests(
    args: &Args,
    constraint: &ExecutionConstraintDescriptor,
) -> Execution<SealedRun> {
    let streams = request_streams(args)?;
    let evaluation = EvaluationRequest::parse(&streams.evaluation)
        .map_err(|_defect| tampered("evaluation-request-invalid"))?;
    let snapshot = SnapshotRequest::parse(&streams.snapshot)
        .map_err(|_defect| tampered("snapshot-request-invalid"))?;
    let controls = ControlsRequest::parse(&streams.controls)
        .map_err(|_defect| tampered("controls-request-invalid"))?;
    let (_, constraint_digest) = canonical_execution_constraint(constraint)
        .map_err(|_defect| tampered("execution-constraint-invalid"))?;
    let canonical_requests = evaluation.canonical_bytes().ok().as_deref()
        == Some(streams.evaluation.as_slice())
        && snapshot.canonical_bytes().ok().as_deref() == Some(streams.snapshot.as_slice())
        && controls.canonical_bytes().ok().as_deref() == Some(streams.controls.as_slice());
    if !canonical_requests {
        return Err(tampered("request-noncanonical"));
    }
    let candidate = match (
        evaluation.mode,
        evaluation.candidate_commit.as_ref(),
        snapshot.materialization,
    ) {
        (RequestMode::CommitPair, Some(candidate), SnapshotMaterialization::GitObjects) => {
            candidate.clone()
        }
        (
            RequestMode::CommitPair | RequestMode::Index,
            None | Some(_),
            SnapshotMaterialization::GitObjects | SnapshotMaterialization::Index,
        ) => {
            return Err(tampered("request-mode-mismatch"));
        }
    };
    let repository = sealed_identity(&evaluation).map_err(tampered)?;
    let supplied_constraint = controls
        .execution_constraint
        .as_ref()
        .ok_or_else(|| tampered("execution-constraint-absent"))?;
    let embedded_constraint =
        ExecutionConstraintDescriptor::deserialize(&supplied_constraint.value)
            .map_err(|_defect| tampered("execution-constraint-invalid"))?;
    canonical_execution_constraint(&embedded_constraint)
        .map_err(|_defect| tampered("execution-constraint-invalid"))?;
    if constraint_digest != supplied_constraint.expected_digest
        || embedded_constraint != *constraint
    {
        return Err(tampered("execution-constraint-mismatch"));
    }
    let supplied_time = controls
        .trusted_time
        .as_ref()
        .ok_or_else(|| tampered("trusted-time-absent"))?;
    let statement = TrustedTimeStatement::deserialize(&supplied_time.value)
        .map_err(|_defect| tampered("trusted-time-invalid"))?;
    let (_, statement_digest) =
        canonical_trusted_time(&statement).map_err(|_defect| tampered("trusted-time-invalid"))?;
    if statement_digest != supplied_time.expected_digest
        || statement.provider != supplied_time.provider
        || statement.provider_run_id != supplied_time.provider_run_id
        || statement.provider_run_attempt != supplied_time.provider_run_attempt
    {
        return Err(tampered("trusted-time-mismatch"));
    }
    let expected = SealedExpectations {
        profile: evaluation.profile,
        candidate_ref: evaluation
            .candidate_ref
            .as_ref()
            .map_or_else(String::new, |reference| reference.as_str().to_owned()),
        target_ref: evaluation
            .target_ref
            .as_ref()
            .map_or_else(String::new, |reference| reference.as_str().to_owned()),
        repository,
        provider: supplied_time.provider.clone(),
        provider_run_id: supplied_time.provider_run_id.clone(),
        provider_run_attempt: supplied_time.provider_run_attempt,
        candidate_identity_digest: statement.candidate_identity_digest,
        organization_floor: control_expectation(controls.organization_floor.as_ref()),
        debt_snapshot: control_expectation(controls.debt_snapshot.as_ref()),
        waiver_bundle: control_expectation(controls.waiver_bundle.as_ref()),
        execution_constraint: SealedControlExpectation {
            digest: constraint_digest,
            trust_source: supplied_constraint.trust_source,
        },
        trusted_time_digest: statement_digest,
        semantic_evidence: semantic_expectations(&controls.semantic_evidence)?,
    };
    let mut evaluation = evaluation;
    evaluation.candidate_commit = Some(candidate);
    Ok(SealedRun {
        streams,
        evaluation,
        expected,
    })
}

fn semantic_expectations(
    values: &[amiss_wire::requests::SuppliedSemanticEvidence],
) -> Execution<Vec<SemanticEvidenceProvenance>> {
    values
        .iter()
        .map(|supplied| {
            let bytes = serde_json::to_vec(&supplied.value)
                .map_err(|_defect| tampered("semantic-evidence-invalid"))?;
            let envelope = amiss_wire::semantic::parse(&bytes)
                .map_err(|_defect| tampered("semantic-evidence-invalid"))?;
            if envelope.payload.producer.context_digest != supplied.expected_context_digest {
                return Err(tampered("semantic-evidence-invalid"));
            }
            Ok(SemanticEvidenceProvenance {
                payload_digest: envelope.payload_digest,
                producer: SemanticEvidenceProducer {
                    identity: envelope.payload.producer.identity,
                    input_digest: envelope.payload.producer.input_digest,
                    kind: envelope.payload.producer.kind,
                    version: envelope.payload.producer.version,
                },
            })
        })
        .collect()
}

fn request_streams(args: &Args) -> Execution<RequestStreams> {
    Ok(RequestStreams {
        evaluation: read_input(
            &args.evaluation_request,
            "evaluation-request-unreadable",
            "evaluation-request-invalid",
        )?,
        snapshot: read_input(
            &args.snapshot_request,
            "snapshot-request-unreadable",
            "snapshot-request-invalid",
        )?,
        controls: read_input(
            &args.controls_request,
            "controls-request-unreadable",
            "controls-request-invalid",
        )?,
    })
}

fn sealed_identity(
    evaluation: &EvaluationRequest,
) -> Result<amiss_wire::model::RepositoryIdentity, &'static str> {
    let Some(repository) = evaluation.repository.clone() else {
        return Err("evaluation-identity-absent");
    };
    if evaluation.forge.is_none()
        || evaluation.candidate_ref.is_none()
        || evaluation.target_ref.is_none()
        || evaluation.default_branch_ref.is_none()
    {
        return Err("evaluation-identity-absent");
    }
    Ok(repository)
}

fn control_expectation(
    supplied: Option<&amiss_wire::requests::SuppliedControl>,
) -> Option<SealedControlExpectation> {
    supplied.map(|control| SealedControlExpectation {
        digest: control.expected_digest,
        trust_source: control.trust_source,
    })
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, ReadDefect> {
    let file = File::open(path).map_err(|_defect| ReadDefect::Unavailable)?;
    let mut bytes = Vec::new();
    file.take(REQUEST_STREAM_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_defect| ReadDefect::Unavailable)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > REQUEST_STREAM_BYTES {
        return Err(ReadDefect::Oversized);
    }
    Ok(bytes)
}

pub(super) fn read_input(
    path: &Path,
    unavailable_diagnostic: &'static str,
    invalid_diagnostic: &'static str,
) -> Execution<Vec<u8>> {
    read_bounded(path)
        .map_err(|defect| input_failure(defect, unavailable_diagnostic, invalid_diagnostic))
}

pub(super) fn pre_acquired(path: &Path, evaluation: &EvaluationRequest) -> Result<(), ()> {
    let repository = Repository::open(path, evaluation.object_format).map_err(|_defect| ())?;
    let mut resources = GitResources::new(GitLimits::CONTRACT);
    repository
        .read_expected(&mut resources, &evaluation.base_commit, ObjectKind::Commit)
        .map_err(|_defect| ())?;
    let candidate = evaluation.candidate_commit.as_ref().ok_or(())?;
    repository
        .read_expected(&mut resources, candidate, ObjectKind::Commit)
        .map_err(|_defect| ())?;
    Ok(())
}
