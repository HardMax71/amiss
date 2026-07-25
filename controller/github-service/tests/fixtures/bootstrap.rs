use std::process::ExitCode;

fn main() -> ExitCode {
    amiss_controller_bootstrap_fixture::run(b"{\"provider_lane\":\"pass\"}\n")
}
