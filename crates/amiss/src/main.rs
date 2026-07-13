use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::io::Write as _;
use std::process::ExitCode;

use amiss::invocation::{self, CandidateSelector, Code, Invocation, Outcome, OutputFormat};
use amiss_wire::ExitClass;
use amiss_wire::digest::hb;
use amiss_wire::report::{self, AnalysisErrorCode, EngineProvenance, ErrorDetail};

#[expect(clippy::print_stderr, reason = "contract diagnostics channel")]
fn main() -> ExitCode {
    let argv: Vec<std::ffi::OsString> = env::args_os().skip(1).collect();
    let failure = ExitCode::from(ExitClass::Failure.code());
    match invocation::parse(&argv) {
        Outcome::MalformedOutputSelection => {
            eprint!("{}", invocation::MALFORMED_OUTPUT_LINE);
            failure
        }
        Outcome::Rejected {
            format: OutputFormat::Json,
            codes,
        } => {
            let Some(engine) = engine_provenance() else {
                eprintln!("amiss: {}", AnalysisErrorCode::InternalError.as_str());
                return failure;
            };
            let codes: BTreeSet<AnalysisErrorCode> =
                codes.iter().map(|code| analysis_code(*code)).collect();
            let Some(wire) = report::invocation_failure_wire(&engine, &codes) else {
                eprintln!(
                    "amiss: {}",
                    AnalysisErrorCode::ReportConstructionFailed.as_str()
                );
                return failure;
            };
            emit(&wire);
            failure
        }
        Outcome::Rejected {
            format: OutputFormat::Human,
            codes,
        } => {
            for code in &codes {
                eprintln!("amiss: {}", code.as_str());
            }
            failure
        }
        Outcome::Accepted(invocation) => run(&invocation),
    }
}

#[cfg(unix)]
#[expect(clippy::print_stderr, reason = "contract diagnostics channel")]
fn run(invocation: &Invocation) -> ExitCode {
    use amiss_scan::pipeline::{SetupShell, commit_pair};
    use amiss_scan::resolve::GithubContext;

    let failure = ExitCode::from(ExitClass::Failure.code());
    let Some(engine) = engine_provenance() else {
        eprintln!("amiss: {}", AnalysisErrorCode::InternalError.as_str());
        return failure;
    };
    let repo = match amiss_git::Repository::open(&invocation.repo, invocation.object_format) {
        Ok(repo) => repo,
        Err(defect) => {
            let code = match defect {
                amiss_git::Error::RepositoryUnavailable => {
                    AnalysisErrorCode::GitRepositoryUnavailable
                }
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
            return fatal(
                invocation,
                &engine,
                &[ErrorDetail {
                    code,
                    path: None,
                    resource: None,
                }],
            );
        }
    };

    let github = invocation.identity.as_ref().map(|identity| GithubContext {
        owner: identity.repository.owner.clone(),
        repository: identity.repository.name.clone(),
        candidate_ref: identity.ref_name.as_str().to_owned(),
        default_ref: identity.default_branch_ref.as_str().to_owned(),
    });
    let shell = SetupShell {
        engine,
        enforce: matches!(invocation.profile, amiss_wire::controls::Profile::Enforce),
        repository: invocation.identity.as_ref().map(|identity| {
            (
                identity.repository.owner.clone(),
                identity.repository.name.clone(),
            )
        }),
        candidate_ref: invocation
            .identity
            .as_ref()
            .map(|identity| identity.ref_name.as_str().to_owned()),
        default_branch_ref: invocation
            .identity
            .as_ref()
            .map(|identity| identity.default_branch_ref.as_str().to_owned()),
    };
    let built = match &invocation.candidate {
        CandidateSelector::Commit(candidate_oid) => commit_pair(
            &repo,
            &shell.engine,
            github.as_ref(),
            &shell,
            &invocation.base,
            candidate_oid,
        ),
        CandidateSelector::Index => amiss_scan::pipeline::staged_index(
            &repo,
            &shell.engine,
            github.as_ref(),
            &shell,
            &invocation.base,
        ),
    };
    match invocation.format {
        OutputFormat::Json => emit(&built.wire),
        OutputFormat::Human => human(&built),
    }
    exit_class(built.exit_code)
}

#[cfg(not(unix))]
#[expect(clippy::print_stderr, reason = "contract diagnostics channel")]
fn run(_invocation: &Invocation) -> ExitCode {
    eprintln!("amiss: {}", AnalysisErrorCode::InternalError.as_str());
    ExitCode::from(ExitClass::Failure.code())
}

#[cfg(unix)]
fn fatal(invocation: &Invocation, engine: &EngineProvenance, details: &[ErrorDetail]) -> ExitCode {
    use amiss_scan::report::{Setup, SnapshotIdentity, construct_incomplete};

    let identity = |oid: &amiss_wire::model::Oid| SnapshotIdentity {
        object_format: match invocation.object_format {
            amiss_wire::model::ObjectFormat::Sha1 => "sha1",
            amiss_wire::model::ObjectFormat::Sha256 => "sha256",
        },
        commit_oid: oid.as_str().to_owned(),
        tree_oid: oid.as_str().to_owned(),
    };
    let candidate = match &invocation.candidate {
        CandidateSelector::Commit(oid) => amiss_scan::report::CandidateBlock::Commit(identity(oid)),
        CandidateSelector::Index => {
            amiss_scan::report::CandidateBlock::Unavailable(vec!["not-evaluated"])
        }
    };
    let setup = Setup {
        engine: engine.clone(),
        enforce: matches!(invocation.profile, amiss_wire::controls::Profile::Enforce),
        repository: None,
        candidate_ref: None,
        default_branch_ref: None,
        base: identity(&invocation.base),
        candidate,
    };
    let built = construct_incomplete(&setup, details);
    match invocation.format {
        OutputFormat::Json => emit(&built.wire),
        OutputFormat::Human => human(&built),
    }
    ExitCode::from(ExitClass::Failure.code())
}

#[expect(clippy::print_stderr, reason = "contract diagnostics channel")]
fn emit(wire: &[u8]) {
    if std::io::stdout().write_all(wire).is_err() {
        eprintln!(
            "amiss: {}",
            AnalysisErrorCode::ReportConstructionFailed.as_str()
        );
    }
}

/// A compact deterministic projection of the same payload: the result line,
/// the counts, and one line per finding in canonical key order. The full
/// human rendering contract is still to land; nothing here is parsed back.
#[cfg(unix)]
#[expect(clippy::print_stdout, reason = "the human output channel")]
fn human(built: &amiss_scan::report::Built) {
    use amiss_wire::json::Value;

    let field = |members: &[(String, Value)], name: &str| -> Option<Value> {
        members
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
    };
    let Value::Object(envelope) = &built.envelope else {
        return;
    };
    let Some(Value::Object(payload)) = field(envelope, "payload") else {
        return;
    };
    let counts = |name: &str| -> i64 {
        field(&payload, "result")
            .and_then(|result| match result {
                Value::Object(members) => field(&members, name),
                Value::Null
                | Value::Bool(_)
                | Value::Integer(_)
                | Value::String(_)
                | Value::Array(_) => None,
            })
            .and_then(|value| match value {
                Value::Integer(number) => Some(number),
                Value::Null
                | Value::Bool(_)
                | Value::String(_)
                | Value::Array(_)
                | Value::Object(_) => None,
            })
            .unwrap_or(0)
    };
    println!(
        "amiss: {} ({} findings, {} errors)",
        built.status,
        counts("finding_count"),
        counts("error_count")
    );
    if let Some(Value::Array(findings)) = field(&payload, "findings") {
        for finding in findings {
            let Value::Object(members) = finding else {
                continue;
            };
            let text = |name: &str| -> String {
                match field(&members, name) {
                    Some(Value::String(value)) => value,
                    Some(
                        Value::Null
                        | Value::Bool(_)
                        | Value::Integer(_)
                        | Value::Array(_)
                        | Value::Object(_),
                    )
                    | None => String::new(),
                }
            };
            let path = match field(&members, "location") {
                Some(Value::Object(location)) => match field(&location, "path") {
                    Some(Value::String(value)) => value,
                    Some(
                        Value::Null
                        | Value::Bool(_)
                        | Value::Integer(_)
                        | Value::Array(_)
                        | Value::Object(_),
                    )
                    | None => String::new(),
                },
                Some(
                    Value::Null
                    | Value::Bool(_)
                    | Value::Integer(_)
                    | Value::String(_)
                    | Value::Array(_),
                )
                | None => String::new(),
            };
            println!(
                "  {} {} {}",
                text("effective_disposition"),
                text("kind"),
                path
            );
        }
    }
    if let Some(Value::Array(errors)) = field(&payload, "errors") {
        for row in errors {
            let Value::Object(members) = row else {
                continue;
            };
            if let Some(Value::String(code)) = field(&members, "code") {
                println!("  error {code}");
            }
        }
    }
}

#[cfg(unix)]
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
        digest: hb(report::ENGINE_DOMAIN, &bytes),
    })
}

fn analysis_code(code: Code) -> AnalysisErrorCode {
    match code {
        Code::InvalidEvent => AnalysisErrorCode::InvalidEvent,
        Code::InvalidInvocation => AnalysisErrorCode::InvalidInvocation,
        Code::InvalidProfile => AnalysisErrorCode::InvalidProfile,
        Code::UnsupportedProviderHost => AnalysisErrorCode::UnsupportedProviderHost,
    }
}
