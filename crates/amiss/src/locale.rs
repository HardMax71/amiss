use std::path::Path;
use std::process::ExitCode;

use amiss_wire::envelope::Payload as _;
use amiss_wire::locale::{
    EVIDENCE_DOCUMENT_BYTES, LOCALE_DOCUMENT_BYTES, LocaleCoverageAssessment,
    LocaleCoverageEvidence, LocaleCoveragePlan, assess,
};

use crate::input::{ReadError, bounded_bytes};
use crate::invocation::AssessInvocation;

pub(crate) fn run(invocation: &AssessInvocation) -> ExitCode {
    crate::pure::run(
        "locale-assess",
        invocation.format,
        || {
            Ok((
                document(&invocation.plan, LOCALE_DOCUMENT_BYTES, "a coverage plan")?,
                document(
                    &invocation.evidence,
                    EVIDENCE_DOCUMENT_BYTES,
                    "coverage evidence",
                )?,
            ))
        },
        |(plan, evidence), version, digest| {
            let plan = LocaleCoveragePlan::parse(&plan)?;
            let evidence = LocaleCoverageEvidence::parse(&evidence)?;
            assess(&plan, Some(&evidence), version, digest)
        },
        |bytes| {
            LocaleCoverageAssessment::parse(bytes)
                .map(|document| crate::human::coverage(&document.payload))
                .map_err(|defect| defect.to_string())
        },
    )
}

fn document(path: &Path, limit: u64, shape: &str) -> Result<Vec<u8>, String> {
    let shown = path.display();
    bounded_bytes(path, limit).map_err(|error| match error {
        ReadError::Unreadable => format!("{shown} is unreadable"),
        ReadError::TooLarge => format!("{shown} is larger than {shape} can be"),
    })
}
