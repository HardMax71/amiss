use std::path::Path;
use std::process::ExitCode;

use amiss_git::{GitLimits, GitResources, Repository};
use amiss_scan::{LOCALE_CONTEXT_BYTES, LocaleTreeContext, tree_inventory};
use amiss_wire::de::Document as _;
use amiss_wire::envelope::Payload as _;
use amiss_wire::locale::{
    EVIDENCE_DOCUMENT_BYTES, LOCALE_DOCUMENT_BYTES, LocaleCoverageAssessment,
    LocaleCoverageEvidence, LocaleCoveragePlan, assess,
};
use amiss_wire::model::Digest;

use crate::input::{ReadError, bounded_bytes};
use crate::invocation::{AssessInvocation, InventoryInvocation};

/// Which locale form is running: both read the coverage plan plus one more
/// bounded document and project one artifact from the pair.
pub(crate) enum Form<'a> {
    Assess(&'a AssessInvocation),
    Inventory(&'a InventoryInvocation),
}

pub(crate) fn run(form: &Form<'_>) -> ExitCode {
    let plan = match form {
        Form::Assess(invocation) => &invocation.plan,
        Form::Inventory(invocation) => &invocation.plan,
    };
    let (command, format, second) = match form {
        Form::Assess(invocation) => (
            "locale-assess",
            invocation.format,
            (
                invocation.evidence.as_path(),
                EVIDENCE_DOCUMENT_BYTES,
                "coverage evidence",
            ),
        ),
        Form::Inventory(invocation) => (
            "locale-inventory",
            invocation.format,
            (
                invocation.context.as_path(),
                LOCALE_CONTEXT_BYTES,
                "a locale context",
            ),
        ),
    };
    crate::pure::run(
        command,
        format,
        || {
            Ok([
                document(plan, LOCALE_DOCUMENT_BYTES, "a coverage plan")?,
                document(second.0, second.1, second.2)?,
            ])
        },
        |[plan, second], version, digest| match form {
            Form::Assess(_) => assessed(&plan, &second, version, digest),
            Form::Inventory(invocation) => produce(invocation, &plan, &second),
        },
        |bytes| match form {
            Form::Assess(_) => LocaleCoverageAssessment::parse(bytes)
                .map(|document| crate::human::coverage(&document.payload))
                .map_err(|defect| defect.to_string()),
            Form::Inventory(_) => LocaleCoverageEvidence::parse(bytes)
                .map(|document| crate::human::inventory(&document.payload))
                .map_err(|defect| defect.to_string()),
        },
    )
}

fn assessed(
    plan: &[u8],
    evidence: &[u8],
    version: &str,
    digest: Digest,
) -> Result<Vec<u8>, String> {
    let plan = LocaleCoveragePlan::parse(plan).map_err(|defect| defect.to_string())?;
    let evidence = LocaleCoverageEvidence::parse(evidence).map_err(|defect| defect.to_string())?;
    assess(&plan, Some(&evidence), version, digest).map_err(|defect| defect.to_string())
}

fn document(path: &Path, limit: u64, shape: &str) -> Result<Vec<u8>, String> {
    let shown = path.display();
    bounded_bytes(path, limit).map_err(|error| match error {
        ReadError::Unreadable => format!("{shown} is unreadable"),
        ReadError::TooLarge => format!("{shown} is larger than {shape} can be"),
    })
}

fn produce(
    invocation: &InventoryInvocation,
    plan: &[u8],
    context: &[u8],
) -> Result<Vec<u8>, String> {
    let context = LocaleTreeContext::parse(context)
        .map_err(|defect| format!("the context is invalid: {defect}"))?;
    let object_format = LocaleCoveragePlan::parse(plan)
        .map_err(|defect| defect.to_string())?
        .payload
        .docs
        .object_format;
    let repo = Repository::open(&invocation.repo, object_format)
        .map_err(|_defect| format!("{} is not a readable repository", invocation.repo.display()))?;
    let mut git = GitResources::new(GitLimits::CONTRACT);
    tree_inventory(&repo, &mut git, plan, &context).map_err(|defect| match defect {
        amiss_scan::InventoryError::Context => "the context contradicts the plan".to_owned(),
        amiss_scan::InventoryError::Plan => {
            "the plan is unreadable or binds another snapshot".to_owned()
        }
        amiss_scan::InventoryError::Snapshot(defect) => defect.code().as_ref().to_owned(),
        amiss_scan::InventoryError::Evidence => {
            "the inventory leaves the coverage contract".to_owned()
        }
    })
}
