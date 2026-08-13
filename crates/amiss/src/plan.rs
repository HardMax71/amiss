use std::fs;
use std::process::ExitCode;

use amiss_wire::ExitClass;
use amiss_wire::json::{self, Value};
use amiss_wire::report::FatalSerializer;

use crate::invocation::{OutputFormat, PlanInvocation};
use crate::view::View;

/// Refusals are diagnostics; the machine lane emits through the reserve
/// serializer and the human projection prints its own lines.
#[expect(clippy::print_stderr, reason = "refusals are diagnostics")]
pub(crate) fn run(invocation: &PlanInvocation, reserve: &mut FatalSerializer) -> ExitCode {
    let failure = ExitCode::from(ExitClass::Failure.code());
    let Ok(bytes) = fs::read(&invocation.report) else {
        eprintln!(
            "amiss external-plan: {} is unreadable",
            invocation.report.display()
        );
        return failure;
    };
    let Ok(report) = json::parse(&bytes) else {
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
    let planned = amiss_wire::external::plan(&report, &engine.version, &engine.digest.to_string());
    let envelope = match planned {
        Ok(envelope) => envelope,
        Err(defect) => {
            eprintln!("amiss external-plan: {}", defect.describe());
            return failure;
        }
    };
    match invocation.format {
        OutputFormat::Json => crate::emit(reserve, &envelope),
        // The grammar admits human and json only; the other two never reach here.
        OutputFormat::Human | OutputFormat::Sarif | OutputFormat::CodeQuality => human(&envelope),
    }
    ExitCode::from(ExitClass::Success.code())
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
