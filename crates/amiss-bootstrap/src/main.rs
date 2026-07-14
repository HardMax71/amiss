use std::env;
use std::ffi::OsString;
use std::io::{Read as _, Write as _};
use std::path::PathBuf;
use std::process::{ExitCode, Stdio};
use std::time::Duration;

use amiss_bootstrap::supervise::{
    AcceptanceDefect, Defect, Expectations, Supervised, settle, supervise,
};
use amiss_bootstrap::{Refusal, validate};
use amiss_git::{GitLimits, GitResources, Repository};
use amiss_wire::controls::ExecutionConstraintDescriptor;
use amiss_wire::report::MACHINE_JSON_BYTES;

/// The operational wall ceiling from the security contract: the trusted
/// wrapper kills the whole evaluator after 120 seconds, and a killed evaluator
/// yields no accepted result.
const WATCHDOG_CEILING: Duration = Duration::from_mins(2);

/// The trusted bootstrap, which is also the trusted wrapper the security
/// contract names. It validates the pinned action tree as data, launches the
/// verified engine with a cleared environment and fixed arguments, holds it to
/// the wall ceiling, and publishes only an envelope it can accept. It never
/// runs the action's declared Node launcher, never resolves a binary through
/// `PATH`, and never downloads, installs, or discovers anything.
///
/// `amiss-bootstrap exec --action-repository P --constraint F -- <engine args>`
#[expect(clippy::print_stderr, reason = "the bootstrap's diagnostic channel")]
fn main() -> ExitCode {
    let argv: Vec<OsString> = env::args_os().skip(1).collect();
    let failure = ExitCode::from(2);
    let Some(parsed) = parse_args(&argv) else {
        eprintln!("amiss-bootstrap: invalid-invocation");
        return failure;
    };
    let Ok(constraint_bytes) = std::fs::read(&parsed.constraint) else {
        eprintln!("amiss-bootstrap: constraint-unreadable");
        return failure;
    };
    let Ok(constraint) = ExecutionConstraintDescriptor::parse(&constraint_bytes) else {
        eprintln!("amiss-bootstrap: constraint-invalid");
        return failure;
    };
    let Ok(own_path) = env::current_exe() else {
        eprintln!("amiss-bootstrap: self-unreadable");
        return failure;
    };
    let Ok(own_bytes) = std::fs::read(&own_path) else {
        eprintln!("amiss-bootstrap: self-unreadable");
        return failure;
    };
    let Ok(action) = Repository::open(&parsed.action_repository, constraint.action_object_format)
    else {
        eprintln!("amiss-bootstrap: action-tree-unavailable");
        return failure;
    };

    let mut resources = GitResources::new(GitLimits::CONTRACT);
    let validated = match validate(&action, &mut resources, &constraint, &own_bytes) {
        Ok(validated) => validated,
        Err(refusal) => {
            eprintln!("amiss-bootstrap: {}", reason(refusal));
            return failure;
        }
    };

    run_engine(&parsed, &validated)
}

const fn reason(refusal: Refusal) -> &'static str {
    match refusal {
        Refusal::UnsupportedPlatform => "unsupported-platform",
        Refusal::ActionTree => "action-tree-mismatch",
        Refusal::ActionMetadata => "action-metadata-invalid",
        Refusal::PathNotRegularBlob => "path-not-regular-blob",
        Refusal::ManifestUnreadable => "manifest-unreadable",
        Refusal::ManifestDigest => "manifest-digest-mismatch",
        Refusal::DependencyLock => "dependency-lock-mismatch",
        Refusal::ArtifactSelection => "artifact-selection-failed",
        Refusal::RuntimeClosure => "runtime-closure-mismatch",
        Refusal::EngineDigest => "engine-digest-mismatch",
        Refusal::PlatformBinding => "platform-binding-mismatch",
        Refusal::BootstrapDigest => "bootstrap-digest-mismatch",
    }
}

struct Args {
    action_repository: PathBuf,
    constraint: PathBuf,
    engine: Vec<OsString>,
}

fn parse_args(argv: &[OsString]) -> Option<Args> {
    let mut action_repository: Option<PathBuf> = None;
    let mut constraint: Option<PathBuf> = None;
    let mut engine: Vec<OsString> = Vec::new();
    let mut items = argv.iter();
    if items.next()? != "exec" {
        return None;
    }
    while let Some(flag) = items.next() {
        if flag == "--" {
            engine.extend(items.cloned());
            break;
        }
        let value = items.next()?;
        let slot = match flag.to_str()? {
            "--action-repository" => &mut action_repository,
            "--constraint" => &mut constraint,
            _ => return None,
        };
        if slot.is_some() {
            return None;
        }
        *slot = Some(PathBuf::from(value));
    }
    Some(Args {
        action_repository: action_repository?,
        constraint: constraint?,
        engine,
    })
}

/// What the wrapper asked the engine for, read back out of the fixed arguments
/// it is about to pass. A wrapper can only hold an engine to an identity it
/// knows it requested, and it can only accept a machine envelope it asked for.
///
/// The engine's own parser is the authoritative one. This reads the same
/// grammar only far enough to state an expectation, and an invocation the
/// engine will reject as invalid yields an unavailable evaluation, whose
/// identities acceptance does not check.
fn asked(engine: &[OsString]) -> Option<Expectations> {
    let mut base: Option<String> = None;
    let mut candidate: Option<String> = None;
    let mut index = false;
    let mut format: Option<String> = None;
    let mut items = engine.iter();
    while let Some(flag) = items.next() {
        let mut value = || {
            items
                .next()
                .and_then(|next| next.to_str())
                .filter(|next| !next.starts_with("--"))
                .map(str::to_owned)
        };
        match flag.to_str() {
            Some("--index") => index = true,
            Some("--base") => base = value(),
            Some("--candidate") => candidate = value(),
            Some("--format") => format = value(),
            _ => {}
        }
    }
    if format.as_deref() != Some("json") || candidate.is_some() == index {
        return None;
    }
    Some(Expectations {
        engine_digest: String::new(),
        base_commit: base?,
        candidate_commit: candidate,
    })
}

/// Writes the verified engine bytes into a private directory, launches them
/// with an empty environment, holds them to the wall ceiling, and republishes
/// only an accepted envelope. The bytes come from the validated tree, never
/// from a worktree file, a `PATH` lookup, or the action's launcher.
#[expect(clippy::print_stderr, reason = "the bootstrap's diagnostic channel")]
fn run_engine(args: &Args, validated: &amiss_bootstrap::Validated) -> ExitCode {
    let failure = ExitCode::from(2);
    let Some(mut expectations) = asked(&args.engine) else {
        eprintln!("amiss-bootstrap: invalid-engine-invocation");
        return failure;
    };
    expectations.engine_digest = validated.engine_digest.to_string();

    let Ok(private) = tempfile::TempDir::new() else {
        eprintln!("amiss-bootstrap: private-storage-unavailable");
        return failure;
    };
    let engine = private.path().join("engine");
    if std::fs::write(&engine, &validated.binary).is_err() || executable_bit(&engine).is_err() {
        eprintln!("amiss-bootstrap: private-storage-unavailable");
        return failure;
    }

    let launched = std::process::Command::new(&engine)
        .args(&args.engine)
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn();
    let Ok(mut child) = launched else {
        eprintln!("amiss-bootstrap: engine-launch-failed");
        return failure;
    };

    let Ok((outcome, wire)) = collect(&mut child) else {
        eprintln!("amiss-bootstrap: engine-launch-failed");
        return failure;
    };
    match settle(&outcome, &wire, &expectations) {
        Ok(class) => publish(&wire, class),
        Err(defect) => {
            eprintln!("amiss-bootstrap: {}", refused(defect));
            failure
        }
    }
}

/// Drains the engine's stdout while the watchdog runs. A supervisor that only
/// polls would deadlock the moment the engine's report outgrew the pipe
/// buffer: the engine would block writing, never exit, and be killed for a
/// slowness that was the supervisor's own.
fn collect(child: &mut std::process::Child) -> std::io::Result<(Supervised, Vec<u8>)> {
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| std::io::Error::other("no engine stdout"))?;
    let reader = std::thread::spawn(move || {
        let mut wire = Vec::new();
        let mut bounded = stdout.take(MACHINE_JSON_BYTES.saturating_add(1));
        bounded.read_to_end(&mut wire).map(|_count| wire)
    });
    let outcome = supervise(child, WATCHDOG_CEILING)?;
    let wire = reader
        .join()
        .map_err(|_panic| std::io::Error::other("engine reader failed"))??;
    Ok((outcome, wire))
}

/// The accepted envelope is republished byte for byte, and the wrapper exits
/// with the class the engine claimed and was held to.
fn publish(wire: &[u8], class: i64) -> ExitCode {
    let mut out = std::io::stdout().lock();
    if out.write_all(wire).is_err() || out.flush().is_err() {
        return ExitCode::from(2);
    }
    u8::try_from(class).map_or(ExitCode::from(2), ExitCode::from)
}

const fn refused(defect: Defect) -> &'static str {
    match defect {
        Defect::Killed => "evaluator-watchdog-kill",
        Defect::Signalled => "evaluator-signalled",
        Defect::Oversize => "report-over-wire-ceiling",
        Defect::ExitMismatch => "evaluator-exit-mismatch",
        Defect::Acceptance(AcceptanceDefect::Shape) => "report-shape",
        Defect::Acceptance(AcceptanceDefect::Noncanonical) => "report-noncanonical",
        Defect::Acceptance(AcceptanceDefect::PayloadDigest) => "report-payload-digest",
        Defect::Acceptance(AcceptanceDefect::Engine) => "report-engine-mismatch",
        Defect::Acceptance(AcceptanceDefect::BaseIdentity) => "report-base-mismatch",
        Defect::Acceptance(AcceptanceDefect::CandidateIdentity) => "report-candidate-mismatch",
        Defect::Acceptance(AcceptanceDefect::Completeness) => "report-completeness",
        Defect::Acceptance(AcceptanceDefect::FindingCount) => "report-finding-count",
    }
}

#[cfg(unix)]
fn executable_bit(path: &std::path::Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn executable_bit(_path: &std::path::Path) -> std::io::Result<()> {
    Ok(())
}
