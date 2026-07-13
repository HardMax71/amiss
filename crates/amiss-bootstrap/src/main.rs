use std::env;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use amiss_bootstrap::{Refusal, validate};
use amiss_git::{GitLimits, GitResources, Repository};
use amiss_wire::controls::ExecutionConstraintDescriptor;

/// The trusted bootstrap: it validates the pinned action tree as data and
/// only then execs the verified native engine. It never runs the action's
/// declared Node launcher, never resolves a binary through `PATH`, and never
/// downloads, installs, or discovers anything.
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

    exec_engine(&parsed, &validated)
}

const fn reason(refusal: Refusal) -> &'static str {
    match refusal {
        Refusal::UnsupportedPlatform => "unsupported-platform",
        Refusal::ActionTree => "action-tree-mismatch",
        Refusal::ActionMetadata => "action-metadata-invalid",
        Refusal::PathNotRegularBlob => "path-not-regular-blob",
        Refusal::ManifestUnreadable => "manifest-unreadable",
        Refusal::ManifestDigest => "manifest-digest-mismatch",
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

/// Writes the verified engine bytes into a private directory and execs them
/// with an empty environment. The bytes come from the validated tree, never
/// from a worktree file, a `PATH` lookup, or the action's launcher.
#[expect(clippy::print_stderr, reason = "the bootstrap's diagnostic channel")]
fn exec_engine(args: &Args, validated: &amiss_bootstrap::Validated) -> ExitCode {
    let failure = ExitCode::from(2);
    let Ok(private) = tempfile::TempDir::new() else {
        eprintln!("amiss-bootstrap: private-storage-unavailable");
        return failure;
    };
    let engine = private.path().join("engine");
    if std::fs::write(&engine, &validated.binary).is_err() {
        eprintln!("amiss-bootstrap: private-storage-unavailable");
        return failure;
    }
    if executable_bit(&engine).is_err() {
        eprintln!("amiss-bootstrap: private-storage-unavailable");
        return failure;
    }
    let mut command = std::process::Command::new(&engine);
    command
        .args(&args.engine)
        .env_clear()
        .env("TMPDIR", private.path())
        .stdin(std::process::Stdio::null());
    match command.status() {
        Ok(status) => status
            .code()
            .and_then(|code| u8::try_from(code).ok())
            .map_or(failure, ExitCode::from),
        Err(_defect) => {
            eprintln!("amiss-bootstrap: engine-launch-failed");
            failure
        }
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
