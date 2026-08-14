mod net;
mod select;

use std::env;
use std::fs;
use std::io::Read as _;
use std::process::ExitCode;
use std::time::{Duration, Instant, SystemTime};

use amiss_wire::external::{evidence_file, probe_evidence_row};
use amiss_wire::json::{self, Value};
use amiss_wire::report::MACHINE_JSON_BYTES;

use crate::net::{Observation, probe, shown};
use crate::select::targets;

const USAGE: &str = "amiss-probe --plan <path>
amiss-probe --version";

/// Destinations probed per run; everything past the cap stays unproven.
const RUN_CAP: usize = 64;

/// The run's wall ceiling, consulted between destinations and between
/// requests; one resolver lookup or in-flight request can overhang it.
const RUN_BUDGET: Duration = Duration::from_mins(2);

/// Diagnostics are the stderr channel and the evidence is stdout; nothing
/// else leaves the process.
#[expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "the evidence is the output and refusals are diagnostics"
)]
fn main() -> ExitCode {
    let arguments: Vec<String> = env::args().skip(1).collect();
    let plan_path = match arguments.as_slice() {
        [version] if version == "--version" => {
            println!("amiss-probe {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        [flag, path] if flag == "--plan" && !path.is_empty() => path,
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    let Some(plan) = strict_plan(plan_path) else {
        return ExitCode::from(2);
    };
    let Ok(elapsed) = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) else {
        eprintln!("amiss-probe: the clock answers before the epoch");
        return ExitCode::from(2);
    };
    let checked_at = elapsed.as_millis().to_string();

    let (selected, capped) = targets(&plan, RUN_CAP);
    let started = Instant::now();
    let deadline = started.checked_add(RUN_BUDGET);
    let Some(deadline) = deadline else {
        eprintln!("amiss-probe: the clock cannot hold the run budget");
        return ExitCode::from(2);
    };
    let mut skipped = capped;
    let mut rows = Vec::new();
    for (position, destination) in selected.iter().enumerate() {
        if started.elapsed() >= RUN_BUDGET {
            skipped = skipped.saturating_add(selected.len().saturating_sub(position));
            break;
        }
        let row = match probe(destination, deadline) {
            Observation::Answered {
                method,
                status,
                final_destination,
            } => probe_evidence_row(
                destination,
                method,
                Some(status),
                None,
                final_destination.as_deref(),
                &checked_at,
            ),
            Observation::Failed { method, failure } => {
                probe_evidence_row(destination, method, None, Some(failure), None, &checked_at)
            }
            // Policy refusals state no observation, but they do get named.
            Observation::Refused => {
                eprintln!(
                    "amiss-probe: {} refused by the address policy, stays unproven",
                    shown(destination)
                );
                continue;
            }
        };
        rows.push(row);
    }
    if skipped > 0 {
        eprintln!("amiss-probe: {skipped} destinations past the run cap or budget stay unproven");
    }
    let Some(evidence) = evidence_file(&plan, "amiss-probe", env!("CARGO_PKG_VERSION"), rows)
    else {
        eprintln!("amiss-probe: the plan names no payload digest");
        return ExitCode::from(2);
    };
    let mut out = String::new();
    json::stream(&evidence, &mut out);
    println!("{out}");
    ExitCode::SUCCESS
}

/// One bounded read and strict parse of a digest-whole plan.
#[expect(clippy::print_stderr, reason = "refusals are diagnostics")]
fn strict_plan(path: &str) -> Option<Value> {
    let Ok(file) = fs::File::open(path) else {
        eprintln!("amiss-probe: {path} is unreadable");
        return None;
    };
    let mut bytes = Vec::new();
    let bounded = file
        .take(MACHINE_JSON_BYTES.saturating_add(1))
        .read_to_end(&mut bytes);
    if bounded.is_err() || u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MACHINE_JSON_BYTES {
        eprintln!("amiss-probe: {path} is unreadable or larger than a plan can be");
        return None;
    }
    let Ok(plan) = json::parse(&bytes) else {
        eprintln!("amiss-probe: {path} is not the scanner's strict JSON");
        return None;
    };
    if !amiss_wire::external::bound_plan(&plan) {
        eprintln!("amiss-probe: {path} is not a digest-whole external plan");
        return None;
    }
    Some(plan)
}
