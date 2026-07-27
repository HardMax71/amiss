#![forbid(unsafe_code)]

fn main() -> std::process::ExitCode {
    amiss_controller_service::service_main(
        "amiss-controller-github",
        env!("CARGO_PKG_VERSION"),
        amiss_controller_github_service::ServiceConfig::load,
        amiss_controller_github_service::run,
    )
}
