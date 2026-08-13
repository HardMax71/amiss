use std::fs;
use std::io::Read as _;
use std::process::ExitCode;

use amiss_wire::ExitClass;
use amiss_wire::external::PlanDefect;
use amiss_wire::json::{self, Value};
use amiss_wire::report::{FatalSerializer, MACHINE_JSON_BYTES};

use crate::invocation::{OutputFormat, PlanInvocation};
use crate::view::View;

/// Refusals are diagnostics; the machine lane emits through the reserve
/// serializer and the human projection prints its own lines.
#[expect(clippy::print_stderr, reason = "refusals are diagnostics")]
pub(crate) fn run(invocation: &PlanInvocation, reserve: &mut FatalSerializer) -> ExitCode {
    let failure = ExitCode::from(ExitClass::Failure.code());
    let report = invocation.report.display();
    let Ok(file) = fs::File::open(&invocation.report) else {
        eprintln!("amiss external-plan: {report} is unreadable");
        return failure;
    };
    let mut bytes = Vec::new();
    let bounded = file
        .take(MACHINE_JSON_BYTES.saturating_add(1))
        .read_to_end(&mut bytes);
    if bounded.is_err() {
        eprintln!("amiss external-plan: {report} is unreadable");
        return failure;
    }
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MACHINE_JSON_BYTES {
        eprintln!("amiss external-plan: {report} is larger than a scanner report can be");
        return failure;
    }
    let Ok(parsed) = json::parse(&bytes) else {
        eprintln!("amiss external-plan: the report is not the scanner's strict JSON");
        return failure;
    };
    let Some(engine) = crate::engine_provenance() else {
        eprintln!(
            "amiss: {}",
            amiss_wire::report::AnalysisErrorCode::InternalError.as_str()
        );
        return failure;
    };
    let planned = amiss_wire::external::plan(&parsed, &engine.version, &engine.digest.to_string());
    let envelope = match planned {
        Ok(envelope) => envelope,
        Err(defect) => {
            eprintln!("amiss external-plan: {}", describe(defect));
            return failure;
        }
    };
    match invocation.format {
        // A closed pipe means the consumer stopped reading, never a lost plan;
        // any other write defect did lose bytes, so the exit says so.
        OutputFormat::Json => {
            if let Err(defect) = reserve.emit(&envelope, &mut std::io::stdout())
                && defect.kind() != std::io::ErrorKind::BrokenPipe
            {
                eprintln!("amiss external-plan: the plan could not be written");
                return failure;
            }
        }
        // The grammar admits human and json only; the other two never reach here.
        OutputFormat::Human | OutputFormat::Sarif | OutputFormat::CodeQuality => human(&envelope),
    }
    ExitCode::from(ExitClass::Success.code())
}

/// The command's own wording for each refusal; the wire enum stays data.
const fn describe(defect: PlanDefect) -> &'static str {
    match defect {
        PlanDefect::NotAReport => "the input is not a scanner report envelope",
        PlanDefect::DigestMismatch => "the report payload does not match its recorded digest",
        PlanDefect::Incomplete => "the report is incomplete, so its sides cannot be compared",
        PlanDefect::MalformedExternal => {
            "an external occurrence is missing its destination, document, or scheme"
        }
    }
}

/// The human projection: one totals line, then up to ten introduced
/// destinations with an overflow line, the machine payload untouched.
#[expect(clippy::print_stdout, reason = "the human projection's output channel")]
fn human(envelope: &Value) {
    let payload = View::of(Some(envelope)).view("payload");
    let introduced = payload.rows("introduced");
    println!(
        "amiss external-plan: introduced {} removed {} retained {}",
        introduced.len(),
        payload.rows("removed").len(),
        payload.number("retained_count"),
    );
    for row in introduced.iter().take(10) {
        println!(
            "introduced {} in {} documents",
            row.text("destination"),
            row.rows("documents").len(),
        );
    }
    let overflow = introduced.len().saturating_sub(10);
    if overflow > 0 {
        println!("introduced overflow: {overflow} more in the full plan");
    }
}
