#![expect(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "end-to-end harness over asserted fixture shapes"
)]

#[path = "wrapper/semantic.rs"]
mod semantic;
mod support;

use std::ffi::OsStr;
use std::fs;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Output};
use std::sync::LazyLock;

use amiss_bootstrap::result::{BootstrapResult, parse_result};
use amiss_fixtures::requests::SealedRequests;
use amiss_fixtures::{CommitChain, report_bytes};
use amiss_wire::controls::{
    ActionBootstrapContract, ExecutionConstraintDescriptor, ExecutionConstraintSchema,
    TrustedTimeController, TrustedTimeSchema, TrustedTimeStatement, canonical_execution_constraint,
    canonical_organization_floor, canonical_trusted_time,
};
use amiss_wire::digest::{Digest, hb};
use amiss_wire::model::{ObjectFormat, Oid, RepoPathText, RepositoryIdentity};
use amiss_wire::report::model;
use amiss_wire::requests::{
    CandidateSnapshot, GitSnapshotIdentity, GitSnapshotKind, REQUEST_STREAM_BYTES, RequestTrust,
    SEALED_ENGINE_ARGUMENT, commit_candidate_identity_digest,
};

use support::release::{Release, release, release_with_engine};

const INSTANT: &str = "2026-07-12T10:00:00Z";
const VALID_UNTIL: &str = "2026-07-12T10:05:00Z";
const ABSENT_COMMIT: &str = "6666666666666666666666666666666666666666";

/// One binary, two roles: the scenario runner, and the engine the wrapper
/// launches from the validated tree when spawned with the sealed argument.
#[expect(
    clippy::print_stdout,
    reason = "the harness-free test protocol speaks on stdout"
)]
fn main() -> ExitCode {
    if std::env::args_os().nth(1).as_deref() == Some(OsStr::new(SEALED_ENGINE_ARGUMENT)) {
        return engine();
    }
    if std::env::args_os().any(|argument| argument == OsStr::new("--list")) {
        if !std::env::args_os().any(|argument| argument == OsStr::new("--ignored")) {
            println!("wrapper: test");
        }
        return ExitCode::SUCCESS;
    }
    let own = fs::read(std::env::current_exe().expect("own path")).expect("own bytes");
    let staged = release_with_engine(&own, |_root| {});
    let refused = release(|_root| {});
    engine_output(&staged);
    block_run(&staged);
    absent_candidate(&refused);
    identity_absent(&refused);
    invalid_supplied_controls(&staged);
    semantic::capture(&staged);
    invalid_invocation_writes_nothing(&refused, "result2", false, "wrong result name");
    #[cfg(unix)]
    invalid_invocation_writes_nothing(&refused, "result", true, "symlinked scratch");
    request_ceiling(&staged);
    unread_requests(&staged);
    println!("wrapper: every scenario held");
    ExitCode::SUCCESS
}

/// The engine role: drain stdin unless told not to, replay the planted
/// stdout, exit with the planted class. The plants live in the repository
/// the wrapper set as the working directory.
fn engine() -> ExitCode {
    if !Path::new("engine-skip-stdin").exists() {
        let mut sink = Vec::new();
        let _drained = std::io::stdin().lock().read_to_end(&mut sink);
    }
    let planted = fs::read("engine-stdout").unwrap_or_default();
    let mut stdout = std::io::stdout().lock();
    if stdout
        .write_all(&planted)
        .and_then(|()| stdout.flush())
        .is_err()
    {
        return ExitCode::from(9);
    }
    fs::read_to_string("engine-exit")
        .ok()
        .and_then(|raw| raw.trim().parse::<u8>().ok())
        .map_or(ExitCode::from(9), ExitCode::from)
}

fn wrapper_constraint(staged: &Release) -> ExecutionConstraintDescriptor {
    let own = fs::read(env!("CARGO_BIN_EXE_amiss-bootstrap")).unwrap();
    ExecutionConstraintDescriptor {
        action_commit_oid: Oid::new(ObjectFormat::Sha1, staged.commit.clone()).unwrap(),
        action_object_format: ObjectFormat::Sha1,
        action_repository: RepositoryIdentity::new(
            "git.example.internal".to_owned(),
            "platform/security".to_owned(),
            "amiss".to_owned(),
        )
        .unwrap(),
        action_tree_oid: Oid::new(ObjectFormat::Sha1, staged.tree.clone()).unwrap(),
        bootstrap_contract: ActionBootstrapContract::Current,
        bootstrap_digest: hb(amiss_bootstrap::BOOTSTRAP_DOMAIN, &own),
        manifest_path: "release-manifest.json".parse().unwrap(),
        release_manifest_digest: staged.manifest_digest,
        required_status_name: "amiss / assure".to_owned(),
        schema: ExecutionConstraintSchema::Current,
        selected_platform: staged.platform,
    }
}

struct Run {
    repository: CommitChain,
    requests: SealedRequests,
    controls_input: Option<Vec<u8>>,
    report: model::ReportEnvelope,
}

static REPORT: LazyLock<model::ReportEnvelope> =
    LazyLock::new(|| serde_json::from_slice(amiss_fixtures::SCANNER_REPORT).unwrap());

fn sealed_run(staged: &Release) -> Run {
    let repository = amiss_fixtures::commit_chain(&[
        ("base", &[("doc.md", "# base\n")]),
        ("candidate", &[("doc.md", "# candidate\n")]),
    ])
    .expect("a commit chain");
    let [base, candidate] = repository.commits.as_slice() else {
        panic!("a two-commit fixture");
    };
    let mut requests = SealedRequests::new(wrapper_constraint(staged));
    let format = requests.evaluation.object_format;
    requests.evaluation.base_commit = Oid::new(format, base.id.clone()).unwrap();
    requests.evaluation.candidate_commit = Some(Oid::new(format, candidate.id.clone()).unwrap());
    let report = bind_envelope(staged, &mut requests, &repository);
    Run {
        repository,
        requests,
        controls_input: None,
        report,
    }
}

fn bind_statement(requests: &mut SealedRequests, identity: Digest) {
    let request = &requests.evaluation;
    let time = requests
        .controls
        .trusted_time
        .as_mut()
        .expect("supplied time");
    time.value = TrustedTimeStatement {
        candidate_identity_digest: identity,
        controller: TrustedTimeController::ExternalRequiredCheckClock,
        evaluation_instant: INSTANT.to_owned().try_into().unwrap(),
        provider: time.provider.clone(),
        provider_run_attempt: time.provider_run_attempt,
        provider_run_id: time.provider_run_id.clone(),
        ref_name: request.target_ref.clone().expect("a target"),
        repository: request.repository.clone().expect("an identity"),
        schema: TrustedTimeSchema::Current,
        valid_until: VALID_UNTIL.to_owned().try_into().unwrap(),
    };
    time.expected_digest = canonical_trusted_time(&time.value).unwrap().1;
}

fn bind_envelope(
    staged: &Release,
    requests: &mut SealedRequests,
    repository: &CommitChain,
) -> model::ReportEnvelope {
    let [base, candidate] = repository.commits.as_slice() else {
        panic!("a two-commit fixture");
    };
    let format = requests.evaluation.object_format;
    let base_tree = Oid::new(format, base.tree.clone()).unwrap();
    let candidate_tree = Oid::new(format, candidate.tree.clone()).unwrap();
    let identity =
        commit_candidate_identity_digest(&requests.evaluation, &base_tree, &candidate_tree)
            .expect("a commit-pair identity");
    bind_statement(requests, identity);
    let time = requests
        .controls
        .trusted_time
        .as_ref()
        .expect("supplied time");
    let request = &requests.evaluation;
    let mut report = REPORT.clone();
    report.payload.engine.engine_digest = staged.engine_digest;
    let model::Evaluation::Resolved(evaluation) = &mut report.payload.evaluation else {
        panic!("a resolved evaluation");
    };
    evaluation.repository.clone_from(&request.repository);
    evaluation.forge = request.forge;
    evaluation.candidate_ref.clone_from(&request.candidate_ref);
    evaluation.target_ref.clone_from(&request.target_ref);
    evaluation
        .default_branch_ref
        .clone_from(&request.default_branch_ref);
    evaluation.base = model::BaseSnapshot::Git(GitSnapshotIdentity {
        commit_oid: request.base_commit.clone(),
        kind: GitSnapshotKind::GitCommit,
        object_format: format,
        tree_oid: base_tree,
    });
    evaluation.candidate =
        model::Snapshot::Available(CandidateSnapshot::Git(GitSnapshotIdentity {
            commit_oid: request.candidate_commit.clone().expect("a candidate"),
            kind: GitSnapshotKind::GitCommit,
            object_format: format,
            tree_oid: candidate_tree,
        }));
    evaluation.evaluation_instant = Some(time.value.evaluation_instant.clone());
    evaluation.trusted_time = true;
    let floor = requests
        .controls
        .organization_floor
        .as_ref()
        .expect("a floor");
    let supplied = requests
        .controls
        .execution_constraint
        .as_ref()
        .expect("a constraint");
    let model::Controls::Resolved(controls) = &mut report.payload.controls else {
        panic!("resolved controls");
    };
    controls.profile = request.profile;
    controls.organization_floor = model::ControlProvenance {
        digest: Some(floor.expected_digest),
        status: model::ControlStatus::Verified,
        trust_source: model::ControlTrustSource::Verified(floor.trust_source),
    };
    controls.execution_constraint = model::ExecutionConstraintProvenance::Verified(Box::new(
        model::VerifiedExecutionConstraint {
            descriptor: supplied.value.clone(),
            descriptor_digest: canonical_execution_constraint(&requests.constraint)
                .unwrap()
                .1,
            status: model::VerifiedControlStatus::Verified,
            trust_source: supplied.trust_source,
        },
    ));
    controls.trusted_time_source =
        model::TrustedTimeProvenance::Verified(Box::new(model::VerifiedTrustedTime {
            statement: time.value.clone(),
            statement_digest: time.expected_digest,
            status: model::VerifiedControlStatus::Verified,
            trust_source: model::TrustedTimeTrustSource::ExternalRequiredCheck,
        }));
    report
}

struct Invocation {
    output: Output,
    report: PathBuf,
    result: PathBuf,
    _scratch: tempfile::TempDir,
}

fn invoke(staged: &Release, run: &Run, result_name: &str, scratch_link: bool) -> Invocation {
    let scratch = tempfile::tempdir().expect("a scratch root");
    let paths = run.requests.write(scratch.path());
    if let Some(bytes) = &run.controls_input {
        fs::write(&paths.controls, bytes).unwrap();
    }
    let report = scratch.path().join("report");
    let result = scratch.path().join(result_name);
    fs::write(&report, b"").unwrap();
    fs::write(&result, b"").unwrap();
    let scratch_argument = if scratch_link {
        let link = scratch.path().with_extension("link");
        #[cfg(unix)]
        std::os::unix::fs::symlink(scratch.path(), &link).expect("a scratch symlink");
        link
    } else {
        scratch.path().to_path_buf()
    };
    let report_argument = scratch_argument.join("report");
    let result_argument = scratch_argument.join(result_name);
    let output = Command::new(env!("CARGO_BIN_EXE_amiss-bootstrap"))
        .arg("exec")
        .arg("--action-repository")
        .arg(staged.dir.path())
        .arg("--repository")
        .arg(run.repository.root())
        .arg("--constraint")
        .arg(&paths.constraint)
        .arg("--evaluation-request")
        .arg(&paths.evaluation)
        .arg("--snapshot-request")
        .arg(&paths.snapshot)
        .arg("--controls-request")
        .arg(&paths.controls)
        .arg("--scratch")
        .arg(&scratch_argument)
        .arg("--report")
        .arg(&report_argument)
        .arg("--result")
        .arg(&result_argument)
        .output()
        .expect("the wrapper runs");
    if scratch_link {
        let _removed = fs::remove_file(scratch_argument);
    }
    Invocation {
        output,
        report,
        result,
        _scratch: scratch,
    }
}

fn plant(run: &Run, stdout: &[u8], exit: &str) {
    fs::write(run.repository.root().join("engine-stdout"), stdout).unwrap();
    fs::write(run.repository.root().join("engine-exit"), exit).unwrap();
}

fn stderr_names(invocation: &Invocation, diagnostic: &str, scenario: &str) {
    let stderr = String::from_utf8_lossy(&invocation.output.stderr);
    assert!(
        stderr.contains(diagnostic),
        "{scenario}: stderr {stderr:?} does not name {diagnostic}"
    );
}

fn engine_output(staged: &Release) {
    for floor_source in [
        RequestTrust::ExternalRequiredCheck,
        RequestTrust::OrganizationPolicy,
    ] {
        for constraint_source in [
            RequestTrust::ExternalRequiredCheck,
            RequestTrust::OrganizationPolicy,
        ] {
            let mut run = sealed_run(staged);
            run.requests
                .controls
                .organization_floor
                .as_mut()
                .unwrap()
                .trust_source = floor_source;
            run.requests
                .controls
                .execution_constraint
                .as_mut()
                .unwrap()
                .trust_source = constraint_source;
            run.report = bind_envelope(staged, &mut run.requests, &run.repository);
            let wire = report_bytes(run.report.clone()).unwrap();
            for (stdout, expected, exit, published, diagnostic) in [
                (
                    wire.as_slice(),
                    BootstrapResult::Pass,
                    0,
                    wire.as_slice(),
                    "",
                ),
                (
                    b"".as_slice(),
                    BootstrapResult::MissingOutput,
                    2,
                    b"".as_slice(),
                    "report-missing",
                ),
                (
                    b"not an envelope\n".as_slice(),
                    BootstrapResult::TamperedRuntime,
                    2,
                    b"".as_slice(),
                    "report-rejected",
                ),
            ] {
                plant(&run, stdout, "0");
                let invocation = invoke(staged, &run, "result", false);
                assert_eq!(invocation.output.status.code(), Some(exit));
                assert_eq!(
                    parse_result(&fs::read(&invocation.result).unwrap()),
                    Some(expected)
                );
                assert_eq!(fs::read(&invocation.report).unwrap(), published);
                let stderr = String::from_utf8_lossy(&invocation.output.stderr);
                assert_eq!(stderr.is_empty(), diagnostic.is_empty());
                assert!(stderr.contains(diagnostic), "{stderr:?}");
            }
        }
    }
}

fn block_run(staged: &Release) {
    let mut run = sealed_run(staged);
    run.report.payload.result.exit_code = 1;
    run.report.payload.result.status = model::ReportStatus::Fail;
    plant(&run, &report_bytes(run.report.clone()).unwrap(), "1");
    let invocation = invoke(staged, &run, "result", false);
    assert_eq!(
        invocation.output.status.code(),
        Some(1),
        "a block run exits one"
    );
    assert_eq!(
        parse_result(&fs::read(&invocation.result).unwrap()),
        Some(BootstrapResult::Block)
    );
}

fn absent_candidate(staged: &Release) {
    let mut run = sealed_run(staged);
    let format = run.requests.evaluation.object_format;
    run.requests.evaluation.candidate_commit =
        Some(Oid::new(format, ABSENT_COMMIT.to_owned()).unwrap());
    run.report = bind_envelope(staged, &mut run.requests, &run.repository);
    plant(&run, &report_bytes(run.report.clone()).unwrap(), "0");
    let invocation = invoke(staged, &run, "result", false);
    assert_eq!(invocation.output.status.code(), Some(2));
    assert_eq!(
        parse_result(&fs::read(&invocation.result).unwrap()),
        Some(BootstrapResult::Unavailable)
    );
    stderr_names(
        &invocation,
        "repository-not-pre-acquired",
        "absent candidate",
    );
}

/// The request grammar makes the four identity fields all-or-nothing, so the
/// only sealed-identity gap a parsed request can carry is an absent forge.
fn identity_absent(staged: &Release) {
    let mut run = sealed_run(staged);
    run.requests.evaluation.forge = None;
    plant(&run, &report_bytes(run.report.clone()).unwrap(), "0");
    let invocation = invoke(staged, &run, "result", false);
    assert_eq!(
        parse_result(&fs::read(&invocation.result).unwrap()),
        Some(BootstrapResult::TamperedRuntime)
    );
    stderr_names(&invocation, "evaluation-identity-absent", "absent forge");
}

fn invalid_supplied_controls(staged: &Release) {
    let mut constraint = sealed_run(staged);
    let controls = &mut constraint.requests.controls;
    " bad".clone_into(
        &mut controls
            .execution_constraint
            .as_mut()
            .unwrap()
            .value
            .required_status_name,
    );
    let mut provider = sealed_run(staged);
    let controls = &mut provider.requests.controls;
    "bad provider!".clone_into(&mut controls.trusted_time.as_mut().unwrap().value.provider);
    let mut lifetime = sealed_run(staged);
    let controls = &mut lifetime.requests.controls;
    let statement = &mut controls.trusted_time.as_mut().unwrap().value;
    statement.valid_until = statement.evaluation_instant.clone();
    for (run, field, diagnostic) in [
        (
            constraint,
            "required_status_name",
            "execution-constraint-invalid",
        ),
        (provider, "provider", "trusted-time-invalid"),
        (lifetime, "valid_until", "trusted-time-invalid"),
    ] {
        plant(&run, &report_bytes(run.report.clone()).unwrap(), "0");
        let invocation = invoke(staged, &run, "result", false);
        assert_eq!(invocation.output.status.code(), Some(2), "{field}");
        assert_eq!(
            parse_result(&fs::read(&invocation.result).unwrap()),
            Some(BootstrapResult::TamperedRuntime)
        );
        assert!(fs::read(&invocation.report).unwrap().is_empty(), "{field}");
        stderr_names(&invocation, diagnostic, field);
    }
}

fn invalid_invocation_writes_nothing(
    staged: &Release,
    result_name: &str,
    scratch_link: bool,
    scenario: &str,
) {
    let run = sealed_run(staged);
    plant(&run, &report_bytes(run.report.clone()).unwrap(), "0");
    let invocation = invoke(staged, &run, result_name, scratch_link);
    assert_eq!(invocation.output.status.code(), Some(2), "{scenario}");
    stderr_names(&invocation, "invalid-invocation", scenario);
    assert!(
        fs::read(&invocation.result).unwrap().is_empty(),
        "{scenario}: the result is never written"
    );
    assert!(
        fs::read(&invocation.report).unwrap().is_empty(),
        "{scenario}"
    );
}

/// Grows valid protected paths until the canonical request is exactly `target` bytes long.
fn inflate_controls(staged: &Release, run: &mut Run, target: u64) {
    let floor = run
        .requests
        .controls
        .organization_floor
        .as_mut()
        .expect("a floor");
    floor.value.protected_inventory.clear();
    let measured = run
        .requests
        .controls
        .canonical_bytes()
        .expect("controls serialize")
        .len();
    let grow = usize::try_from(target)
        .unwrap()
        .checked_sub(measured)
        .expect("a growable request");
    let count = grow.div_ceil(1024);
    let text_bytes = grow
        .checked_add(1)
        .unwrap()
        .checked_sub(count.checked_mul(3).unwrap())
        .unwrap();
    let path_length = text_bytes.checked_div(count).unwrap();
    let longer_paths = text_bytes.checked_rem(count).unwrap();
    let floor = run
        .requests
        .controls
        .organization_floor
        .as_mut()
        .expect("a floor");
    floor.value.protected_inventory = (0..count)
        .map(|index| {
            let length = path_length
                .checked_add(usize::from(index < longer_paths))
                .unwrap();
            RepoPathText::new(format!(
                "{index:06}{}",
                "x".repeat(length.checked_sub(6).unwrap())
            ))
            .unwrap()
        })
        .collect();
    floor.expected_digest = canonical_organization_floor(&floor.value).unwrap().1;
    run.report = bind_envelope(staged, &mut run.requests, &run.repository);
    let sized = run
        .requests
        .controls
        .canonical_bytes()
        .expect("controls serialize");
    assert_eq!(u64::try_from(sized.len()).unwrap(), target);
}

fn request_ceiling(staged: &Release) {
    let mut run = sealed_run(staged);
    inflate_controls(staged, &mut run, REQUEST_STREAM_BYTES);
    plant(&run, &report_bytes(run.report.clone()).unwrap(), "0");
    let invocation = invoke(staged, &run, "result", false);
    assert_eq!(
        parse_result(&fs::read(&invocation.result).unwrap()),
        Some(BootstrapResult::Pass),
        "a controls request exactly at the stream ceiling is read whole"
    );

    let mut over = sealed_run(staged);
    inflate_controls(staged, &mut over, REQUEST_STREAM_BYTES + 1);
    plant(&over, &report_bytes(over.report.clone()).unwrap(), "0");
    let invocation = invoke(staged, &over, "result", false);
    assert_eq!(
        parse_result(&fs::read(&invocation.result).unwrap()),
        Some(BootstrapResult::TamperedRuntime)
    );
    stderr_names(&invocation, "controls-request-invalid", "over the ceiling");
}

/// An engine that never reads its requests while they overflow the pipe
/// buffer fails the request writer, and a completed engine must not have
/// that failure forgiven.
fn unread_requests(staged: &Release) {
    let mut run = sealed_run(staged);
    inflate_controls(staged, &mut run, REQUEST_STREAM_BYTES);
    plant(&run, &report_bytes(run.report.clone()).unwrap(), "0");
    fs::write(run.repository.root().join("engine-skip-stdin"), b"").unwrap();
    let invocation = invoke(staged, &run, "result", false);
    assert_eq!(
        parse_result(&fs::read(&invocation.result).unwrap()),
        Some(BootstrapResult::Unavailable)
    );
    stderr_names(&invocation, "engine-collection-failed", "unread requests");
}
