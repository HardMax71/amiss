use amiss_controller::ProviderError;
use serde::de::DeserializeOwned;

use crate::identity::{canonical_project_path, exact_sha1, repository_url};
use crate::{
    GitLabApi, GitLabBranch, GitLabJob, GitLabMergeRequest, GitLabObjects, GitLabPipeline,
    GitLabProject, GitLabProtection, GitLabRefresh, GitLabRefreshQuery, GitLabTrainCar,
};

use super::GitLabClient;
use super::model::{
    self, BranchResponse, CommitResponse, JobResponse, PipelineResponse, ProjectResponse,
    TrainResponse,
};
use super::transport::Budget;

struct ChangeData {
    merge_request: GitLabMergeRequest,
    project: GitLabProject,
    target: GitLabBranch,
    protections: Vec<GitLabProtection>,
}

impl GitLabApi for GitLabClient {
    fn refresh(&self, query: &GitLabRefreshQuery) -> Result<GitLabRefresh, ProviderError> {
        validate_query(query)?;
        let project_id = query.project_id.to_string();
        let budget = self.transport.budget()?;
        let (job, pipeline, budget) = self.job_pipeline(&project_id, query.job_id, budget)?;
        let (train, budget) = self.train_car(&project_id, query.merge_request_iid, budget)?;
        let (change, budget) = self.change_data(query, &project_id, budget)?;
        let objects =
            self.resolve_objects(query, &project_id, &pipeline, &change.project, budget)?;
        Ok(GitLabRefresh {
            project: change.project,
            job,
            pipeline,
            train,
            merge_request: change.merge_request,
            target: change.target,
            gate: objects.gate,
            base: objects.base,
            protections: change.protections,
        })
    }
}

impl GitLabClient {
    fn train_car(
        &self,
        project_id: &str,
        merge_request_iid: u64,
        budget: Budget,
    ) -> Result<(Option<GitLabTrainCar>, Budget), ProviderError> {
        let (train, budget) = self.transport.get_optional::<TrainResponse>(
            self.endpoint(
                project_id,
                [
                    "merge_trains".to_owned(),
                    "merge_requests".to_owned(),
                    merge_request_iid.to_string(),
                ],
            )?,
            budget,
        )?;
        Ok((train.map(model::train).transpose()?, budget))
    }

    fn job_pipeline(
        &self,
        project_id: &str,
        job_id: u64,
        budget: Budget,
    ) -> Result<(GitLabJob, GitLabPipeline, Budget), ProviderError> {
        let (job, budget) = self.fetch_project::<JobResponse>(
            project_id,
            ["jobs".to_owned(), job_id.to_string()],
            budget,
        )?;
        let job = model::job(job)?;
        let (pipeline, budget) = self.fetch_project::<PipelineResponse>(
            project_id,
            ["pipelines".to_owned(), job.pipeline_id.to_string()],
            budget,
        )?;
        Ok((job, model::pipeline(pipeline), budget))
    }

    fn change_data(
        &self,
        query: &GitLabRefreshQuery,
        project_id: &str,
        budget: Budget,
    ) -> Result<(ChangeData, Budget), ProviderError> {
        let (merge_request, budget) = self.fetch_project::<GitLabMergeRequest>(
            project_id,
            [
                "merge_requests".to_owned(),
                query.merge_request_iid.to_string(),
            ],
            budget,
        )?;
        let (project, budget) = self.fetch_project::<ProjectResponse>(project_id, [], budget)?;
        let project = model::project(project);
        let (target, protections, budget) =
            self.target_data(project_id, &merge_request.target_branch, budget)?;
        Ok((
            ChangeData {
                merge_request,
                project,
                target,
                protections,
            },
            budget,
        ))
    }

    fn target_data(
        &self,
        project_id: &str,
        target_branch: &str,
        budget: Budget,
    ) -> Result<(GitLabBranch, Vec<GitLabProtection>, Budget), ProviderError> {
        let (target, budget) = self.fetch_project::<BranchResponse>(
            project_id,
            [
                "repository".to_owned(),
                "branches".to_owned(),
                target_branch.to_owned(),
            ],
            budget,
        )?;
        let (protections, budget) = self.protections(project_id, budget)?;
        Ok((model::branch(target), protections, budget))
    }

    fn resolve_objects(
        &self,
        query: &GitLabRefreshQuery,
        project_id: &str,
        pipeline: &GitLabPipeline,
        project: &GitLabProject,
        budget: Budget,
    ) -> Result<GitLabObjects, ProviderError> {
        let repository_url = validated_repository_url(
            self.transport.provider_instance(),
            query.project_id,
            project.id,
            &project.path_with_namespace,
            &project.http_url_to_repo,
        )?;
        let gate_commit = exact_sha1(&pipeline.sha).ok_or(ProviderError::InvalidResponse)?;
        let (commit, budget) = self.fetch_project::<CommitResponse>(
            project_id,
            [
                "repository".to_owned(),
                "commits".to_owned(),
                gate_commit.as_str().to_owned(),
            ],
            budget,
        )?;
        let base_commit = commit
            .parent_ids
            .first()
            .and_then(|parent| exact_sha1(parent))
            .ok_or(ProviderError::InvalidResponse)?;
        if commit.id != gate_commit.as_str() {
            return Err(ProviderError::InvalidResponse);
        }
        let objects = self.objects.resolve(&crate::GitLabObjectRequest {
            project_id: query.project_id,
            repository_url,
            gate_commit,
            base_commit,
            timeout: budget.remaining()?,
        })?;
        budget.remaining()?;
        let resolved_base = objects
            .gate
            .parents
            .first()
            .ok_or(ProviderError::InvalidResponse)?;
        if objects.gate.id != commit.id
            || objects.gate.parents != commit.parent_ids
            || &objects.base.id != resolved_base
        {
            return Err(ProviderError::InvalidResponse);
        }
        Ok(objects)
    }

    fn fetch_project<T: DeserializeOwned>(
        &self,
        project_id: &str,
        tail: impl IntoIterator<Item = String>,
        budget: Budget,
    ) -> Result<(T, Budget), ProviderError> {
        self.transport.get(self.endpoint(project_id, tail)?, budget)
    }
}

pub(super) fn validated_repository_url(
    provider_instance: &str,
    expected_project_id: u64,
    project_id: u64,
    project_path: &str,
    reported_url: &str,
) -> Result<String, ProviderError> {
    let project_path =
        canonical_project_path(project_path).ok_or(ProviderError::InvalidResponse)?;
    let canonical =
        repository_url(provider_instance, &project_path).ok_or(ProviderError::InvalidResponse)?;
    (project_id == expected_project_id && reported_url == canonical)
        .then_some(canonical)
        .ok_or(ProviderError::InvalidResponse)
}

fn validate_query(query: &GitLabRefreshQuery) -> Result<(), ProviderError> {
    let valid = query.project_id > 0
        && query.merge_request_iid > 0
        && query.pipeline_id > 0
        && query.job_id > 0
        && query.runner_id > 0
        && exact_sha1(query.gate_commit.as_str()).as_ref() == Some(&query.gate_commit);
    valid.then_some(()).ok_or(ProviderError::InvalidResponse)
}
