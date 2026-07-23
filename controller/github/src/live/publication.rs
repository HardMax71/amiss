use amiss_controller::{CheckConclusion, IntegrationId, ProviderError, Publication};
use amiss_wire::digest::sha256;
use amiss_wire::model::{ForgeDialect, ObjectFormat};

use super::Config;
use super::model::{CheckRunRecord, CreateCheckRun, CreateCheckRunOutput};
use crate::GitHubPullRequest;

const COMPLETED: &str = "completed";
const TITLE: &str = "Amiss provider verification";

pub(super) enum PublicationDecision {
    Reuse,
    Create(CreateCheckRun),
}

pub(super) fn validate_publication(
    config: &Config,
    pull_request: GitHubPullRequest<'_>,
    publication: &Publication,
) -> Result<(), ProviderError> {
    let integration = IntegrationId::new(config.installation_id.to_string())
        .ok_or(ProviderError::InvalidResponse)?;
    let expected_run = crate::provider_run(
        &integration,
        pull_request.change,
        pull_request.candidate_commit,
        &publication.run.refs.candidate,
        &publication.run.refs.target,
    )
    .ok_or(ProviderError::InvalidResponse)?;
    let event_bound = publication.provider_run == expected_run;
    let exact_gate = amiss_wire::model::Oid::new(
        ObjectFormat::Sha1,
        publication.gate_commit.as_str().to_owned(),
    )
    .as_ref()
        == Some(&publication.gate_commit);
    let exact = exact_gate
        && publication.provider_run.candidate_commit == *pull_request.candidate_commit
        && publication.run.change == *pull_request.change
        && publication.run.object_format == ObjectFormat::Sha1
        && publication.run.refs.forge == ForgeDialect::Github
        && publication.run.commits.candidate == *pull_request.candidate_commit
        && publication.check.required_status_name == config.required_status_name
        && (event_bound || matches!(publication.conclusion, CheckConclusion::Superseded));
    exact.then_some(()).ok_or(ProviderError::InvalidResponse)
}

pub(super) fn publication_decision(
    config: &Config,
    publication: &Publication,
    runs: &[CheckRunRecord],
) -> Result<PublicationDecision, ProviderError> {
    let expected = expected(config, publication)?;
    let mut matching = None;
    for run in runs {
        let app = run.app.as_ref().ok_or(ProviderError::InvalidResponse)?;
        if app.id != config.app_id {
            continue;
        }
        if run.id == 0 || run.name != expected.name || run.head_sha != expected.head_sha {
            return Err(ProviderError::InvalidResponse);
        }
        if run.external_id.as_deref() != Some(expected.external_id.as_str()) {
            continue;
        }
        if matching.replace(run).is_some() {
            return Err(ProviderError::InvalidResponse);
        }
    }

    match matching {
        Some(run) if matches_expected(run, &expected) => Ok(PublicationDecision::Reuse),
        Some(_) => Err(ProviderError::InvalidResponse),
        None => Ok(PublicationDecision::Create(expected)),
    }
}

pub(super) fn validate_created(
    config: &Config,
    expected: &CreateCheckRun,
    created: &CheckRunRecord,
) -> Result<(), ProviderError> {
    let own_app = created.app.as_ref().map(|app| app.id) == Some(config.app_id);
    if created.id == 0 || !own_app || !matches_expected(created, expected) {
        return Err(ProviderError::InvalidResponse);
    }
    Ok(())
}

fn expected(config: &Config, publication: &Publication) -> Result<CreateCheckRun, ProviderError> {
    if publication.check.required_status_name != config.required_status_name {
        return Err(ProviderError::InvalidResponse);
    }
    let (label, conclusion) = conclusion(publication.conclusion);
    let failure = provider_failure(publication.conclusion)?
        .map(|failure| format!("\nfailure: {failure}"))
        .unwrap_or_default();
    let report_digest = sha256(publication.report.as_deref().unwrap_or_default());
    let run = &publication.run;
    let repository = &run.change.repository;
    let summary = format!(
        "evaluation: {}\nconclusion: {label}{failure}\nprovider: {}/{}\nrepository: {}/{}/{}\nchange: {}\nprovider-run: {}#{}\ngate-commit: {}\ncandidate-ref: {}\ntarget-ref: {}\ndefault-ref: {}\nbase-commit: {}\nbase-tree: {}\ncandidate-commit: {}\ncandidate-tree: {}\nplan: {}\nconstraint: {}\nreport: {report_digest}",
        publication.evaluation_id,
        run.change.provider.namespace,
        run.change.provider.instance,
        repository.host,
        repository.owner,
        repository.name,
        run.change.change,
        publication.provider_run.run_id,
        publication.provider_run.attempt.get(),
        publication.gate_commit.as_str(),
        run.refs.candidate.as_str(),
        run.refs.target.as_str(),
        run.refs.default_branch.as_str(),
        run.commits.base.as_str(),
        run.trees.base.as_str(),
        run.commits.candidate.as_str(),
        run.trees.candidate.as_str(),
        publication.check.plan_digest,
        publication.check.execution_constraint_digest,
    );
    Ok(CreateCheckRun {
        name: config.required_status_name.clone(),
        head_sha: publication.gate_commit.as_str().to_owned(),
        external_id: publication.evaluation_id.as_str().to_owned(),
        status: COMPLETED,
        conclusion: conclusion.to_owned(),
        output: CreateCheckRunOutput {
            title: TITLE.to_owned(),
            summary,
        },
    })
}

fn provider_failure(conclusion: CheckConclusion) -> Result<Option<String>, ProviderError> {
    let CheckConclusion::Unavailable(failure) = conclusion else {
        return Ok(None);
    };
    serde_json::to_value(failure)
        .map_err(|_defect| ProviderError::InvalidResponse)?
        .as_str()
        .map(str::to_owned)
        .map(Some)
        .ok_or(ProviderError::InvalidResponse)
}

fn conclusion(conclusion: CheckConclusion) -> (&'static str, &'static str) {
    match conclusion {
        CheckConclusion::Pass => ("pass", "success"),
        CheckConclusion::Block => ("block", "failure"),
        CheckConclusion::Superseded => ("superseded", "cancelled"),
        CheckConclusion::Unavailable(_) => ("unavailable", "failure"),
    }
}

fn matches_expected(run: &CheckRunRecord, expected: &CreateCheckRun) -> bool {
    run.name == expected.name
        && run.head_sha == expected.head_sha
        && run.external_id.as_deref() == Some(expected.external_id.as_str())
        && run.status == expected.status
        && run.conclusion.as_deref() == Some(expected.conclusion.as_str())
        && run.output.title.as_deref() == Some(expected.output.title.as_str())
        && run.output.summary.as_deref() == Some(expected.output.summary.as_str())
}
