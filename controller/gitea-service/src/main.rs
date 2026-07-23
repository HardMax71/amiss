#![forbid(unsafe_code)]

fn main() -> std::process::ExitCode {
    amiss_controller_service::service_main(
        "amiss-controller-gitea",
        amiss_controller_gitea_service::ServiceConfig::load,
        amiss_controller_gitea_service::run,
    )
}
