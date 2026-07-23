use amiss_controller::{
    AuthenticatedDelivery, ChangeSnapshot, CheckBinding, CheckConclusion, ControllerEvaluationId,
    Publication,
};
use amiss_controller_gitlab::{
    GitLabAccess, GitLabBranch, GitLabCommit, GitLabJob, GitLabMergeChecks, GitLabMergeRequest,
    GitLabPipeline, GitLabProject, GitLabProtection, GitLabRefresh, GitLabTrainCar,
    GitLabTrainSettings,
};
use amiss_wire::digest::hb;

use super::identity::{HOST, PROJECT_PATH, oid};

pub fn valid_refresh(delivery: &AuthenticatedDelivery) -> GitLabRefresh {
    let gate = delivery.provider_run.candidate_commit.as_str().to_owned();
    GitLabRefresh {
        project: GitLabProject {
            id: 101,
            path_with_namespace: PROJECT_PATH.to_owned(),
            default_branch: "main".to_owned(),
            http_url_to_repo: format!("https://{HOST}/{PROJECT_PATH}.git"),
            repository_object_format: "sha1".to_owned(),
            checks: GitLabMergeChecks {
                pipeline_must_succeed: true,
                skipped_pipeline_allowed: false,
                merged_results_enabled: true,
            },
            train: GitLabTrainSettings {
                enabled: true,
                skip_allowed: false,
                enforcement: "enforce_for_all_users".to_owned(),
            },
            merge_method: "merge".to_owned(),
            squash_option: "never".to_owned(),
        },
        job: GitLabJob {
            id: 303,
            name: "amiss:policy".to_owned(),
            status: "running".to_owned(),
            source: "pipeline_execution_policy".to_owned(),
            pipeline_id: 202,
            commit: gate.clone(),
            runner_id: 77,
        },
        pipeline: GitLabPipeline {
            id: 202,
            project_id: 101,
            sha: gate.clone(),
            reference: "refs/merge-requests/42/train".to_owned(),
            source: "merge_request_event".to_owned(),
            status: "running".to_owned(),
        },
        train: Some(GitLabTrainCar {
            id: 404,
            status: "fresh".to_owned(),
            target_branch: "main".to_owned(),
            merge_request_iid: 42,
            merge_request_project_id: 101,
            merge_request_state: "opened".to_owned(),
            pipeline_id: 202,
            pipeline_project_id: 101,
            pipeline_sha: gate.clone(),
            pipeline_ref: "refs/merge-requests/42/train".to_owned(),
            pipeline_source: "merge_request_event".to_owned(),
            pipeline_status: "running".to_owned(),
        }),
        merge_request: GitLabMergeRequest {
            iid: 42,
            project_id: 101,
            state: "opened".to_owned(),
            draft: false,
            source_project_id: 101,
            target_project_id: 101,
            source_branch: "topic".to_owned(),
            target_branch: "main".to_owned(),
            sha: oid('c').as_str().to_owned(),
            detailed_merge_status: "ci_still_running".to_owned(),
            squash_on_merge: false,
        },
        target: GitLabBranch {
            name: "main".to_owned(),
            commit: oid('d').as_str().to_owned(),
        },
        gate: GitLabCommit {
            id: gate,
            tree: oid('e').as_str().to_owned(),
            parents: vec![oid('a').as_str().to_owned(), oid('c').as_str().to_owned()],
        },
        base: GitLabCommit {
            id: oid('a').as_str().to_owned(),
            tree: oid('f').as_str().to_owned(),
            parents: Vec::new(),
        },
        protections: vec![GitLabProtection {
            name: "main".to_owned(),
            allow_force_push: false,
            push_access_levels: vec![GitLabAccess {
                access_level: 0,
                user_id: None,
                group_id: None,
                deploy_key_id: None,
                member_role_id: None,
            }],
        }],
    }
}

pub fn publication(
    delivery: &AuthenticatedDelivery,
    snapshot: &ChangeSnapshot,
    conclusion: CheckConclusion,
) -> Publication {
    let digest = hb("amiss/controller-gitlab-test", b"fixture");
    Publication {
        provider_run: delivery.provider_run.clone(),
        evaluation_id: ControllerEvaluationId::new("evaluation/1".to_owned()).unwrap(),
        check: CheckBinding {
            plan_digest: digest,
            required_status_name: "amiss".to_owned(),
            execution_constraint_digest: digest,
        },
        run: snapshot.run.clone(),
        gate_commit: snapshot.gate_commit.clone(),
        conclusion,
        report: Some(br#"{"schema":"amiss/report"}"#.to_vec()),
    }
}
