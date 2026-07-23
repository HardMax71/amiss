use std::ffi::{OsStr, OsString};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use amiss_bootstrap::result::{BootstrapResult, result_bytes, result_exit_code};
use amiss_wire::controls::ExecutionConstraintDescriptor;

fn main() -> ExitCode {
    let Some(output) = output_paths(std::env::args_os().skip(1)) else {
        return ExitCode::from(2);
    };
    let Some(mode) = std::fs::read(&output.constraint)
        .ok()
        .and_then(|bytes| ExecutionConstraintDescriptor::parse(&bytes).ok())
        .map(|constraint| constraint.required_status_name)
    else {
        return ExitCode::from(2);
    };
    match mode.as_str() {
        "runner-pass" => complete(&output, BootstrapResult::Pass),
        "runner-block" => complete(&output, BootstrapResult::Block),
        "runner-missing" => ExitCode::SUCCESS,
        "runner-hang" => loop {
            std::thread::sleep(Duration::from_mins(1));
        },
        _ => ExitCode::from(2),
    }
}

struct OutputPaths {
    constraint: PathBuf,
    report: PathBuf,
    result: PathBuf,
}

fn output_paths(mut arguments: impl Iterator<Item = OsString>) -> Option<OutputPaths> {
    literal(&mut arguments, "exec")?;
    let _action = value(&mut arguments, "--action-repository")?;
    let _repository = value(&mut arguments, "--repository")?;
    let constraint = value(&mut arguments, "--constraint")?;
    let _evaluation = value(&mut arguments, "--evaluation-request")?;
    let _snapshot = value(&mut arguments, "--snapshot-request")?;
    let _controls = value(&mut arguments, "--controls-request")?;
    let _scratch = value(&mut arguments, "--scratch")?;
    let report = value(&mut arguments, "--report")?;
    let result = value(&mut arguments, "--result")?;
    arguments.next().is_none().then_some(OutputPaths {
        constraint,
        report,
        result,
    })
}

fn value(arguments: &mut impl Iterator<Item = OsString>, expected: &str) -> Option<PathBuf> {
    literal(arguments, expected)?;
    arguments.next().map(PathBuf::from)
}

fn literal(arguments: &mut impl Iterator<Item = OsString>, expected: &str) -> Option<()> {
    (arguments.next()?.as_os_str() == OsStr::new(expected)).then_some(())
}

fn complete(paths: &OutputPaths, result: BootstrapResult) -> ExitCode {
    let written = std::fs::write(&paths.report, b"{\"provider_lane\":\"gitlab\"}\n")
        .and_then(|()| std::fs::write(&paths.result, result_bytes(result)));
    if written.is_err() {
        return ExitCode::from(2);
    }
    u8::try_from(result_exit_code(result)).map_or(ExitCode::from(2), ExitCode::from)
}
