mod adopt;
mod author;
mod codequality;
mod external;
mod human;
mod input;
mod invocation;
mod junit;
mod output;
mod policy_include;
mod record_set;
mod references;
mod render;
mod repair;
mod sarif;
mod view;

use std::borrow::Cow;
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::io::{BufWriter, Stdout};
use std::process::ExitCode;

use amiss_wire::ExitClass;
use amiss_wire::digest::hb;
use amiss_wire::model::Oid;
use amiss_wire::report::model::{
    ControlsUnavailableReason, ReportEnvelope, ReportPayload, SnapshotUnavailableReason,
};
use amiss_wire::report::{self, AnalysisErrorCode, EngineProvenance, ErrorDetail};
use amiss_wire::requests::{
    CONTROLS_REQUEST_SCHEMA, ControlsRequest, EVALUATION_REQUEST_SCHEMA, EvaluationRequest,
    RequestMode, RequestStreams, SEALED_ENGINE_ARGUMENT, SNAPSHOT_REQUEST_SCHEMA,
    SnapshotMaterialization, SnapshotRequest,
};
use invocation::{CandidateSelector, Code, Invocation, Outcome, OutputFormat, Verb};

/// Self-restriction, in safe Rust only: no child processes (the contract's
/// zero repository-process budget), no core dumps (the address space holds
/// repository bytes), and the sandbox's memory ceiling. Failures are
/// tolerated, since a plain process is always self-asserted; the report says
/// so, and the closed provider-verified mechanisms are the controller's to
/// enforce. Network denial is structural: the engine has no network code and
/// no network dependency.
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
            current: Some(report::EVALUATOR_MANAGED_MEMORY_BYTES),
            maximum: Some(report::EVALUATOR_MANAGED_MEMORY_BYTES),
        },
    );
}

#[cfg(not(unix))]
const fn apply_sandbox() {}

#[expect(
    clippy::print_stderr,
    clippy::print_stdout,
    reason = "contract output channels"
)]
fn main() -> ExitCode {
    apply_sandbox();
    let mut reserve = BufWriter::with_capacity(report::FATAL_SCRATCH_BYTES, std::io::stdout());
    let argv: Vec<std::ffi::OsString> = env::args_os().skip(1).collect();
    if argv.as_slice() == [std::ffi::OsString::from(SEALED_ENGINE_ARGUMENT)] {
        return run_sealed(&mut reserve);
    }
    let failure = ExitCode::from(ExitClass::Failure.code());
    match invocation::parse(&argv) {
        Outcome::Help => {
            println!("{}", invocation::GRAMMAR);
            ExitCode::from(ExitClass::Success.code())
        }
        Outcome::Version => version(),
        Outcome::MalformedOutputSelection => {
            eprint!("{}", invocation::MALFORMED_OUTPUT_LINE);
            failure
        }
        Outcome::Rejected {
            format: format @ (OutputFormat::Json | OutputFormat::Sarif | OutputFormat::CodeQuality),
            codes,
        } => {
            match machine_refusal(&codes) {
                Ok(envelope) => {
                    return projection_exit(
                        project(&envelope, format, false, false, &mut reserve, |path| {
                            path.as_str()
                                .ok_or_else(|| Cow::Owned(hex::encode(path.as_bytes())))
                        }),
                        failure,
                    );
                }
                Err(code) => {
                    if format == OutputFormat::CodeQuality {
                        diagnose_emission(output::write_serialized(
                            &Vec::<codequality::Issue<'_>>::new(),
                        ));
                    }
                    eprintln!("amiss: {}", code.as_ref());
                }
            }
            failure
        }
        Outcome::Rejected {
            format: OutputFormat::Human | OutputFormat::Junit,
            codes,
        } => {
            for code in &codes {
                eprintln!("amiss: {}", code.as_ref());
                eprintln!("  {}", code.meaning());
            }
            eprintln!("{}", invocation::GRAMMAR);
            failure
        }
        Outcome::Accepted(command) => match *command {
            invocation::Command::Scan(invocation) => run(&invocation, &mut reserve),
            invocation::Command::Author(author) => author::run(&author),
            invocation::Command::Plan(plan) => external::run_plan(&plan),
            invocation::Command::Assess(assess) => external::run_assess(&assess),
            invocation::Command::Render(render) => render::run(&render, &mut reserve),
            invocation::Command::Refs(refs) => references::run(&refs),
            invocation::Command::PolicyInclude(include) => policy_include::run(&include),
            invocation::Command::RecordSet(record_set) => record_set::run(&record_set),
        },
    }
}

fn project<P, R, M, E>(
    envelope: &ReportEnvelope<ReportPayload<P, R, M, E>>,
    format: OutputFormat,
    explain_scope: bool,
    full_feedback: bool,
    reserve: &mut BufWriter<Stdout>,
    path: impl Fn(&P) -> Result<&str, Cow<'_, str>> + Copy,
) -> std::io::Result<()>
where
    ReportPayload<P, R, M, E>: serde::Serialize,
{
    let payload = &envelope.payload;
    match format {
        OutputFormat::Json => {
            report::emit_report(envelope, reserve)?;
        }
        OutputFormat::Sarif => {
            output::write_serialized(&sarif::log(payload, |value| path(value).ok()))?;
        }
        OutputFormat::CodeQuality => {
            output::write_serialized(&codequality::issues(payload, |value| {
                path(value).map_or_else(|hex| hex, Cow::Borrowed)
            }))?;
        }
        OutputFormat::Junit => junit::write(payload, reserve, |value| path(value).ok())?,
        OutputFormat::Human => human::report(payload, explain_scope, full_feedback, |value| {
            value.map_or_else(
                || "-".to_owned(),
                |value| match path(value) {
                    Ok(text) => amiss_wire::human::atom(text),
                    Err(hex) => amiss_wire::human::atom_bytes(&amiss_wire::human::decode_hex(&hex)),
                },
            )
        }),
    }
    Ok(())
}

#[expect(clippy::print_stderr, reason = "contract diagnostics channel")]
fn projection_exit(result: std::io::Result<()>, verdict: ExitCode) -> ExitCode {
    match result {
        Ok(()) => verdict,
        Err(defect) if defect.kind() == std::io::ErrorKind::BrokenPipe => verdict,
        Err(_defect) => {
            eprintln!(
                "amiss: {}",
                AnalysisErrorCode::ReportConstructionFailed.as_ref()
            );
            ExitCode::from(ExitClass::Failure.code())
        }
    }
}

/// The machine refusal lanes share one envelope; the error is the code the
/// caller prints on stderr, and the artifact lane still answers its empty
/// array, the one machine answer that needs no envelope.
fn machine_refusal(
    codes: &BTreeSet<Code>,
) -> Result<ReportEnvelope<ReportPayload<amiss_wire::model::RepoPath>>, AnalysisErrorCode> {
    let Some(engine) = engine_provenance() else {
        return Err(AnalysisErrorCode::InternalError);
    };
    report::invocation_failure_envelope(&engine, codes)
        .map_err(|_defect| AnalysisErrorCode::ReportConstructionFailed)?
        .ok_or(AnalysisErrorCode::ReportConstructionFailed)
}

#[expect(clippy::print_stderr, reason = "contract diagnostics channel")]
fn run_sealed(reserve: &mut BufWriter<Stdout>) -> ExitCode {
    use amiss_scan::pipeline::SetupShell;

    let failure = ExitCode::from(ExitClass::Failure.code());
    let Some(engine) = engine_provenance() else {
        eprintln!("amiss: {}", AnalysisErrorCode::InternalError.as_ref());
        return failure;
    };
    let Ok(streams) = RequestStreams::read_from(&mut std::io::stdin().lock()) else {
        eprintln!("amiss: {}", AnalysisErrorCode::RequestUnreadable.as_ref());
        return failure;
    };
    let parsed = (
        EvaluationRequest::parse(&streams.evaluation),
        SnapshotRequest::parse(&streams.snapshot),
        ControlsRequest::parse(&streams.controls),
    );
    let (Ok(evaluation), Ok(snapshot), Ok(controls)) = parsed else {
        eprintln!("amiss: {}", AnalysisErrorCode::InvalidInvocation.as_ref());
        return failure;
    };
    let canonical = evaluation.canonical_bytes().ok().as_deref() == Some(&streams.evaluation)
        && snapshot.canonical_bytes().ok().as_deref() == Some(&streams.snapshot)
        && controls.canonical_bytes().ok().as_deref() == Some(&streams.controls);
    let modes_match = matches!(
        (evaluation.mode, snapshot.materialization),
        (RequestMode::CommitPair, SnapshotMaterialization::GitObjects)
            | (RequestMode::Index, SnapshotMaterialization::Index)
    );
    if !canonical || !modes_match {
        eprintln!("amiss: {}", AnalysisErrorCode::InvalidInvocation.as_ref());
        return failure;
    }
    let control_result = amiss_scan::request::controls(&controls);
    let (inputs, external_defect) = match control_result {
        Ok(inputs) => (inputs, None),
        Err(detail) => (amiss_scan::request::ControlInputs::default(), Some(detail)),
    };
    let repo =
        match amiss_git::Repository::open(std::path::Path::new("."), evaluation.object_format) {
            Ok(repository) => repository,
            Err(_defect) => {
                eprintln!(
                    "amiss: {}",
                    AnalysisErrorCode::GitRepositoryUnavailable.as_ref()
                );
                return failure;
            }
        };
    let forge = forge_context(
        evaluation.repository.as_ref(),
        evaluation.forge,
        evaluation.object_format,
        evaluation.candidate_ref.as_ref(),
        evaluation.default_branch_ref.as_ref(),
    );
    let requests = amiss_scan::report::RequestDigests {
        evaluation: Some(hb(EVALUATION_REQUEST_SCHEMA, &streams.evaluation)),
        snapshot: Some(hb(SNAPSHOT_REQUEST_SCHEMA, &streams.snapshot)),
        controls: Some(hb(CONTROLS_REQUEST_SCHEMA, &streams.controls)),
    };
    let shell = SetupShell {
        engine,
        profile: evaluation.profile,
        repository: evaluation.repository.clone(),
        forge: evaluation.forge,
        candidate_ref: evaluation.candidate_ref.clone(),
        target_ref: evaluation.target_ref.clone(),
        default_branch_ref: evaluation.default_branch_ref.clone(),
        floor: inputs.floor,
        debt: inputs.debt,
        waiver: inputs.waiver,
        time: inputs.time,
        constraint: inputs.constraint,
        semantic: amiss_scan::semantic::Input::Bound(inputs.semantic),
        requests,
        external_defect: external_defect
            .map(|detail| (ControlsUnavailableReason::InvalidExternalControl, detail)),
        errors_retained: 64,
    };
    let Ok(built) = evaluate_snapshots(
        &repo,
        forge.as_ref(),
        &shell,
        &evaluation.base_commit,
        evaluation.candidate_commit.as_ref(),
    ) else {
        eprintln!(
            "amiss: {}",
            AnalysisErrorCode::ReportConstructionFailed.as_ref()
        );
        return failure;
    };
    projection_exit(
        report::emit_report(&built.envelope, reserve).map(|_written| ()),
        ExitCode::from(built.exit_code),
    )
}

fn forge_context(
    repository: Option<&amiss_wire::model::RepositoryIdentity>,
    dialect: Option<amiss_wire::model::ForgeDialect>,
    object_format: amiss_wire::model::ObjectFormat,
    candidate_ref: Option<&amiss_wire::model::BranchRef>,
    default_branch_ref: Option<&amiss_wire::model::BranchRef>,
) -> Option<amiss_scan::resolve::ForgeContext> {
    let (Some(repository), Some(dialect)) = (repository, dialect) else {
        return None;
    };
    Some(amiss_scan::resolve::ForgeContext {
        host: repository.host().to_owned(),
        dialect,
        object_format,
        owner: repository.owner().to_owned(),
        repository: repository.name().to_owned(),
        candidate_ref: candidate_ref
            .map_or_else(String::new, |reference| reference.as_str().to_owned()),
        default_ref: default_branch_ref
            .map_or_else(String::new, |reference| reference.as_str().to_owned()),
    })
}

fn evaluate_snapshots(
    repo: &amiss_git::Repository,
    forge: Option<&amiss_scan::resolve::ForgeContext>,
    shell: &amiss_scan::pipeline::SetupShell,
    base: &Oid,
    candidate: Option<&Oid>,
) -> Result<amiss_scan::report::Built, amiss_scan::Error> {
    match candidate {
        Some(candidate) => {
            amiss_scan::pipeline::commit_pair(repo, &shell.engine, forge, shell, base, candidate)
        }
        None => amiss_scan::pipeline::staged_index(repo, &shell.engine, forge, shell, base),
    }
}

#[expect(clippy::print_stderr, reason = "contract diagnostics channel")]
fn run(invocation: &Invocation, reserve: &mut BufWriter<Stdout>) -> ExitCode {
    use amiss_scan::pipeline::SetupShell;

    let failure = ExitCode::from(ExitClass::Failure.code());
    let Some(engine) = engine_provenance() else {
        eprintln!("amiss: {}", AnalysisErrorCode::InternalError.as_ref());
        return failure;
    };
    let semantic = match semantic_input(invocation.semantic_template.as_deref()) {
        Ok(input) => input,
        Err(detail) => return fatal(invocation, &engine, &[detail], reserve),
    };
    let repo = match amiss_git::Repository::open(&invocation.repo, invocation.object_format) {
        Ok(repo) => repo,
        Err(_defect) => {
            return fatal(
                invocation,
                &engine,
                &[ErrorDetail {
                    code: AnalysisErrorCode::GitRepositoryUnavailable,
                    path: None,
                    path_bytes: None,
                    resource: None,
                }],
                reserve,
            );
        }
    };

    let identity = invocation.identity.as_ref();
    let forge = forge_context(
        identity.map(|identity| &identity.repository),
        invocation.forge,
        invocation.object_format,
        identity.map(|identity| &identity.ref_name),
        identity.map(|identity| &identity.default_branch_ref),
    );
    let staged_snapshot = pinned_index(invocation, &repo);
    let shell = SetupShell {
        engine,
        profile: invocation.profile,
        repository: identity.map(|identity| identity.repository.clone()),
        forge: invocation.forge,
        candidate_ref: identity.map(|identity| identity.ref_name.clone()),
        target_ref: None,
        default_branch_ref: identity.map(|identity| identity.default_branch_ref.clone()),
        // Public semantic templates remain self-asserted inputs.
        floor: None,
        debt: None,
        waiver: None,
        time: None,
        constraint: None,
        semantic,
        requests: amiss_scan::report::RequestDigests::default(),
        external_defect: None,
        errors_retained: 64,
    };
    let candidate = match &invocation.candidate {
        CandidateSelector::Commit(candidate) => Some(candidate),
        CandidateSelector::Index => None,
    };
    let Ok(built) = evaluate_snapshots(&repo, forge.as_ref(), &shell, &invocation.base, candidate)
    else {
        eprintln!(
            "amiss: {}",
            AnalysisErrorCode::ReportConstructionFailed.as_ref()
        );
        return failure;
    };
    if invocation.verb == Verb::Fix {
        return repair::run(
            &invocation.repo,
            &repo,
            invocation.object_format,
            &built,
            staged_snapshot.as_deref(),
        );
    }
    if let (Verb::Adopt, Some(adoption)) = (invocation.verb, &invocation.adoption) {
        return adopt::run(invocation, adoption, &built);
    }
    projection_exit(
        project(
            &built.envelope,
            invocation.format,
            invocation.explain_scope,
            false,
            reserve,
            |path| {
                path.as_str()
                    .ok_or_else(|| Cow::Owned(hex::encode(path.as_bytes())))
            },
        ),
        ExitCode::from(built.exit_code),
    )
}

fn semantic_input(
    path: Option<&std::path::Path>,
) -> Result<amiss_scan::semantic::Input, ErrorDetail> {
    let Some(path) = path else {
        return Ok(amiss_scan::semantic::Input::None);
    };
    let bytes = input::bounded_bytes(path, amiss_wire::semantic::SEMANTIC_EVIDENCE_BYTES).map_err(
        |_error| ErrorDetail {
            code: AnalysisErrorCode::ConfigurationInvalid,
            path: None,
            path_bytes: None,
            resource: None,
        },
    )?;
    amiss_wire::semantic::parse_template(&bytes)
        .map(amiss_scan::semantic::Input::Template)
        .map_err(|error| amiss_scan::request::configuration_detail(&error))
}

/// The repair verb pins the index before the evaluation reads it, so the
/// spans it later applies were proven against exactly these bytes.
fn pinned_index(invocation: &Invocation, repo: &amiss_git::Repository) -> Option<Vec<u8>> {
    (invocation.verb == Verb::Fix)
        .then(|| {
            let mut resources = amiss_git::GitResources::new(amiss_git::GitLimits::default());
            repo.read_index_bytes(&mut resources).ok()
        })
        .flatten()
}

fn fatal(
    invocation: &Invocation,
    engine: &EngineProvenance,
    details: &[ErrorDetail],
    reserve: &mut BufWriter<Stdout>,
) -> ExitCode {
    use amiss_scan::report::{Setup, SnapshotIdentity, construct_incomplete};

    let identity = |oid: &Oid| SnapshotIdentity {
        commit_oid: oid.clone(),
        kind: amiss_wire::requests::GitSnapshotKind::GitCommit,
        object_format: invocation.object_format,
        tree_oid: oid.clone(),
    };
    let candidate = match &invocation.candidate {
        CandidateSelector::Commit(oid) => amiss_scan::report::CandidateBlock::Commit(identity(oid)),
        CandidateSelector::Index => amiss_scan::report::CandidateBlock::Unavailable(vec![
            SnapshotUnavailableReason::NotEvaluated,
        ]),
    };
    let setup = Setup {
        engine: engine.clone(),
        profile: invocation.profile,
        repository: None,
        forge: None,
        candidate_ref: None,
        target_ref: None,
        default_branch_ref: None,
        base: identity(&invocation.base),
        candidate,
        policy: amiss_scan::policy::Effects::default(),
        controls_unavailable: None,
        requests: amiss_scan::report::RequestDigests::default(),
    };
    projection_exit(
        construct_incomplete(&setup, details)
            .map_err(|defect| std::io::Error::other(defect.code().meaning()))
            .and_then(|built| {
                project(
                    &built.envelope,
                    invocation.format,
                    invocation.explain_scope,
                    false,
                    reserve,
                    |path| {
                        path.as_str()
                            .ok_or_else(|| Cow::Owned(hex::encode(path.as_bytes())))
                    },
                )
            }),
        ExitCode::from(ExitClass::Failure.code()),
    )
}

#[expect(clippy::print_stderr, reason = "contract diagnostics channel")]
fn diagnose_emission(result: std::io::Result<()>) {
    if let Err(defect) = result
        && defect.kind() != std::io::ErrorKind::BrokenPipe
    {
        eprintln!(
            "amiss: {}",
            AnalysisErrorCode::ReportConstructionFailed.as_ref()
        );
    }
}

/// The second line is the `engine_digest` the release manifest pins and every report carries.
#[expect(clippy::print_stdout, reason = "the identity query's output channel")]
fn version() -> ExitCode {
    println!("amiss {}", env!("CARGO_PKG_VERSION"));
    match engine_provenance() {
        Some(engine) => println!("engine {}", engine.digest),
        None => println!("engine unavailable"),
    }
    ExitCode::from(ExitClass::Success.code())
}

fn engine_provenance() -> Option<EngineProvenance> {
    let exe = env::current_exe().ok()?;
    let bytes = fs::read(exe).ok()?;
    Some(EngineProvenance {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        digest: hb(report::ENGINE_DOMAIN, &bytes),
    })
}
