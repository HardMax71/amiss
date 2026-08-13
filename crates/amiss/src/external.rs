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
        |values, version, digest| match values {
            [report] => amiss_wire::external::plan(report, version, digest).map_err(describe_plan),
            [..] => Err("the input set is not one report"),
        },
    )
}

pub(crate) fn run_assess(invocation: &AssessInvocation, reserve: &mut FatalSerializer) -> ExitCode {
    run_pure(
        "external-assess",
        &[&invocation.plan, &invocation.evidence],
        invocation.format,
        reserve,
        |values, version, digest| match values {
            [plan, evidence] => amiss_wire::external::assess(plan, evidence, version, digest)
                .map_err(describe_assess),
            [..] => Err("the input set is not a plan and its evidence"),
        },
    )
}

/// One pure verb's whole run: bounded reads, provenance, derivation,
/// projection. Refusals are diagnostics; the machine lane emits through the
/// reserve serializer and the human projection prints its own lines.
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

/// One bounded read and strict parse: the scanner's own writer caps an
/// envelope at `MACHINE_JSON_BYTES`, so a larger input is provably not one
/// of its artifacts. Diagnostics carry the calling form's name.
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

/// A closed pipe means the consumer stopped reading, never a lost artifact;
/// any other write defect did lose bytes, so the exit says so.
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
            "the evidence binds another plan, repeats a destination, or names one the plan did not introduce"
        }
        AssessDefect::MalformedEvidence => "an evidence row breaks its own kind's grammar",
    }
}

/// The human projection: one totals line, then up to ten introduced
/// destinations with an overflow line, the machine payload untouched.
#[expect(clippy::print_stdout, reason = "the human projection's output channel")]
fn human_plan(envelope: &Value) {
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

/// The assessment's human window: totals, then up to ten refuted rows.
#[expect(clippy::print_stdout, reason = "the human projection's output channel")]
fn human_assessment(envelope: &Value) {
    let payload = View::of(Some(envelope)).view("payload");
    let verdicts = payload.rows("verdicts");
    let count = |wanted: &str| {
        verdicts
            .iter()
            .filter(|row| row.text("verdict") == wanted)
            .count()
    };
    println!(
        "amiss external-assess: refuted {} unproven {} reachable {}",
        count("refuted"),
        count("unproven"),
        count("reachable"),
    );
    let refuted: Vec<&View> = verdicts
        .iter()
        .filter(|row| row.text("verdict") == "refuted")
        .collect();
    for row in refuted.iter().take(10) {
        println!(
            "refuted {} ({})",
            row.text("destination"),
            row.text("reason")
        );
    }
    let overflow = refuted.len().saturating_sub(10);
    if overflow > 0 {
        println!("refuted overflow: {overflow} more in the full assessment");
    }
}
