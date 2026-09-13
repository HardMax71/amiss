use std::process::ExitCode;

use amiss_wire::external::{parse_assessment, parse_plan};

use crate::invocation::{AssessInvocation, PlanInvocation};

pub(crate) fn run_plan(invocation: &PlanInvocation) -> ExitCode {
    crate::pure::run(
        "external-plan",
        invocation.format,
        || crate::input::report_bytes(&invocation.report),
        |report, version, digest| amiss_wire::external::plan(&report, version, digest),
        |bytes| {
            parse_plan(bytes)
                .map(|document| crate::human::plan(&document.payload))
                .map_err(|defect| defect.to_string())
        },
    )
}

pub(crate) fn run_assess(invocation: &AssessInvocation) -> ExitCode {
    crate::pure::run(
        "external-assess",
        invocation.format,
        || {
            Ok((
                crate::input::report_bytes(&invocation.plan)?,
                crate::input::report_bytes(&invocation.evidence)?,
            ))
        },
        |(plan, evidence), version, digest| {
            amiss_wire::external::assess(&plan, &evidence, version, digest)
        },
        |bytes| {
            parse_assessment(bytes)
                .map(|document| crate::human::assessment(&document.payload))
                .map_err(|defect| defect.to_string())
        },
    )
}
