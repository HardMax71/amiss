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
use amiss_wire::digest::{Digest, hb, hj};
use amiss_wire::json::{Value, canonical};
use amiss_wire::report::{
    AnalysisErrorCode, EngineProvenance, ErrorDetail, PAYLOAD_SCHEMA, unavailable_evaluation_wire,
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
        let Some(wire) = unavailable_evaluation_wire(&engine, &codes, None, None) else {
            eprintln!(
                "amiss-wrapper: {}",
                AnalysisErrorCode::ReportConstructionFailed.as_str()
            );
            return failure;
        };
        emit_to(None, &wire);
        return failure;
    };
    run(&engine, &parsed)
}

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

#[cfg(unix)]
#[expect(clippy::print_stderr, reason = "contract diagnostics channel")]
fn run(engine: &EngineProvenance, args: &Args) -> ExitCode {
    let failure = ExitCode::from(ExitClass::Failure.code());
    let captured = capture_all(args);
    let requests = match parse_requests(&captured) {
        Ok(requests) => requests,
        Err(codes) => {
            let codes = if codes.is_empty() {
                [AnalysisErrorCode::RequestUnreadable].into_iter().collect()
            } else {
                codes
            };
            return request_failure(engine, &codes, &captured.digests, args.output.as_deref());
        }
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
            return repository_failure(engine, evaluation, &captured.digests, &defect, args);
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
    if accept(engine, &requests, &built.wire).is_err() {
        eprintln!(
            "amiss-wrapper: {}",
            AnalysisErrorCode::ReportConstructionFailed.as_str()
        );
        return failure;
    }
    emit_to(args.output.as_deref(), &built.wire);
    exit_class(built.exit_code)
}

#[cfg(not(unix))]
#[expect(clippy::print_stderr, reason = "contract diagnostics channel")]
fn run(_engine: &EngineProvenance, _args: &Args) -> ExitCode {
    eprintln!(
        "amiss-wrapper: {}",
        AnalysisErrorCode::InternalError.as_str()
    );
    ExitCode::from(ExitClass::Failure.code())
}

#[expect(clippy::print_stderr, reason = "contract diagnostics channel")]
fn request_failure(
    engine: &EngineProvenance,
    codes: &BTreeSet<AnalysisErrorCode>,
    digests: &RequestDigests,
    output: Option<&Path>,
) -> ExitCode {
    let failure = ExitCode::from(ExitClass::Failure.code());
    let Some(wire) =
        unavailable_evaluation_wire(engine, codes, digests.evaluation, digests.controls)
    else {
        eprintln!(
            "amiss-wrapper: {}",
            AnalysisErrorCode::ReportConstructionFailed.as_str()
        );
        return failure;
    };
    emit_to(output, &wire);
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
    emit_to(args.output.as_deref(), &built.wire);
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

fn member<'value>(value: &'value Value, key: &str) -> Option<&'value Value> {
    match value {
        Value::Object(members) => members
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, member)| member),
        Value::Null | Value::Bool(_) | Value::Integer(_) | Value::String(_) | Value::Array(_) => {
            None
        }
    }
}

fn text<'value>(value: &'value Value, key: &str) -> Option<&'value str> {
    match member(value, key) {
        Some(Value::String(text)) => Some(text),
        _ => None,
    }
}

/// The acceptance law: parse one complete schema version and verify the
/// payload-only digest, the evaluated identities against the request, the
/// engine digest, the floor digest when supplied and resolved, the
/// completeness flag against the exit class, and the finding count.
fn accept(engine: &EngineProvenance, requests: &Requests, wire: &[u8]) -> Result<(), ()> {
    let trimmed = wire.strip_suffix(b"\n").ok_or(())?;
    let envelope = amiss_wire::json::parse(trimmed).map_err(|_defect| ())?;
    let payload = member(&envelope, "payload").ok_or(())?;
    let recorded = text(&envelope, "payload_digest").ok_or(())?;
    if hj(PAYLOAD_SCHEMA, payload).to_string() != recorded {
        return Err(());
    }
    let engine_row = member(payload, "engine").ok_or(())?;
    if text(engine_row, "engine_digest") != Some(engine.digest.to_string().as_str()) {
        return Err(());
    }
    let evaluation = member(payload, "evaluation").ok_or(())?;
    let resolved = text(evaluation, "status") != Some("unavailable");
    if resolved {
        let base = member(evaluation, "base").ok_or(())?;
        if text(base, "commit_oid") != Some(requests.evaluation.base_commit.as_str()) {
            return Err(());
        }
        let candidate = member(evaluation, "candidate").ok_or(())?;
        if let (Some(expected), Some("git-commit")) = (
            requests.evaluation.candidate_commit.as_ref(),
            text(candidate, "kind"),
        ) && text(candidate, "commit_oid") != Some(expected.as_str())
        {
            return Err(());
        }
        let controls_row = member(payload, "controls").ok_or(())?;
        let controls_resolved = text(controls_row, "status") != Some("unavailable");
        if controls_resolved && let Some(expected) = &requests.controls.organization_floor {
            let floor_row = member(controls_row, "organization_floor").ok_or(())?;
            if text(floor_row, "digest") != Some(expected.expected_digest.to_string().as_str()) {
                return Err(());
            }
        }
    }
    let result = member(payload, "result").ok_or(())?;
    let exit_code = match member(result, "exit_code") {
        Some(Value::Integer(code)) => *code,
        _ => return Err(()),
    };
    let complete = member(result, "complete") == Some(&Value::Bool(true));
    if complete != (exit_code == 0 || exit_code == 1) {
        return Err(());
    }
    let count = match member(result, "finding_count") {
        Some(Value::Integer(count)) => *count,
        _ => return Err(()),
    };
    let findings = match member(payload, "findings") {
        Some(Value::Array(rows)) => rows.len(),
        _ => return Err(()),
    };
    if i64::try_from(findings).map_err(|_defect| ())? != count {
        return Err(());
    }
    Ok(())
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
