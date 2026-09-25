use std::path::Path;
use std::process::ExitCode;

use amiss_git::{GitLimits, GitResources, Repository};
use amiss_scan::{LOCALE_CONTEXT_BYTES, LocaleTreeContext, tree_inventory};
use amiss_wire::de::Document as _;
use amiss_wire::envelope::Payload as _;
use amiss_wire::locale::{
    EVIDENCE_DOCUMENT_BYTES, LOCALE_DOCUMENT_BYTES, LocaleCoverageAssessment,
    LocaleCoverageEvidence, LocaleCoveragePlan, LocaleFallbackRule, LocalePageRequirement, assess,
};
use amiss_wire::model::Digest;

use crate::input::{ReadError, bounded_bytes};
use crate::invocation::{AssessInvocation, InventoryInvocation, LocalePlanInvocation};

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

/// Writes the coverage plan an audit of one report's candidate needs, so no
/// digest is sealed by hand: the report fixes the docs candidate and its
/// identity, the locale layout fixes the producer and both locales, and the
/// flags fix the scope and the policy.
pub(crate) fn run_plan(invocation: &LocalePlanInvocation) -> ExitCode {
    crate::pure::run(
        "locale-plan",
        invocation.format,
        || {
            Ok((
                crate::input::report_bytes(&invocation.report)?,
                document(
                    &invocation.context,
                    LOCALE_CONTEXT_BYTES,
                    "a locale context",
                )?,
            ))
        },
        |(report, context), _version, _digest| planned(invocation, &report, &context),
        |bytes| {
            LocaleCoveragePlan::parse(bytes)
                .map(|document| crate::human::locale_plan(&document))
                .map_err(|defect| defect.to_string())
        },
    )
}

fn planned(
    invocation: &LocalePlanInvocation,
    report: &[u8],
    context: &[u8],
) -> Result<Vec<u8>, String> {
    use amiss_wire::report::model::{Evaluation, IdentityPreimage, ReportPayload, Snapshot};
    use amiss_wire::requests::{
        CANDIDATE_IDENTITY_DOMAIN, CandidateIdentitySchema, CandidateSnapshot,
    };

    let context = LocaleTreeContext::parse(context)
        .map_err(|defect| format!("the context is invalid: {defect}"))?;
    let parsed = <ReportPayload>::parse(report).map_err(|defect| defect.to_string())?;
    if !parsed.payload.result.complete {
        return Err("the report is incomplete, so no audit can bind it".to_owned());
    }
    let Evaluation::Resolved(evaluation) = &parsed.payload.evaluation else {
        return Err("the report names no evaluated candidate".to_owned());
    };
    let Snapshot::Available(CandidateSnapshot::Git(candidate)) = &evaluation.candidate else {
        return Err("the report's candidate is not a commit, so no tree can be walked".to_owned());
    };
    let repository = evaluation
        .repository
        .clone()
        .ok_or_else(|| "the report names no repository; run check with --repository".to_owned())?;
    let candidate_identity_digest = amiss_wire::envelope::document_digest(
        CANDIDATE_IDENTITY_DOMAIN,
        &IdentityPreimage {
            evaluation,
            schema: CandidateIdentitySchema::Current,
        },
    )
    .ok_or_else(|| "the report's candidate identity cannot be spelled".to_owned())?;
    let producer = amiss_scan::tree_producer(&context)
        .map_err(|_defect| "the locale context cannot be spelled".to_owned())?;
    let required = LocalePageRequirement::AllSource {};
    let fallbacks: Vec<LocaleFallbackRule> = invocation
        .fallback
        .iter()
        .map(|class| LocaleFallbackRule {
            class: class.clone(),
            pages: LocalePageRequirement::AllSource {},
        })
        .collect();
    let context_digest = amiss_wire::envelope::document_digest(
        PLAN_POLICY_DOMAIN,
        &(&required, &fallbacks, invocation.require_lineage),
    )
    .ok_or_else(|| "the policy cannot be spelled".to_owned())?;
    let plan = LocaleCoveragePlan {
        schema: amiss_wire::locale::PlanPayloadSchema::Current,
        report_payload_digest: parsed.payload_digest,
        docs: amiss_wire::publication::DocsCandidate {
            repository,
            object_format: candidate.object_format,
            commit: candidate.commit_oid.clone(),
            tree: candidate.tree_oid.clone(),
            candidate_identity_digest,
        },
        scope: amiss_wire::locale::LocaleCoverageScope {
            site: invocation.site.clone(),
            source_locale: context.source.locale.clone(),
            target_locale: context.target.locale,
            channel: invocation.channel.clone(),
            version: invocation.version.clone().map_or(
                amiss_wire::assessment::Nullable::Null,
                amiss_wire::assessment::Nullable::Value,
            ),
        },
        product: amiss_wire::assessment::Nullable::Null,
        producer,
        policy: amiss_wire::locale::LocaleCoveragePolicy {
            identity: PLAN_POLICY,
            context_digest,
            required,
            fallbacks,
            require_target_lineage: invocation.require_lineage,
        },
    };
    plan.emit().map_err(|defect| defect.to_string())
}

/// The coverage policy a plan this verb writes names, with its context digest
/// taken over the rules the flags chose.
const PLAN_POLICY: amiss_wire::model::ArtifactId = amiss_wire::artifact_id!("amiss-locale-plan");
const PLAN_POLICY_DOMAIN: &str = "amiss/locale-plan-policy-v1";
