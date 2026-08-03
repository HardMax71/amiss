use std::ffi::{OsStr, OsString};
use std::fmt::Display;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// The version is the caller's own, so a service names its crate and not this launcher.
#[expect(
    clippy::print_stderr,
    clippy::print_stdout,
    reason = "the provider service command-line contract"
)]
pub fn service_main<C, LoadError, RunError, RunFuture>(
    name: &str,
    version: &str,
    load: impl FnOnce(&Path) -> Result<C, LoadError>,
    run: impl FnOnce(C) -> RunFuture,
) -> ExitCode
where
    LoadError: Display,
    RunError: Display,
    RunFuture: Future<Output = Result<(), RunError>>,
{
    let arguments: Vec<OsString> = std::env::args_os().skip(1).collect();
    if version_request(&arguments) {
        println!("{name} {version}");
        return ExitCode::SUCCESS;
    }
    let Some((path, check_only)) = config_path(&arguments) else {
        eprintln!("{name}: expected ABS_CONFIG, --check ABS_CONFIG, or --version");
        return ExitCode::FAILURE;
    };
    let config = match load(&path) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("{name}: {error}");
            return ExitCode::FAILURE;
        }
    };
    if check_only {
        println!("{name}: configuration valid");
        return ExitCode::SUCCESS;
    }
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(_defect) => {
            eprintln!("{name}: runtime unavailable");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(run(config)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{name}: {error}");
            ExitCode::FAILURE
        }
    }
}

fn version_request(arguments: &[OsString]) -> bool {
    matches!(arguments, [only] if only.to_str() == Some("--version"))
}

fn config_path(arguments: &[OsString]) -> Option<(PathBuf, bool)> {
    let mut arguments = arguments.iter();
    let first = arguments.next()?;
    let check_only = first == OsStr::new("--check");
    let path = PathBuf::from(if check_only { arguments.next()? } else { first });
    (arguments.next().is_none() && path.is_absolute()).then_some((path, check_only))
}

#[path = "../tests/internal/launcher.rs"]
mod tests;
