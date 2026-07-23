#![forbid(unsafe_code)]

fn main() -> std::process::ExitCode {
    amiss_controller_service::service_main(
        "amiss-controller-gitlab",
        amiss_controller_gitlab_service::ServiceConfig::load,
        amiss_controller_gitlab_service::run,
    )
}
