use std::collections::BTreeSet;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use amiss_scan::SetupShell;
use amiss_scan::policy::{
    ConstraintInput, DebtInput, FloorInput, TimeInput, TrustSource, WaiverInput,
};
use amiss_scan::report::RequestDigests;
use amiss_wire::ExitClass;
use amiss_wire::controls::{
    DebtSnapshot, ExecutionConstraintDescriptor, FloorDefect, OrganizationFloor,
    TrustedTimeStatement, WaiverBundle,
};
use amiss_wire::de::ErrorKind;
use amiss_wire::digest::{Digest, hb};
use amiss_wire::json::canonical;
use amiss_wire::report::{
    AnalysisErrorCode, EngineProvenance, ErrorDetail, FatalSerializer,
    unavailable_evaluation_envelope,
};
use amiss_wire::requests::{
    CONTROLS_REQUEST_SCHEMA, ControlsRequest, EVALUATION_REQUEST_SCHEMA, EvaluationRequest,
    REQUEST_STREAM_BYTES, RequestMode, RequestTrust, SNAPSHOT_REQUEST_SCHEMA, SnapshotRequest,
    SuppliedControl,
};

/// The experimental request-wire launcher: it composes nothing and
/// authenticates nothing. It reads the three request streams, runs the
/// engine in-process, applies the acceptance law to the produced envelope,
/// and publishes it. Nothing on this lane is a required check.
#[expect(clippy::print_stderr, reason = "contract diagnostics channel")]
fn main() -> ExitCode {
    let mut reserve = FatalSerializer::new();
    let argv: Vec<OsString> = env::args_os().skip(1).collect();
    let failure = ExitCode::from(ExitClass::Failure.code());
    let Some(engine) = engine_provenance() else {
        eprintln!(
            "amiss-wrapper: {}",
            AnalysisErrorCode::InternalError.as_str()
        );
        return failure;
    };
    let Ok(parsed) = parse_args(&argv) else {
        let codes: BTreeSet<AnalysisErrorCode> =
            [AnalysisErrorCode::InvalidInvocation].into_iter().collect();
        let Some(envelope) = unavailable_evaluation_envelope(&engine, &codes, None, None) else {
            eprintln!(
                "amiss-wrapper: {}",
                AnalysisErrorCode::ReportConstructionFailed.as_str()
            );
            return failure;
        };
        emit_to(None, &reserve.wire_bytes(&envelope));
        return failure;
    };
    if env::var_os(EVALUATOR_SENTINEL).is_some_and(|value| value == "1") {
        run_evaluator(&engine, &parsed, &mut reserve)
    } else {
        run(&engine, &parsed, &mut reserve)
    }
}

const EVALUATOR_SENTINEL: &str = "AMISS_WRAPPER_EVALUATOR";
const WATCHDOG_CEILING: std::time::Duration = std::time::Duration::from_mins(2);
const SANDBOX_MEMORY_BYTES: u64 = 1_073_741_824;

struct Args {
    repository: PathBuf,
    evaluation: PathBuf,
    snapshot: PathBuf,
    controls: PathBuf,
    output: Option<PathBuf>,
}

fn parse_args(argv: &[OsString]) -> Result<Args, ()> {
    let mut repository: Option<PathBuf> = None;
    let mut evaluation: Option<PathBuf> = None;
    let mut snapshot: Option<PathBuf> = None;
    let mut controls: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut items = argv.iter();
    if items.next().map(OsString::as_os_str) != Some("check".as_ref()) {
        return Err(());
    }
    while let Some(flag) = items.next() {
        let value = items.next().ok_or(())?;
        let slot = match flag.to_str() {
            Some("--repository") => &mut repository,
            Some("--evaluation-request") => &mut evaluation,
            Some("--snapshot-request") => &mut snapshot,
            Some("--controls-request") => &mut controls,
            Some("--output") => &mut output,
            _ => return Err(()),
        };
        if slot.is_some() {
            return Err(());
        }
        *slot = Some(PathBuf::from(value));
    }
    Ok(Args {
        repository: repository.ok_or(())?,
        evaluation: evaluation.ok_or(())?,
        snapshot: snapshot.ok_or(())?,
        controls: controls.ok_or(())?,
        output,
    })
}

/// One complete bounded capture from byte zero through EOF: the diagnostic
/// digest exists exactly when EOF was obtained within the cap.
fn capture(path: &Path, domain: &'static str) -> (Option<Digest>, Option<Vec<u8>>) {
    let Ok(file) = fs::File::open(path) else {
        return (None, None);
    };
    let mut bytes = Vec::new();
    let mut bounded = file.take(REQUEST_STREAM_BYTES.saturating_add(1));
    if bounded.read_to_end(&mut bytes).is_err() {
        return (None, None);
    }
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > REQUEST_STREAM_BYTES {
        return (None, None);
    }
    let digest = hb(domain, &bytes);
    (Some(digest), Some(bytes))
}

/// The captured request trio with each stream's diagnostic digest.
struct Captured {
    digests: RequestDigests,
    evaluation: Option<Vec<u8>>,
    snapshot: Option<Vec<u8>>,
    controls: Option<Vec<u8>>,
}

fn capture_all(args: &Args) -> Captured {
    let (evaluation_digest, evaluation) = capture(&args.evaluation, EVALUATION_REQUEST_SCHEMA);
    let (snapshot_digest, snapshot) = capture(&args.snapshot, SNAPSHOT_REQUEST_SCHEMA);
    let (controls_digest, controls) = capture(&args.controls, CONTROLS_REQUEST_SCHEMA);
    Captured {
        digests: RequestDigests {
            evaluation: evaluation_digest,
            snapshot: snapshot_digest,
            controls: controls_digest,
        },
        evaluation,
        snapshot,
        controls,
    }
}

/// The invocation-phase code one request defect anchors: consistency
/// defects are invalid invocations, a defective profile value names itself,
/// and every capture or parse defect is the unreadable request.
fn request_code(defect: &amiss_wire::de::Error) -> AnalysisErrorCode {
    if defect.kind == ErrorKind::Inconsistent {
        AnalysisErrorCode::InvalidInvocation
    } else if defect.path == "$.profile" {
        AnalysisErrorCode::InvalidProfile
    } else {
        AnalysisErrorCode::RequestUnreadable
    }
}

/// The parsed request trio after the cross-request pairing law.
struct Requests {
    evaluation: EvaluationRequest,
    controls: ControlsRequest,
}

fn parse_requests(captured: &Captured) -> Result<Requests, BTreeSet<AnalysisErrorCode>> {
    let mut codes: BTreeSet<AnalysisErrorCode> = BTreeSet::new();
    let evaluation = match &captured.evaluation {
        None => {
            codes.insert(AnalysisErrorCode::RequestUnreadable);
            None
        }
        Some(bytes) => match EvaluationRequest::parse(bytes) {
            Ok(request) => Some(request),
            Err(defect) => {
                codes.insert(request_code(&defect));
                None
            }
        },
    };
    let snapshot = match &captured.snapshot {
        None => {
            codes.insert(AnalysisErrorCode::RequestUnreadable);
            None
        }
        Some(bytes) => match SnapshotRequest::parse(bytes) {
            Ok(request) => Some(request),
            Err(defect) => {
                codes.insert(request_code(&defect));
                None
            }
        },
    };
    let controls = match &captured.controls {
        None => {
            codes.insert(AnalysisErrorCode::RequestUnreadable);
            None
        }
        Some(bytes) => match ControlsRequest::parse(bytes) {
            Ok(request) => Some(request),
            Err(defect) => {
                codes.insert(request_code(&defect));
                None
            }
        },
    };
    match (evaluation, snapshot, controls) {
        (Some(evaluation), Some(snapshot), Some(controls)) if codes.is_empty() => {
            if snapshot.materialization == evaluation.mode {
                Ok(Requests {
                    evaluation,
                    controls,
                })
            } else {
                Err([AnalysisErrorCode::InvalidInvocation].into_iter().collect())
            }
        }
        _ => Err(codes),
    }
}

const fn trust(source: RequestTrust) -> TrustSource {
    match source {
        RequestTrust::ExternalRequiredWorkflow => TrustSource::ExternalRequiredWorkflow,
        RequestTrust::OrganizationRuleset => TrustSource::OrganizationRuleset,
    }
}

/// One verified embedded control: its canonical bytes under the effective
/// raw ceiling, the typed parse, and expected-digest equality.
fn verified_bytes(
    supplied: &SuppliedControl,
    cap: u64,
) -> Result<Vec<u8>, (&'static str, ErrorDetail)> {
    let bytes = canonical(&supplied.value);
    let declared = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if declared > cap {
        return Err((
            "not-parsed",
            ErrorDetail {
                code: AnalysisErrorCode::ResourceLimitExceeded,
                path: None,
                resource: Some((
                    amiss_wire::controls::ResourceName::ControlInputBytes,
                    cap,
                    declared,
                )),
            },
        ));
    }
    Ok(bytes)
}

fn schema_defect(kind: &ErrorKind) -> (&'static str, ErrorDetail) {
    let code = match kind {
        ErrorKind::Json(_)
        | ErrorKind::MissingField
        | ErrorKind::UnknownField
        | ErrorKind::WrongType
        | ErrorKind::InvalidValue
        | ErrorKind::UnsortedSet
        | ErrorKind::DuplicateMember
        | ErrorKind::LimitExceeded
        | ErrorKind::Inconsistent => AnalysisErrorCode::ConfigurationInvalid,
        ErrorKind::DigestMismatch => AnalysisErrorCode::DigestMismatch,
    };
    (
        "invalid-external-control",
        ErrorDetail {
            code,
            path: None,
            resource: None,
        },
    )
}

const fn digest_mismatch() -> (&'static str, ErrorDetail) {
    (
        "invalid-external-control",
        ErrorDetail {
            code: AnalysisErrorCode::DigestMismatch,
            path: None,
            resource: None,
        },
    )
}

/// The wrapper-side control verification in the fatal order: constraint,
/// floor, trusted time, debt, waiver. Every recomputed semantic digest must
/// equal its expected digest, and debt and waiver parse under the verified
/// floor's effective raw ceiling.
struct VerifiedControls {
    floor: Option<FloorInput>,
    debt: Option<DebtInput>,
    waiver: Option<WaiverInput>,
    time: Option<TimeInput>,
    constraint: Option<ConstraintInput>,
}

fn verify_controls(
    request: &ControlsRequest,
) -> Result<VerifiedControls, (&'static str, ErrorDetail)> {
    let (constraint, floor) = verify_static(request)?;
    let built_in = amiss_scan::ScanLimits::CONTRACT.control_input_bytes;
    let effective = floor.as_ref().map_or(built_in, |input| {
        let (scan, _git) = amiss_scan::policy::tightened_limits(
            amiss_scan::ScanLimits::CONTRACT,
            amiss_git::GitLimits::CONTRACT,
            &input.floor,
        );
        scan.control_input_bytes
    });
    let (time, debt, waiver) = verify_exceptions(request, effective)?;
    Ok(VerifiedControls {
        floor,
        debt,
        waiver,
        time,
        constraint,
    })
}

/// The pre-snapshot controls in the fatal order: the execution constraint,
/// then the floor whose verified value tightens later control ceilings.
type StaticControls = (Option<ConstraintInput>, Option<FloorInput>);

fn verify_static(request: &ControlsRequest) -> Result<StaticControls, (&'static str, ErrorDetail)> {
    let built_in = amiss_scan::ScanLimits::CONTRACT.control_input_bytes;
    let constraint = match &request.execution_constraint {
        None => None,
        Some(supplied) => {
            let bytes = verified_bytes(supplied, built_in)?;
            let descriptor = ExecutionConstraintDescriptor::parse(&bytes)
                .map_err(|defect| schema_defect(&defect.kind))?;
            if descriptor.digest != supplied.expected_digest {
                return Err(digest_mismatch());
            }
            Some(ConstraintInput {
                descriptor,
                trust_source: trust(supplied.trust_source),
            })
        }
    };
    let floor = match &request.organization_floor {
        None => None,
        Some(supplied) => {
            let bytes = verified_bytes(supplied, built_in)?;
            let floor = OrganizationFloor::parse(&bytes).map_err(|defect| match defect {
                FloorDefect::Schema(error) => schema_defect(&error.kind),
                FloorDefect::Entries {
                    configured_limit,
                    observed_lower_bound,
                } => (
                    "not-parsed",
                    ErrorDetail {
                        code: AnalysisErrorCode::ResourceLimitExceeded,
                        path: None,
                        resource: Some((
                            amiss_wire::controls::ResourceName::OrganizationPolicyEntries,
                            configured_limit,
                            observed_lower_bound,
                        )),
                    },
                ),
            })?;
            if floor.digest != supplied.expected_digest {
                return Err(digest_mismatch());
            }
            Some(FloorInput {
                floor,
                trust_source: trust(supplied.trust_source),
            })
        }
    };
    Ok((constraint, floor))
}

/// The expiry-bearing controls under the effective ceiling: trusted time,
/// debt, then waiver.
type ExceptionControls = (Option<TimeInput>, Option<DebtInput>, Option<WaiverInput>);

fn verify_exceptions(
    request: &ControlsRequest,
    effective: u64,
) -> Result<ExceptionControls, (&'static str, ErrorDetail)> {
    let time = match &request.trusted_time {
        None => None,
        Some(supplied) => {
            let bytes = canonical(&supplied.value);
            if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > effective {
                return Err((
                    "not-parsed",
                    ErrorDetail {
                        code: AnalysisErrorCode::ResourceLimitExceeded,
                        path: None,
                        resource: Some((
                            amiss_wire::controls::ResourceName::ControlInputBytes,
                            effective,
                            u64::try_from(bytes.len()).unwrap_or(u64::MAX),
                        )),
                    },
                ));
            }
            let statement = TrustedTimeStatement::parse(&bytes)
                .map_err(|defect| schema_defect(&defect.kind))?;
            if statement.digest != supplied.expected_digest {
                return Err(digest_mismatch());
            }
            Some(TimeInput {
                statement,
                provider_run_id: supplied.provider_run_id.clone(),
                provider_run_attempt: supplied.provider_run_attempt,
            })
        }
    };
    let debt = match &request.debt_snapshot {
        None => None,
        Some(supplied) => {
            let bytes = verified_bytes(supplied, effective)?;
            let snapshot =
                DebtSnapshot::parse(&bytes).map_err(|defect| schema_defect(&defect.kind))?;
            if snapshot.digest != supplied.expected_digest {
                return Err(digest_mismatch());
            }
            Some(DebtInput {
                snapshot,
                trust_source: trust(supplied.trust_source),
            })
        }
    };
    let waiver = match &request.waiver_bundle {
        None => None,
        Some(supplied) => {
            let bytes = verified_bytes(supplied, effective)?;
            let bundle =
                WaiverBundle::parse(&bytes).map_err(|defect| schema_defect(&defect.kind))?;
            if bundle.digest != supplied.expected_digest {
                return Err(digest_mismatch());
            }
            Some(WaiverInput {
                bundle,
                trust_source: trust(supplied.trust_source),
            })
        }
    };
    Ok((time, debt, waiver))
}

fn parse_gate(
    engine: &EngineProvenance,
    captured: &Captured,
    args: &Args,
    reserve: &mut FatalSerializer,
) -> Result<Requests, ExitCode> {
    match parse_requests(captured) {
        Ok(requests) => Ok(requests),
        Err(codes) => {
            let codes = if codes.is_empty() {
                [AnalysisErrorCode::RequestUnreadable].into_iter().collect()
            } else {
                codes
            };
            Err(request_failure(
                engine,
                &codes,
                &captured.digests,
                args.output.as_deref(),
                reserve,
            ))
        }
    }
}

/// The supervisor entry: capture and validate the requests in-process, then
/// run the evaluation out of process under the operational watchdog.
fn run(engine: &EngineProvenance, args: &Args, reserve: &mut FatalSerializer) -> ExitCode {
    let captured = capture_all(args);
    let requests = match parse_gate(engine, &captured, args, reserve) {
        Ok(requests) => requests,
        Err(exit) => return exit,
    };
    supervise_evaluator(engine, args, &captured, &requests)
}

/// The evaluator half: sandbox self-restriction, request verification, the
/// engine, and one envelope written to the supervisor's private report
/// path. Acceptance never happens here.
#[expect(clippy::print_stderr, reason = "contract diagnostics channel")]
fn run_evaluator(
    engine: &EngineProvenance,
    args: &Args,
    reserve: &mut FatalSerializer,
) -> ExitCode {
    apply_sandbox();
    let failure = ExitCode::from(ExitClass::Failure.code());
    let captured = capture_all(args);
    let requests = match parse_gate(engine, &captured, args, reserve) {
        Ok(requests) => requests,
        Err(exit) => return exit,
    };

    let (controls, external_defect) = match verify_controls(&requests.controls) {
        Ok(controls) => (controls, None),
        Err(defect) => (
            VerifiedControls {
                floor: None,
                debt: None,
                waiver: None,
                time: None,
                constraint: None,
            },
            Some(defect),
        ),
    };
    let evaluation = &requests.evaluation;
    let repo = match amiss_git::Repository::open(&args.repository, evaluation.object_format) {
        Ok(repo) => repo,
        Err(defect) => {
            return repository_failure(
                engine,
                evaluation,
                &captured.digests,
                &defect,
                args,
                reserve,
            );
        }
    };
    let github = build_github(evaluation);
    let shell = SetupShell {
        engine: engine.clone(),
        enforce: matches!(evaluation.profile, amiss_wire::controls::Profile::Enforce),
        repository: evaluation
            .repository
            .as_ref()
            .map(|identity| (identity.owner.clone(), identity.name.clone())),
        candidate_ref: evaluation
            .ref_name
            .as_ref()
            .map(|reference| reference.as_str().to_owned()),
        default_branch_ref: evaluation
            .default_branch_ref
            .as_ref()
            .map(|reference| reference.as_str().to_owned()),
        floor: controls.floor,
        debt: controls.debt,
        waiver: controls.waiver,
        time: controls.time,
        constraint: controls.constraint,
        requests: captured.digests,
        external_defect,
        errors_retained: 64,
    };
    let built = match (&evaluation.mode, &evaluation.candidate_commit) {
        (RequestMode::CommitPair, Some(candidate)) => amiss_scan::pipeline::commit_pair(
            &repo,
            &shell.engine,
            github.as_ref(),
            &shell,
            &evaluation.base_commit,
            candidate,
        ),
        (RequestMode::Index, None) => amiss_scan::pipeline::staged_index(
            &repo,
            &shell.engine,
            github.as_ref(),
            &shell,
            &evaluation.base_commit,
        ),
        (RequestMode::CommitPair, None) | (RequestMode::Index, Some(_)) => {
            eprintln!(
                "amiss-wrapper: {}",
                AnalysisErrorCode::InternalError.as_str()
            );
            return failure;
        }
    };
    let wire = reserve.wire_bytes(&built.envelope);
    emit_to(args.output.as_deref(), &wire);
    exit_class(built.exit_code)
}

/// The acceptance law runs before any byte is published; a violation is a
/// failed construction with no accepted envelope.
/// The supervisor's publication gate: acceptance over the evaluator's exact
/// report bytes, plus the process-exit consistency law (the evaluator's exit
/// class must equal the accepted envelope's), then one canonical emission.
#[expect(clippy::print_stderr, reason = "contract diagnostics channel")]
fn publish_wire(
    engine: &EngineProvenance,
    requests: &Requests,
    wire: &[u8],
    evaluator: std::process::ExitStatus,
    output: Option<&Path>,
) -> ExitCode {
    let failure = ExitCode::from(ExitClass::Failure.code());
    let expectations = amiss_wrapper::Expectations {
        engine_digest: engine.digest.to_string(),
        base_commit: requests.evaluation.base_commit.as_str().to_owned(),
        candidate_commit: requests
            .evaluation
            .candidate_commit
            .as_ref()
            .map(|oid| oid.as_str().to_owned()),
        floor_digest: requests
            .controls
            .organization_floor
            .as_ref()
            .map(|floor| floor.expected_digest.to_string()),
    };
    let Ok(class) = amiss_wrapper::accept(wire, &expectations) else {
        eprintln!(
            "amiss-wrapper: {}",
            AnalysisErrorCode::ReportConstructionFailed.as_str()
        );
        return failure;
    };
    if evaluator.code() != i32::try_from(class).ok() {
        eprintln!("amiss-wrapper: evaluator-exit-mismatch");
        return failure;
    }
    emit_to(output, wire);
    exit_class(class)
}

/// The supervisor half: stage the captured request bytes into a private
/// directory, spawn this binary as the evaluator with a clean environment,
/// hold it to the operational wall ceiling, and publish only an accepted
/// envelope. A kill or crash yields no envelope; the run just fails.
#[expect(clippy::print_stderr, reason = "contract diagnostics channel")]
fn supervise_evaluator(
    engine: &EngineProvenance,
    args: &Args,
    captured: &Captured,
    requests: &Requests,
) -> ExitCode {
    let failure = ExitCode::from(ExitClass::Failure.code());
    let internal = || {
        eprintln!(
            "amiss-wrapper: {}",
            AnalysisErrorCode::InternalError.as_str()
        );
        failure
    };
    let Ok(private) = tempfile::TempDir::new() else {
        return internal();
    };
    let Ok(report) = stage_requests(captured, private.path()).and_then(|staged| {
        let mut child = spawn_evaluator(args, private.path(), &staged)?;
        match amiss_wrapper::supervise(&mut child, WATCHDOG_CEILING)? {
            amiss_wrapper::Supervised::Killed => {
                eprintln!("amiss-wrapper: evaluator-watchdog-kill");
                Err(std::io::Error::other("watchdog"))
            }
            amiss_wrapper::Supervised::Completed(status) => Ok((staged.report, status)),
        }
    }) else {
        return failure;
    };
    let (report_path, status) = report;
    let Ok(wire) = fs::read(report_path) else {
        eprintln!(
            "amiss-wrapper: {}",
            AnalysisErrorCode::ReportConstructionFailed.as_str()
        );
        return failure;
    };
    publish_wire(engine, requests, &wire, status, args.output.as_deref())
}

/// The staged evaluator inputs inside the supervisor's private directory.
struct Staged {
    evaluation: PathBuf,
    snapshot: PathBuf,
    controls: PathBuf,
    report: PathBuf,
}

fn stage_requests(captured: &Captured, dir: &Path) -> std::io::Result<Staged> {
    let (Some(evaluation), Some(snapshot), Some(controls)) =
        (&captured.evaluation, &captured.snapshot, &captured.controls)
    else {
        return Err(std::io::Error::other("unreadable request survived parsing"));
    };
    let staged = Staged {
        evaluation: dir.join("evaluation-request.json"),
        snapshot: dir.join("snapshot-request.json"),
        controls: dir.join("controls-request.json"),
        report: dir.join("report.json"),
    };
    fs::write(&staged.evaluation, evaluation)?;
    fs::write(&staged.snapshot, snapshot)?;
    fs::write(&staged.controls, controls)?;
    Ok(staged)
}

fn spawn_evaluator(
    args: &Args,
    private: &Path,
    staged: &Staged,
) -> std::io::Result<std::process::Child> {
    let exe = env::current_exe()?;
    std::process::Command::new(exe)
        .arg("check")
        .arg("--repository")
        .arg(&args.repository)
        .arg("--evaluation-request")
        .arg(&staged.evaluation)
        .arg("--snapshot-request")
        .arg(&staged.snapshot)
        .arg("--controls-request")
        .arg(&staged.controls)
        .arg("--output")
        .arg(&staged.report)
        .env_clear()
        .env(EVALUATOR_SENTINEL, "1")
        .env("TMPDIR", private)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::inherit())
        .spawn()
}

/// Self-restriction for the evaluator process, in safe Rust only: no child
/// processes (the contract's zero repository-process budget), no core dumps
/// (the address space holds repository bytes), and the sandbox descriptor's
/// 1 GiB memory ceiling as an address-space limit. Failures are tolerated,
/// since a plain process is always self-asserted; network denial is
/// structural here (the engine has no network code and no network
/// dependency), and the closed provider-verified mechanisms are the
/// controller's to enforce.
#[cfg(unix)]
fn apply_sandbox() {
    use rustix::process::{Resource, Rlimit, setrlimit};
    let zero = Rlimit {
        current: Some(0),
        maximum: Some(0),
    };
    let _forks = setrlimit(Resource::Nproc, zero);
    let _core = setrlimit(Resource::Core, zero);
    let _memory = setrlimit(
        Resource::As,
        Rlimit {
            current: Some(SANDBOX_MEMORY_BYTES),
            maximum: Some(SANDBOX_MEMORY_BYTES),
        },
    );
}

#[cfg(not(unix))]
fn apply_sandbox() {}

#[expect(clippy::print_stderr, reason = "contract diagnostics channel")]
fn request_failure(
    engine: &EngineProvenance,
    codes: &BTreeSet<AnalysisErrorCode>,
    digests: &RequestDigests,
    output: Option<&Path>,
    reserve: &mut FatalSerializer,
) -> ExitCode {
    let failure = ExitCode::from(ExitClass::Failure.code());
    let Some(envelope) =
        unavailable_evaluation_envelope(engine, codes, digests.evaluation, digests.controls)
    else {
        eprintln!(
            "amiss-wrapper: {}",
            AnalysisErrorCode::ReportConstructionFailed.as_str()
        );
        return failure;
    };
    emit_to(output, &reserve.wire_bytes(&envelope));
    failure
}

/// The repository handle could not be opened: the snapshots are unavailable
/// with the exact Git code against the requested identities.
fn repository_failure(
    engine: &EngineProvenance,
    evaluation: &EvaluationRequest,
    digests: &RequestDigests,
    defect: &amiss_git::Error,
    args: &Args,
    reserve: &mut FatalSerializer,
) -> ExitCode {
    use amiss_scan::report::{CandidateBlock, Setup, SnapshotIdentity, construct_incomplete};
    let code = match defect {
        amiss_git::Error::RepositoryUnavailable => AnalysisErrorCode::GitRepositoryUnavailable,
        amiss_git::Error::ObjectMissing => AnalysisErrorCode::GitObjectMissing,
        amiss_git::Error::ObjectWrongKind => AnalysisErrorCode::GitObjectWrongKind,
        amiss_git::Error::ObjectUnreadable | amiss_git::Error::ResourceLimit { .. } => {
            AnalysisErrorCode::GitObjectUnreadable
        }
        amiss_git::Error::IndexInvalid => AnalysisErrorCode::GitIndexInvalid,
        amiss_git::Error::IndexUnmerged => AnalysisErrorCode::GitIndexUnmerged,
        amiss_git::Error::IntentToAdd => AnalysisErrorCode::GitIntentToAdd,
        amiss_git::Error::SnapshotChanged => AnalysisErrorCode::GitSnapshotChanged,
    };
    let identity = |oid: &amiss_wire::model::Oid| SnapshotIdentity {
        object_format: match evaluation.object_format {
            amiss_wire::model::ObjectFormat::Sha1 => "sha1",
            amiss_wire::model::ObjectFormat::Sha256 => "sha256",
        },
        commit_oid: oid.as_str().to_owned(),
        tree_oid: oid.as_str().to_owned(),
    };
    let candidate = evaluation
        .candidate_commit
        .as_ref()
        .map_or(CandidateBlock::Unavailable(vec!["not-evaluated"]), |oid| {
            CandidateBlock::Commit(identity(oid))
        });
    let setup = Setup {
        engine: engine.clone(),
        enforce: matches!(evaluation.profile, amiss_wire::controls::Profile::Enforce),
        repository: evaluation
            .repository
            .as_ref()
            .map(|repository| (repository.owner.clone(), repository.name.clone())),
        candidate_ref: evaluation
            .ref_name
            .as_ref()
            .map(|reference| reference.as_str().to_owned()),
        default_branch_ref: evaluation
            .default_branch_ref
            .as_ref()
            .map(|reference| reference.as_str().to_owned()),
        base: identity(&evaluation.base_commit),
        candidate,
        policy: amiss_scan::Effects::default(),
        controls_unavailable: None,
        requests: *digests,
    };
    let built = construct_incomplete(
        &setup,
        &[ErrorDetail {
            code,
            path: None,
            resource: None,
        }],
    );
    emit_to(args.output.as_deref(), &reserve.wire_bytes(&built.envelope));
    exit_class(built.exit_code)
}

fn build_github(evaluation: &EvaluationRequest) -> Option<amiss_scan::GithubContext> {
    let repository = evaluation.repository.as_ref()?;
    let candidate_ref = evaluation.ref_name.as_ref()?;
    let default_ref = evaluation.default_branch_ref.as_ref()?;
    Some(amiss_scan::GithubContext {
        owner: repository.owner.clone(),
        repository: repository.name.clone(),
        candidate_ref: candidate_ref.as_str().to_owned(),
        default_ref: default_ref.as_str().to_owned(),
    })
}

#[expect(clippy::print_stderr, reason = "contract diagnostics channel")]
fn emit_to(output: Option<&Path>, wire: &[u8]) {
    let written = match output {
        Some(path) => fs::write(path, wire).is_ok(),
        None => std::io::stdout().write_all(wire).is_ok(),
    };
    if !written {
        eprintln!(
            "amiss-wrapper: {}",
            AnalysisErrorCode::ReportConstructionFailed.as_str()
        );
    }
}

fn exit_class(code: i64) -> ExitCode {
    match code {
        0 => ExitCode::from(ExitClass::Success.code()),
        1 => ExitCode::from(ExitClass::BlockingFindings.code()),
        _ => ExitCode::from(ExitClass::Failure.code()),
    }
}

fn engine_provenance() -> Option<EngineProvenance> {
    let exe = env::current_exe().ok()?;
    let bytes = fs::read(exe).ok()?;
    Some(EngineProvenance {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        digest: hb(amiss_wire::report::ENGINE_DOMAIN, &bytes),
    })
}
