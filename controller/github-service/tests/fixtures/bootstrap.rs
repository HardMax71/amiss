use std::process::ExitCode;

fn main() -> ExitCode {
    amiss_controller_bootstrap_fixture::run(include_bytes!(
        "../../../../spec/examples/scanner-report.canonical.json"
    ))
}
