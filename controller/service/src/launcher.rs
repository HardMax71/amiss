use std::fmt::Display;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Loads and runs one provider service from its sole absolute config argument.
pub fn service_main<C, LoadError, RunError, RunFuture>(
    name: &str,
    load: impl FnOnce(&Path) -> Result<C, LoadError>,
    run: impl FnOnce(C) -> RunFuture,
) -> ExitCode
where
    LoadError: Display,
    RunError: Display,
    RunFuture: Future<Output = Result<(), RunError>>,
{
    let Some(path) = config_path() else {
        eprintln!("{name}: expected one absolute config path");
        return ExitCode::FAILURE;
    };
    let config = match load(&path) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("{name}: {error}");
            return ExitCode::FAILURE;
        }
    };
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

fn config_path() -> Option<PathBuf> {
    let mut arguments = std::env::args_os();
    let _program = arguments.next()?;
    let path = PathBuf::from(arguments.next()?);
    (arguments.next().is_none() && path.is_absolute()).then_some(path)
}
