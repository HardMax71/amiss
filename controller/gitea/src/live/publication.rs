use amiss_controller::{ChangeState, CheckConclusion, IntegrationId, ProviderError, Publication};
use amiss_wire::digest::sha256;
use amiss_wire::model::{ForgeDialect, ObjectFormat};

use crate::GiteaPullRequest;
use crate::identity::provider_run;

use super::Config;
use super::model::{CreateReview, ReviewRecord};

const APPROVED: &str = "APPROVED";
const REQUEST_CHANGES: &str = "REQUEST_CHANGES";
const MARKER: &str = "amiss-evaluation: ";

pub(super) enum PublicationDecision {
    Reuse,
    Create(CreateReview),
}

pub(super) fn validate_publication(
    config: &Config,
    pull_request: GiteaPullRequest<'_>,
    publication: &Publication,
) -> Result<(), ProviderError> {
    let integration =
        IntegrationId::new(config.reviewer.id.to_string()).ok_or(ProviderError::InvalidResponse)?;
    let expected_run = provider_run(
        &integration,
        pull_request.change,
        pull_request.candidate_commit,
        &publication.run.refs.candidate,
        &publication.run.refs.target,
    )
    .ok_or(ProviderError::InvalidResponse)?;
    let exact_gate = publication.gate_commit == *pull_request.candidate_commit;
    let exact = exact_gate
        && publication.provider_run == expected_run
        && publication.provider_run.candidate_commit == *pull_request.candidate_commit
        && publication.run.change == *pull_request.change
        && publication.run.object_format == ObjectFormat::Sha1
        && publication.run.refs.forge == ForgeDialect::Gitea
        && publication.run.commits.candidate == *pull_request.candidate_commit
        && publication.check.required_status_name == config.review_name;
    exact.then_some(()).ok_or(ProviderError::InvalidResponse)
}

pub(super) fn publication_decision(
    config: &Config,
    publication: &Publication,
    reviews: &[ReviewRecord],
) -> Result<PublicationDecision, ProviderError> {
    let expected = expected(publication)?;
    let latest = reviews
        .iter()
        .filter(|review| {
            review
                .user
                .as_ref()
                .is_some_and(|user| user.id == config.reviewer.id)
                && review.commit_id == expected.commit_id
                && !review.stale
                && !review.dismissed
        })
        .max_by_key(|review| review.id);
    match latest {
        Some(review) if matches_expected(review, &expected, config) => {
            Ok(PublicationDecision::Reuse)
        }
        Some(review)
            if review
                .body
                .lines()
                .any(|line| line == format!("{MARKER}{}", publication.evaluation_id)) =>
        {
            Err(ProviderError::InvalidResponse)
        }
        Some(_) | None => Ok(PublicationDecision::Create(expected)),
    }
}

pub(super) fn validate_created(
    config: &Config,
    expected: &CreateReview,
    created: &ReviewRecord,
) -> Result<(), ProviderError> {
    (matches_expected(created, expected, config)
        && created.id > 0
        && !created.stale
        && !created.dismissed)
        .then_some(())
        .ok_or(ProviderError::InvalidResponse)
}

pub(super) fn publishable(state: ChangeState) -> Result<bool, ProviderError> {
    match state {
        ChangeState::Active => Ok(true),
        ChangeState::Superseded | ChangeState::Closed => Ok(false),
        ChangeState::AuthorizationRevoked => Err(ProviderError::AuthorizationRevoked),
    }
}

fn expected(publication: &Publication) -> Result<CreateReview, ProviderError> {
    let (label, event) = conclusion(publication.conclusion);
    let failure = provider_failure(publication.conclusion)?
        .map(|failure| format!("\nfailure: {failure}"))
        .unwrap_or_default();
    let report_digest = sha256(publication.report.as_deref().unwrap_or_default());
    let run = &publication.run;
    let repository = &run.change.repository;
    let body = format!(
        "{MARKER}{}\nconclusion: {label}{failure}\nprovider: {}/{}\nrepository: {}/{}/{}\nchange: {}\nprovider-run: {}#{}\ngate-commit: {}\ncandidate-ref: {}\ntarget-ref: {}\ndefault-ref: {}\nbase-commit: {}\nbase-tree: {}\ncandidate-commit: {}\ncandidate-tree: {}\nplan: {}\nconstraint: {}\nreport: {report_digest}",
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
    Ok(CreateReview {
        event: event.to_owned(),
        body,
        commit_id: publication.gate_commit.as_str().to_owned(),
        comments: Vec::new(),
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
        CheckConclusion::Pass => ("pass", APPROVED),
        CheckConclusion::Block => ("block", REQUEST_CHANGES),
        CheckConclusion::Superseded => ("superseded", REQUEST_CHANGES),
        CheckConclusion::Unavailable(_) => ("unavailable", REQUEST_CHANGES),
    }
}

fn matches_expected(review: &ReviewRecord, expected: &CreateReview, config: &Config) -> bool {
    review.user.as_ref().is_some_and(|user| {
        user.id == config.reviewer.id && user.login.eq_ignore_ascii_case(&config.reviewer.login)
    }) && review.state == expected.event
        && review.body == expected.body
        && review.commit_id == expected.commit_id
}
