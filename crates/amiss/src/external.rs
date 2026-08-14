use std::fs;
use std::io::Read as _;
use std::path::Path;
use std::process::ExitCode;

use amiss_wire::ExitClass;
use amiss_wire::external::{AssessDefect, PlanDefect};
use amiss_wire::json::{self, Value};
use amiss_wire::report::{FatalSerializer, MACHINE_JSON_BYTES};

use crate::invocation::{AssessInvocation, OutputFormat, PlanInvocation};
use crate::view::View;

pub(crate) fn run_plan(invocation: &PlanInvocation, reserve: &mut FatalSerializer) -> ExitCode {
    run_pure(
        "external-plan",
        &[&invocation.report],
        invocation.format,
        reserve,
        |values, version, digest| {
            let [report] = values else {
                return Err("the input set is not one report");
            };
            amiss_wire::external::plan(report, version, digest).map_err(describe_plan)
        },
    )
}

pub(crate) fn run_assess(invocation: &AssessInvocation, reserve: &mut FatalSerializer) -> ExitCode {
    run_pure(
        "external-assess",
        &[&invocation.plan, &invocation.evidence],
        invocation.format,
        reserve,
        |values, version, digest| {
            let [plan, evidence] = values else {
                return Err("the input set is not a plan and its evidence");
            };
            amiss_wire::external::assess(plan, evidence, version, digest).map_err(describe_assess)
        },
    )
}

#[expect(clippy::print_stderr, reason = "refusals are diagnostics")]
fn run_pure(
    command: &str,
    inputs: &[&Path],
    format: OutputFormat,
    reserve: &mut FatalSerializer,
    derive: impl FnOnce(&[Value], &str, &str) -> Result<Value, &'static str>,
) -> ExitCode {
    let failure = ExitCode::from(ExitClass::Failure.code());
    let mut values = Vec::new();
    for path in inputs {
        let Some(value) = strict_value(command, path) else {
            return failure;
        };
        values.push(value);
    }
    let Some(engine) = crate::engine_provenance() else {
        return internal_error();
    };
    match derive(&values, &engine.version, &engine.digest.to_string()) {
        Ok(envelope) => project(command, &envelope, format, reserve),
        Err(defect) => {
            eprintln!("amiss {command}: {defect}");
            failure
        }
    }
}

/// The writer caps an envelope at `MACHINE_JSON_BYTES`, so a larger input
/// is provably not the scanner's artifact.
#[expect(clippy::print_stderr, reason = "refusals are diagnostics")]
fn strict_value(command: &str, path: &Path) -> Option<Value> {
    let shown = path.display();
    let Ok(file) = fs::File::open(path) else {
        eprintln!("amiss {command}: {shown} is unreadable");
        return None;
    };
    let mut bytes = Vec::new();
    let bounded = file
        .take(MACHINE_JSON_BYTES.saturating_add(1))
        .read_to_end(&mut bytes);
    if bounded.is_err() {
        eprintln!("amiss {command}: {shown} is unreadable");
        return None;
    }
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MACHINE_JSON_BYTES {
        eprintln!("amiss {command}: {shown} is larger than a scanner report can be");
        return None;
    }
    let Ok(parsed) = json::parse(&bytes) else {
        eprintln!("amiss {command}: {shown} is not the scanner's strict JSON");
        return None;
    };
    Some(parsed)
}

#[expect(clippy::print_stderr, reason = "refusals are diagnostics")]
fn internal_error() -> ExitCode {
    eprintln!(
        "amiss: {}",
        amiss_wire::report::AnalysisErrorCode::InternalError.as_str()
    );
    ExitCode::from(ExitClass::Failure.code())
}

/// A closed pipe never fails the exit; any other write defect lost bytes.
#[expect(clippy::print_stderr, reason = "refusals are diagnostics")]
fn project(
    command: &str,
    envelope: &Value,
    format: OutputFormat,
    reserve: &mut FatalSerializer,
) -> ExitCode {
    match format {
        OutputFormat::Json => {
            if let Err(defect) = reserve.emit(envelope, &mut std::io::stdout())
                && defect.kind() != std::io::ErrorKind::BrokenPipe
            {
                eprintln!("amiss {command}: the artifact could not be written");
                return ExitCode::from(ExitClass::Failure.code());
            }
        }
        // The grammar admits human and json only; the other two never reach here.
        OutputFormat::Human | OutputFormat::Sarif | OutputFormat::CodeQuality => {
            if command == "external-plan" {
                human_plan(envelope);
            } else {
                human_assessment(envelope);
            }
        }
    }
    ExitCode::from(ExitClass::Success.code())
}

/// The command's own wording for each refusal; the wire enums stay data.
const fn describe_plan(defect: PlanDefect) -> &'static str {
    match defect {
        PlanDefect::NotAReport => "the input is not a scanner report envelope",
        PlanDefect::DigestMismatch => "the report payload does not match its recorded digest",
        PlanDefect::Incomplete => "the report is incomplete, so its sides cannot be compared",
        PlanDefect::MalformedExternal => {
            "an external occurrence is missing its destination, document, or scheme"
        }
    }
}

const fn describe_assess(defect: AssessDefect) -> &'static str {
    match defect {
        AssessDefect::NotAPlan => "the input is not an external plan envelope",
        AssessDefect::PlanDigestMismatch => "the plan payload does not match its recorded digest",
        AssessDefect::NotEvidence => "the input is not an external evidence file",
        AssessDefect::UnboundEvidence => {
            "the evidence binds another plan, repeats a destination, names one the plan did not \
             introduce, or resolves a tail the plan's shape does not carry"
        }
        AssessDefect::MalformedEvidence => "an evidence row breaks its own kind's grammar",
    }
}

fn human_plan(envelope: &Value) {
    let mut out = crate::Channel::new();
    let payload = View::of(Some(envelope)).view("payload");
    let introduced = payload.rows("introduced");
    out.line(format_args!(
        "amiss external-plan: introduced {} removed {} retained {}",
        introduced.len(),
        payload.rows("removed").len(),
        payload.number("retained_count"),
    ));
    for row in introduced.iter().take(10) {
        out.line(format_args!(
            "introduced {} in {} documents",
            row.text("destination"),
            row.rows("documents").len(),
        ));
    }
    let overflow = introduced.len().saturating_sub(10);
    if overflow > 0 {
        out.line(format_args!(
            "introduced overflow: {overflow} more in the full plan"
        ));
    }
}

fn human_assessment(envelope: &Value) {
    let mut out = crate::Channel::new();
    let payload = View::of(Some(envelope)).view("payload");
    let verdicts = payload.rows("verdicts");
    let count = |wanted: &str| {
        verdicts
            .iter()
            .filter(|row| row.text("verdict") == wanted)
            .count()
    };
    out.line(format_args!(
        "amiss external-assess: refuted {} unproven {} reachable {}",
        count("refuted"),
        count("unproven"),
        count("reachable"),
    ));
    let refuted: Vec<&View> = verdicts
        .iter()
        .filter(|row| row.text("verdict") == "refuted")
        .collect();
    for row in refuted.iter().take(10) {
        out.line(format_args!(
            "refuted {} ({})",
            row.text("destination"),
            row.text("reason")
        ));
    }
    let overflow = refuted.len().saturating_sub(10);
    if overflow > 0 {
        out.line(format_args!(
            "refuted overflow: {overflow} more in the full assessment"
        ));
    }
}
