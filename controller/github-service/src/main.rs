#![forbid(unsafe_code)]

use std::path::PathBuf;

fn main() -> std::process::ExitCode {
    let Some(path) = config_path() else {
        eprintln!("amiss-controller-github: expected one absolute config path");
        return std::process::ExitCode::FAILURE;
    };
    let config = match amiss_controller_github_service::ServiceConfig::load(&path) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("amiss-controller-github: {error}");
            return std::process::ExitCode::FAILURE;
        }
    };
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(_defect) => {
            eprintln!("amiss-controller-github: runtime unavailable");
            return std::process::ExitCode::FAILURE;
        }
    };
    match runtime.block_on(amiss_controller_github_service::run(config)) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("amiss-controller-github: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn config_path() -> Option<PathBuf> {
    let mut arguments = std::env::args_os();
    let _program = arguments.next()?;
    let path = PathBuf::from(arguments.next()?);
    (arguments.next().is_none() && path.is_absolute()).then_some(path)
}
