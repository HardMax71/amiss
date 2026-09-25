use amiss_wire::de::Document as _;
use sha2::Digest as _;
mod adopt;
mod author;
mod codequality;
mod external;
mod human;
mod input;
mod invocation;
mod junit;
mod locale;
mod output;
mod policy_include;
mod pure;
mod record_set;
mod references;
mod render;
mod repair;
mod sarif;

use std::borrow::Cow;
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::io::{BufWriter, Stdout};
use std::process::ExitCode;

use amiss_wire::ExitClass;
use amiss_wire::model::Oid;
use amiss_wire::report::model::{
    ControlsUnavailableReason, FindingFactEvidence, ReportEnvelope, ReportPayload,
    SnapshotUnavailableReason,
};
use amiss_wire::report::{self, EngineProvenance, ErrorDetail, model::AnalysisErrorCode};
use amiss_wire::requests::{
    CONTROLS_REQUEST_SCHEMA, ControlsRequest, EVALUATION_REQUEST_SCHEMA, EvaluationRequest,
    RequestMode, RequestStreams, SEALED_ENGINE_ARGUMENT, SNAPSHOT_REQUEST_SCHEMA,
    SnapshotMaterialization, SnapshotRequest,
};
use amiss_wire::resolution::ResolutionTag;
use invocation::{CandidateSelector, Invocation, Outcome, OutputFormat, Verb};

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

#[expect(clippy::print_stderr, reason = "the contract diagnostics channel")]
fn main() -> ExitCode {
    apply_sandbox();
    let mut reserve = BufWriter::with_capacity(report::FATAL_SCRATCH_BYTES, std::io::stdout());
    let argv: Vec<std::ffi::OsString> = env::args_os().skip(1).collect();
    if argv.as_slice() == [std::ffi::OsString::from(SEALED_ENGINE_ARGUMENT)] {
        return run_sealed(&mut reserve);
    }
    let failure = ExitCode::from(ExitClass::Failure.code());
    match invocation::parse(&argv) {
        Outcome::Help { verb } => {
            answer(&verb.map_or_else(|| invocation::GRAMMAR.to_owned(), invocation::verb_grammar))
        }
        Outcome::Version => version(),
        Outcome::MalformedOutputSelection { reason } => {
            eprintln!("amiss: invalid invocation: {reason}");
            failure
        }
        Outcome::Rejected {
            format: format @ (OutputFormat::Json | OutputFormat::Sarif | OutputFormat::CodeQuality),
            refusals,
        } => {
            let codes = refusals.iter().map(|(code, _reason)| *code).collect();
            match machine_refusal(&codes) {
                Ok(envelope) => {
                    return projection_exit(
                        project(
                            &envelope.payload,
                            format,
                            human::Options {
                                explain_scope: false,
                                full: false,
                            },
                            &mut reserve,
                            |out| report::emit_report(&envelope, out),
                            |path| {
                                path.as_str()
                                    .ok_or_else(|| Cow::Owned(hex::encode(path.as_bytes())))
                            },
                            |resolution| render::wire_resolution(resolution, human::engine_path),
                        ),
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
            refusals,
        } => {
            let mut named = None;
            for (code, reason) in &refusals {
                if named != Some(code) {
                    eprintln!("amiss: {}", code.as_ref());
                    named = Some(code);
                }
                eprintln!("  {reason}");
            }
            eprintln!("{}", invocation::GRAMMAR);
            failure
        }
        Outcome::Accepted(command) => match *command {
            invocation::Command::Scan(invocation) => run(&invocation, &mut reserve),
            invocation::Command::Author(author) => author::run(&author),
            invocation::Command::Plan(plan) => external::run_plan(&plan),
            invocation::Command::Assess(assess) => external::run_assess(&assess),
            invocation::Command::LocaleAssess(assess) => {
                locale::run(&locale::Form::Assess(&assess))
            }
            invocation::Command::LocaleInventory(inventory) => {
                locale::run(&locale::Form::Inventory(&inventory))
            }
            invocation::Command::Render(render) => render::run(&render, &mut reserve),
            invocation::Command::Refs(refs) => references::run(&refs),
            invocation::Command::PolicyInclude(include) => policy_include::run(&include),
            invocation::Command::RecordSet(record_set) => record_set::run(&record_set),
        },
    }
}

fn project<P, R, M, S, D, F>(
    payload: &ReportPayload<P, R, M, FindingFactEvidence<P, R, S, D, M>>,
    format: OutputFormat,
    options: human::Options,
    reserve: &mut BufWriter<Stdout>,
    json: impl FnOnce(&mut BufWriter<Stdout>) -> std::io::Result<u64>,
    path: impl Fn(&P) -> Result<&str, Cow<'_, str>> + Copy,
    resolution: F,
) -> std::io::Result<()>
where
    P: PartialEq,
    F: Fn(&R) -> (ResolutionTag, Option<String>),
{
    match format {
        OutputFormat::Json => {
            json(reserve)?;
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
        OutputFormat::Human => human::report(
            payload,
            options,
            |value| {
                value.map_or_else(
                    || "-".to_owned(),
                    |value| match path(value) {
                        Ok(text) => amiss_wire::human::atom(text),
                        Err(hex) => hex::decode(hex.as_bytes()).map_or_else(
                            |_defect| amiss_wire::human::atom(&hex),
                            |bytes| amiss_wire::human::atom_bytes(&bytes),
                        ),
                    },
                )
            },
            resolution,
        ),
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
    codes: &BTreeSet<AnalysisErrorCode>,
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
    let canonical = serde_json_canonicalizer::to_vec(&evaluation)
        .ok()
        .as_deref()
        == Some(&streams.evaluation)
        && serde_json_canonicalizer::to_vec(&snapshot).ok().as_deref() == Some(&streams.snapshot)
        && serde_json_canonicalizer::to_vec(&controls).ok().as_deref() == Some(&streams.controls);
    let modes_match = matches!(
        (evaluation.mode, snapshot.materialization),
        (RequestMode::CommitPair, SnapshotMaterialization::GitObjects)
            | (RequestMode::Index, SnapshotMaterialization::Index)
    );
    if !canonical || !modes_match {
        eprintln!("amiss: {}", AnalysisErrorCode::InvalidInvocation.as_ref());
        return failure;
    }
    scan_sealed(engine, &streams, &evaluation, controls, reserve)
}

#[expect(clippy::print_stderr, reason = "contract diagnostics channel")]
fn scan_sealed(
    engine: EngineProvenance,
    streams: &RequestStreams,
    evaluation: &EvaluationRequest,
    controls: ControlsRequest,
    reserve: &mut BufWriter<Stdout>,
) -> ExitCode {
    use amiss_scan::pipeline::SetupShell;

    let failure = ExitCode::from(ExitClass::Failure.code());
    let control_result = amiss_scan::request::controls(controls);
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
    let [evaluation_digest, snapshot_digest, controls_digest] = [
        (EVALUATION_REQUEST_SCHEMA, &streams.evaluation),
        (SNAPSHOT_REQUEST_SCHEMA, &streams.snapshot),
        (CONTROLS_REQUEST_SCHEMA, &streams.controls),
    ]
    .map(|(domain, bytes)| {
        amiss_wire::model::Digest::from(
            sha2::Sha256::new_with_prefix(domain)
                .chain_update([0_u8])
                .chain_update(bytes)
                .finalize()
                .0,
        )
    });
    let requests = amiss_scan::report::RequestDigests {
        evaluation: Some(evaluation_digest),
        snapshot: Some(snapshot_digest),
        controls: Some(controls_digest),
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
        report::emit_sealed(
            &built.envelope.schema,
            &built.canonical_payload,
            built.payload_digest,
            reserve,
        )
        .map(|_written| ()),
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
        repository: repository.clone(),
        dialect,
        object_format,
        candidate_ref: candidate_ref.cloned(),
        default_ref: default_branch_ref.cloned(),
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
            &built.envelope.payload,
            invocation.format,
            human_options(invocation),
            reserve,
            |out| {
                report::emit_sealed(
                    &built.envelope.schema,
                    &built.canonical_payload,
                    built.payload_digest,
                    out,
                )
            },
            |path| {
                path.as_str()
                    .ok_or_else(|| Cow::Owned(hex::encode(path.as_bytes())))
            },
            human::engine_resolution,
        ),
        ExitCode::from(built.exit_code),
    )
}

/// A scan's human output is bounded; only a replay may ask for `--full`.
fn human_options(invocation: &Invocation) -> human::Options {
    human::Options {
        explain_scope: invocation.explain_scope,
        full: false,
    }
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
    let template = amiss_wire::semantic::SemanticEvidenceTemplate::parse(&bytes)
        .map_err(|error| amiss_scan::semantic::configuration_detail(&error))?;
    Ok(amiss_scan::semantic::Input::Template(template))
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
    use amiss_scan::report::{BaseBlock, CandidateBlock, Setup, construct_incomplete};

    let unevaluated = || vec![SnapshotUnavailableReason::NotEvaluated];
    let candidate = match &invocation.candidate {
        CandidateSelector::Commit(_) => CandidateBlock::CommitUnavailable(unevaluated()),
        CandidateSelector::Index => CandidateBlock::Unavailable(unevaluated()),
    };
    let setup = Setup {
        engine: engine.clone(),
        profile: invocation.profile,
        repository: None,
        forge: None,
        candidate_ref: None,
        target_ref: None,
        default_branch_ref: None,
        base: BaseBlock::Unavailable(unevaluated()),
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
                    &built.envelope.payload,
                    invocation.format,
                    human_options(invocation),
                    reserve,
                    |out| {
                        report::emit_sealed(
                            &built.envelope.schema,
                            &built.canonical_payload,
                            built.payload_digest,
                            out,
                        )
                    },
                    |path| {
                        path.as_str()
                            .ok_or_else(|| Cow::Owned(hex::encode(path.as_bytes())))
                    },
                    human::engine_resolution,
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
fn version() -> ExitCode {
    let engine = engine_provenance().map_or_else(
        || "engine unavailable".to_owned(),
        |engine| format!("engine {}", engine.digest),
    );
    answer(&format!("amiss {}\n{engine}", env!("CARGO_PKG_VERSION")))
}

/// A query answers on stdout and succeeds. A consumer closing the pipe, `head`
/// among them, ends the printing and not the answer.
fn answer(text: &str) -> ExitCode {
    use std::io::Write as _;

    let mut out = std::io::stdout();
    let _closed = writeln!(out, "{text}");
    let _flushed = out.flush();
    ExitCode::from(ExitClass::Success.code())
}

fn engine_provenance() -> Option<EngineProvenance> {
    let exe = env::current_exe().ok()?;
    let bytes = fs::read(exe).ok()?;
    Some(EngineProvenance {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        digest: amiss_wire::model::Digest::from(
            sha2::Sha256::new_with_prefix(report::ENGINE_DOMAIN)
                .chain_update([0_u8])
                .chain_update(&bytes)
                .finalize()
                .0,
        ),
    })
}
