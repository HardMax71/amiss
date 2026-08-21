use std::io::{Read as _, Write as _};
use std::process::Stdio;
use std::time::Duration;

use amiss_bootstrap::result::BootstrapResult;
use amiss_bootstrap::supervise::{Expectations, Supervised, settle, supervise};
use amiss_wire::report::{MACHINE_JSON_BYTES, WATCHDOG_MILLISECONDS};
use amiss_wire::requests::{RequestStreams, SEALED_ENGINE_ARGUMENT};

use super::{Accepted, Args, Execution, SealedRun, settlement_failure, tampered, unavailable};

/// The operational wall ceiling from the security contract: the trusted
/// wrapper kills the whole evaluator after 120 seconds, and a killed evaluator
/// yields no accepted result.
const WATCHDOG_CEILING: Duration = Duration::from_millis(WATCHDOG_MILLISECONDS);

#[cfg(windows)]
const PRIVATE_ENGINE_NAME: &str = "engine.exe";

#[cfg(not(windows))]
const PRIVATE_ENGINE_NAME: &str = "engine";

/// Writes the verified engine bytes into a private directory and launches them
/// with an empty environment. The bytes come from the validated tree, never
/// from a worktree file, a `PATH` lookup, or the action's launcher.
pub(super) fn run(
    args: &Args,
    validated: &amiss_bootstrap::Validated,
    sealed: SealedRun,
) -> Execution<Accepted> {
    let expectations = Expectations {
        engine_digest: validated.engine_digest.to_string(),
        base_commit: sealed.evaluation.base_commit.as_str().to_owned(),
        candidate_commit: sealed
            .evaluation
            .candidate_commit
            .as_ref()
            .map(|candidate| candidate.as_str().to_owned()),
        sealed: Some(sealed.expected.clone()),
    };

    let private = tempfile::TempDir::new_in(&args.scratch)
        .map_err(|_defect| unavailable("private-storage-unavailable"))?;
    let engine = private.path().join(PRIVATE_ENGINE_NAME);
    std::fs::write(&engine, &validated.binary)
        .map_err(|_defect| unavailable("private-storage-unavailable"))?;
    executable_bit(&engine).map_err(|_defect| unavailable("private-storage-unavailable"))?;

    let mut child = std::process::Command::new(&engine)
        .arg(SEALED_ENGINE_ARGUMENT)
        .current_dir(&args.repository)
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|_defect| unavailable("engine-launch-failed"))?;
    let (outcome, wire) = collect(&mut child, sealed.streams)
        .map_err(|_defect| unavailable("engine-collection-failed"))?;
    let class = settle(&outcome, &wire, &expectations)
        .map_err(|defect| settlement_failure(defect, wire.is_empty()))?;
    let (class, result) = match class {
        0 => (0, BootstrapResult::Pass),
        1 => (1, BootstrapResult::Block),
        _ => return Err(tampered("report-exit-class")),
    };
    Ok(Accepted {
        wire,
        class,
        result,
    })
}

/// Drains the engine's stdout while the watchdog runs. A supervisor that only
/// polls would deadlock the moment the engine's report outgrew the pipe
/// buffer: the engine would block writing, never exit, and be killed for a
/// slowness that was the supervisor's own.
fn collect(
    child: &mut std::process::Child,
    requests: RequestStreams,
) -> std::io::Result<(Supervised, Vec<u8>)> {
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| std::io::Error::other("no engine stdin"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| std::io::Error::other("no engine stdout"))?;
    let writer = std::thread::spawn(move || {
        requests.write_to(&mut stdin)?;
        stdin.flush()
    });
    let reader = std::thread::spawn(move || {
        let mut wire = Vec::new();
        let mut bounded = stdout.take(MACHINE_JSON_BYTES.saturating_add(1));
        bounded.read_to_end(&mut wire).map(|_count| wire)
    });
    let outcome = match supervise(child, WATCHDOG_CEILING) {
        Ok(outcome) => outcome,
        Err(defect) => {
            let _signalled = child.kill();
            let _reaped = child.wait();
            let _writer = writer.join();
            let _reader = reader.join();
            return Err(defect);
        }
    };
    let write_result = writer
        .join()
        .map_err(|_panic| std::io::Error::other("engine request writer failed"));
    if !matches!(outcome, Supervised::Killed) {
        write_result??;
    }
    let wire = reader
        .join()
        .map_err(|_panic| std::io::Error::other("engine reader failed"))??;
    Ok((outcome, wire))
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
